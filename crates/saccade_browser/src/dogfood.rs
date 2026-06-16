use std::cell::{Cell, RefCell};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use euclid::{Point2D, Scale};
use servo::{
    CSSPixel, EmbedderControl, InputEvent, JSValue, Key as ServoKey, KeyState, KeyboardEvent,
    LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey as ServoNamedKey, Opts, RenderingContext, SelectElement,
    SelectElementOptionOrOptgroup, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate,
    WebViewPoint, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{
    ElementState, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::browser_session_worker::{
    FORMMAX_LIVE_FILL_JS, PROBE_JS, action_enabled, action_id_for, action_map,
    fill_agent_fields_script, inspect_fields_script, probe_changed, sensitive_action_count,
};
use crate::{RenderingProfile, RenderingProfileSettings};
use serde::Deserialize;
use serde_json::{Value, json};

const DEFAULT_WIDTH: u32 = 1440;
const DEFAULT_HEIGHT: u32 = 1000;
const FORMMAX_CONTROL_TIMEOUT: Duration = Duration::from_secs(65);

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct DogfoodBrowserConfig {
    pub url: Url,
    pub width: u32,
    pub height: u32,
    pub auto_close_after: Option<Duration>,
    pub rendering_profile: Option<RenderingProfile>,
    pub profile_dir: Option<PathBuf>,
    pub copilot_grant_path: Option<PathBuf>,
    pub auto_grant_copilot: bool,
}

impl DogfoodBrowserConfig {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            auto_close_after: None,
            rendering_profile: None,
            profile_dir: None,
            copilot_grant_path: None,
            auto_grant_copilot: false,
        }
    }
}

pub fn run_dogfood_browser(config: DogfoodBrowserConfig) -> Result<()> {
    let rendering_settings = RenderingProfile::resolve_with_default(
        config.rendering_profile,
        RenderingProfile::ServoModern,
    )?;
    if rendering_settings.profile == RenderingProfile::ChromeReference {
        anyhow::bail!(
            "chrome-reference is a configuration stub; use the Chrome reference capture path for UI parity"
        );
    }
    if let Some(profile_dir) = config.profile_dir.as_ref() {
        std::fs::create_dir_all(profile_dir)
            .with_context(|| format!("failed to create profile dir {}", profile_dir.display()))?;
    }
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let control_bridge = start_dogfood_control_bridge(&event_loop)?;
    let mut app = DogfoodBrowserApp::new(&event_loop, config, rendering_settings, control_bridge);

    event_loop
        .run_app(&mut app)
        .context("dogfood browser event loop failed")
}

fn start_dogfood_control_bridge(
    event_loop: &EventLoop<WakerEvent>,
) -> Result<DogfoodControlBridge> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to bind dogfood control listener")?;
    let addr = listener
        .local_addr()
        .context("failed to read dogfood control listener address")?;
    let endpoint = DogfoodControlEndpoint { addr };
    let (tx, rx) = mpsc::channel::<DogfoodControlCommand>();
    let proxy = event_loop.create_proxy();
    thread::spawn(move || accept_dogfood_control(listener, tx, proxy));
    Ok(DogfoodControlBridge { endpoint, rx })
}

fn accept_dogfood_control(
    listener: TcpListener,
    tx: Sender<DogfoodControlCommand>,
    proxy: EventLoopProxy<WakerEvent>,
) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let tx = tx.clone();
        let proxy = proxy.clone();
        thread::spawn(move || handle_dogfood_control_stream(stream, tx, proxy));
    }
}

fn handle_dogfood_control_stream(
    mut stream: TcpStream,
    tx: Sender<DogfoodControlCommand>,
    proxy: EventLoopProxy<WakerEvent>,
) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let reader = BufReader::new(reader_stream);
    for line in reader.lines() {
        let line = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => line,
            Err(error) => {
                let _ = writeln!(
                    stream,
                    "{}",
                    json!({"id": Value::Null, "ok": false, "error": error.to_string()})
                );
                let _ = stream.flush();
                return;
            }
        };
        let (respond_to, response_rx) = mpsc::channel();
        let request =
            serde_json::from_str::<DogfoodControlRequest>(&line).map_err(|error| error.to_string());
        let timeout = request
            .as_ref()
            .ok()
            .and_then(|request| {
                (request.method == "formmax_live_fill").then_some(FORMMAX_CONTROL_TIMEOUT)
            })
            .unwrap_or_else(|| Duration::from_secs(5));
        if tx
            .send(DogfoodControlCommand {
                request,
                respond_to,
            })
            .is_err()
        {
            return;
        }
        let _ = proxy.send_event(WakerEvent);
        match response_rx.recv_timeout(timeout) {
            Ok(response) => {
                let _ = writeln!(stream, "{response}");
                let _ = stream.flush();
            }
            Err(error) => {
                let _ = writeln!(
                    stream,
                    "{}",
                    json!({"id": Value::Null, "ok": false, "error": format!("dogfood control response timeout: {error}")})
                );
                let _ = stream.flush();
                return;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct SelectChoice {
    id: usize,
    label: String,
    disabled: bool,
}

struct ActiveSelect {
    control: SelectElement,
    choices: Vec<SelectChoice>,
    cursor: usize,
}

#[derive(Debug, Clone)]
struct DogfoodControlEndpoint {
    addr: SocketAddr,
}

struct DogfoodControlBridge {
    endpoint: DogfoodControlEndpoint,
    rx: Receiver<DogfoodControlCommand>,
}

#[derive(Debug)]
struct DogfoodControlCommand {
    request: std::result::Result<DogfoodControlRequest, String>,
    respond_to: Sender<Value>,
}

#[derive(Debug, Deserialize)]
struct DogfoodControlRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Clone, Copy)]
enum DogfoodControlProbeMethod {
    Truth,
    Actions,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BrowserLoadState {
    Starting,
    Loading,
    HeadParsed,
    Complete,
}

impl BrowserLoadState {
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Loading => "loading",
            Self::HeadParsed => "head",
            Self::Complete => "complete",
        }
    }
}

impl From<LoadStatus> for BrowserLoadState {
    fn from(status: LoadStatus) -> Self {
        match status {
            LoadStatus::Started => Self::Loading,
            LoadStatus::HeadParsed => Self::HeadParsed,
            LoadStatus::Complete => Self::Complete,
        }
    }
}

struct ShellTitleParts<'a> {
    profile: &'a str,
    load_state: BrowserLoadState,
    can_go_back: bool,
    can_go_forward: bool,
    page_title: Option<&'a str>,
    current_url: &'a str,
    copilot_label: &'a str,
    address_entry: Option<AddressEntryTitle<'a>>,
    active_select_label: Option<&'a str>,
}

struct AddressEntryTitle<'a> {
    input: &'a str,
    invalid: bool,
}

fn format_shell_title(parts: ShellTitleParts<'_>) -> String {
    let back = if parts.can_go_back { "y" } else { "n" };
    let forward = if parts.can_go_forward { "y" } else { "n" };

    if let Some(address_entry) = parts.address_entry {
        let mode = if address_entry.invalid {
            "location invalid>"
        } else {
            "location>"
        };
        return format!(
            "Saccade [{}] copilot={} {mode} {} | Enter open Esc cancel | {}",
            parts.profile, parts.copilot_label, address_entry.input, parts.current_url
        );
    }

    if let Some(label) = parts.active_select_label {
        return format!(
            "Saccade [{}] copilot={} select={label} | back={back} fwd={forward} reload=Cmd+R | {}",
            parts.profile, parts.copilot_label, parts.current_url
        );
    }

    let title = parts
        .page_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(parts.current_url);
    format!(
        "Saccade [{}] copilot={} load={} back={back} fwd={forward} | {title} | {}",
        parts.profile,
        parts.copilot_label,
        parts.load_state.label(),
        parts.current_url
    )
}

struct DogfoodBrowserState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    url: Url,
    current_url: RefCell<Url>,
    webview: RefCell<Option<WebView>>,
    cursor_x: Cell<f32>,
    cursor_y: Cell<f32>,
    cursor_move_count: Cell<u64>,
    last_cursor_move_at: Cell<Option<Instant>>,
    modifiers: Cell<ModifiersState>,
    load_state: Cell<BrowserLoadState>,
    page_title: RefCell<Option<String>>,
    address_entry: RefCell<Option<String>>,
    address_error: Cell<bool>,
    copilot_granted: Cell<bool>,
    copilot_grant_error: RefCell<Option<String>>,
    copilot_grant_path: Option<PathBuf>,
    control_page_revision: Cell<u64>,
    control_endpoint: DogfoodControlEndpoint,
    control_rx: Receiver<DogfoodControlCommand>,
    active_select: RefCell<Option<ActiveSelect>>,
    started_at: Instant,
    auto_close_after: Option<Duration>,
    rendering_settings: RenderingProfileSettings,
    pointer_trace: bool,
}

impl DogfoodBrowserState {
    fn store_cursor_page_position(&self, position: PhysicalPosition<f64>) {
        let logical = position.to_logical::<f64>(self.window.scale_factor());
        self.cursor_x.set(logical.x as f32);
        self.cursor_y.set(logical.y as f32);
    }

    fn page_point(&self) -> WebViewPoint {
        WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(
            self.cursor_x.get(),
            self.cursor_y.get(),
        ))
    }

    fn update_window_title(&self) {
        let active_select_label = self.active_select.borrow().as_ref().map(|active| {
            active
                .choices
                .get(active.cursor)
                .map(|choice| choice.label.as_str())
                .unwrap_or("(no selectable option)")
                .to_string()
        });
        let page_title = self.page_title.borrow().clone();
        let current_url = self.current_url.borrow().to_string();
        let copilot_label = self.copilot_title_label();
        let address_entry = self.address_entry.borrow().clone();
        let (can_go_back, can_go_forward) = self
            .webview
            .borrow()
            .as_ref()
            .map(|webview| (webview.can_go_back(), webview.can_go_forward()))
            .unwrap_or((false, false));

        self.window.set_title(&format_shell_title(ShellTitleParts {
            profile: self.rendering_settings.profile.name(),
            load_state: self.load_state.get(),
            can_go_back,
            can_go_forward,
            page_title: page_title.as_deref(),
            current_url: current_url.as_str(),
            copilot_label: copilot_label.as_str(),
            address_entry: address_entry.as_deref().map(|input| AddressEntryTitle {
                input,
                invalid: self.address_error.get(),
            }),
            active_select_label: active_select_label.as_deref(),
        }));
    }

    fn copilot_title_label(&self) -> String {
        if self.copilot_grant_error.borrow().is_some() {
            "error".into()
        } else if self.copilot_granted.get() {
            "granted".into()
        } else {
            "off Cmd+Shift+G".into()
        }
    }

    fn trace_cursor_moved(&self, position: PhysicalPosition<f64>) {
        if !self.pointer_trace {
            return;
        }
        let scale = self.window.scale_factor();
        let logical = position.to_logical::<f64>(scale);
        let inner = self.window.inner_size();
        eprintln!(
            "SACCADE_POINTER_TRACE runtime=dogfood event=cursor_moved raw_physical=({:.1},{:.1}) logical_if_css=({:.1},{:.1}) stored_page=({:.1},{:.1}) hidpi={:.3} inner_device={}x{} move_count={}",
            position.x,
            position.y,
            logical.x,
            logical.y,
            self.cursor_x.get(),
            self.cursor_y.get(),
            scale,
            inner.width,
            inner.height,
            self.cursor_move_count.get(),
        );
    }

    fn trace_pointer_event(&self, event: &str, detail: std::fmt::Arguments<'_>) {
        if !self.pointer_trace {
            return;
        }
        let age_ms = self
            .last_cursor_move_at
            .get()
            .map(|instant| instant.elapsed().as_millis());
        eprintln!(
            "SACCADE_POINTER_TRACE runtime=dogfood event={} stored_page=({:.1},{:.1}) cursor_age_ms={:?} move_count={} detail={}",
            event,
            self.cursor_x.get(),
            self.cursor_y.get(),
            age_ms,
            self.cursor_move_count.get(),
            detail,
        );
    }

    fn begin_address_entry(&self) {
        self.active_select.borrow_mut().take();
        *self.address_entry.borrow_mut() = Some(self.current_url.borrow().to_string());
        self.address_error.set(false);
        self.update_window_title();
    }

    fn cancel_address_entry(&self) {
        self.address_entry.borrow_mut().take();
        self.address_error.set(false);
        self.update_window_title();
    }

    fn submit_address_entry(&self) {
        let Some(input) = self.address_entry.borrow().clone() else {
            return;
        };
        let Ok(url) = parse_location_input(&input) else {
            self.address_error.set(true);
            self.update_window_title();
            return;
        };

        self.address_entry.borrow_mut().take();
        self.address_error.set(false);
        self.load_state.set(BrowserLoadState::Loading);
        *self.page_title.borrow_mut() = None;
        *self.current_url.borrow_mut() = url.clone();

        if let Some(webview) = self.webview.borrow().as_ref().cloned() {
            webview.load(url);
        }
        self.update_window_title();
    }

    fn reload_current_page(&self) {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return;
        };
        self.load_state.set(BrowserLoadState::Loading);
        webview.reload();
        self.update_window_title();
    }

    fn navigate_back(&self) {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return;
        };
        if webview.can_go_back() {
            self.load_state.set(BrowserLoadState::Loading);
            webview.go_back(1);
        }
        self.update_window_title();
    }

    fn navigate_forward(&self) {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return;
        };
        if webview.can_go_forward() {
            self.load_state.set(BrowserLoadState::Loading);
            webview.go_forward(1);
        }
        self.update_window_title();
    }

    fn handle_mouse_navigation_button(&self, button: WinitMouseButton) -> bool {
        match button {
            WinitMouseButton::Back => {
                self.navigate_back();
                true
            }
            WinitMouseButton::Forward => {
                self.navigate_forward();
                true
            }
            _ => false,
        }
    }

    fn handle_address_entry_key(&self, event: &KeyEvent) -> bool {
        if self.address_entry.borrow().is_none() {
            return false;
        }
        if event.state != ElementState::Pressed {
            return true;
        }

        match &event.logical_key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.submit_address_entry();
                true
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.cancel_address_entry();
                true
            }
            WinitKey::Named(WinitNamedKey::Backspace) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.pop();
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            _ => {
                let modifiers = self.modifiers.get();
                if modifiers.super_key() || modifiers.control_key() || modifiers.alt_key() {
                    return true;
                }
                let Some(text) = typed_text(event) else {
                    return true;
                };
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.push_str(&text);
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
        }
    }

    fn handle_browser_shortcut(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }

        let modifiers = self.modifiers.get();
        if !modifiers.super_key() {
            return false;
        }

        if self.webview.borrow().is_none() {
            return false;
        }

        match character_key(event).as_deref() {
            Some("l") | Some("L") => {
                self.begin_address_entry();
                true
            }
            Some("r") | Some("R") => {
                self.reload_current_page();
                true
            }
            Some("g") | Some("G") if modifiers.shift_key() => {
                self.grant_current_tab_to_copilot();
                true
            }
            Some("[") => {
                self.navigate_back();
                true
            }
            Some("]") => {
                self.navigate_forward();
                true
            }
            _ => false,
        }
    }

    fn handle_active_select_key(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed || self.active_select.borrow().is_none() {
            return false;
        }

        match &event.logical_key {
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.move_active_select(1);
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.move_active_select(-1);
                true
            }
            WinitKey::Named(WinitNamedKey::Enter) | WinitKey::Named(WinitNamedKey::Tab) => {
                self.submit_active_select();
                true
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.dismiss_active_select();
                true
            }
            _ => false,
        }
    }

    fn move_active_select(&self, direction: isize) {
        let mut active_select = self.active_select.borrow_mut();
        let Some(active) = active_select.as_mut() else {
            return;
        };
        let Some(next) = next_selectable_choice(&active.choices, active.cursor, direction) else {
            return;
        };
        active.cursor = next;
        active.control.select(vec![active.choices[next].id]);
        drop(active_select);
        self.update_window_title();
    }

    fn submit_active_select(&self) {
        let Some(mut active) = self.active_select.borrow_mut().take() else {
            return;
        };
        if let Some(choice) = active.choices.get(active.cursor) {
            active.control.select(vec![choice.id]);
        }
        active.control.submit();
        self.update_window_title();
    }

    fn dismiss_active_select(&self) {
        self.active_select.borrow_mut().take();
        self.update_window_title();
    }

    fn recover_page_focus_from_pointer(&self) {
        let mut changed = false;
        if self.address_entry.borrow().is_some() {
            self.address_entry.borrow_mut().take();
            self.address_error.set(false);
            changed = true;
        }
        if self.active_select.borrow().is_some() {
            self.active_select.borrow_mut().take();
            changed = true;
        }
        if changed {
            self.update_window_title();
        }
    }

    fn close_webview(&self) {
        self.active_select.borrow_mut().take();
        self.webview.borrow_mut().take();
    }

    fn grant_current_tab_to_copilot(&self) {
        match self.write_copilot_grant() {
            Ok(()) => {
                self.copilot_granted.set(true);
                self.copilot_grant_error.borrow_mut().take();
            }
            Err(error) => {
                self.copilot_grant_error
                    .borrow_mut()
                    .replace(error.to_string());
                eprintln!("failed to write Saccade co-pilot grant: {error:#}");
            }
        }
        self.update_window_title();
    }

    fn write_copilot_grant(&self) -> Result<()> {
        let Some(path) = self.copilot_grant_path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let current_url = self.current_url.borrow().to_string();
        let page_title = self.page_title.borrow().clone();
        let payload = current_tab_copilot_grant_payload(
            &current_url,
            page_title.as_deref(),
            self.rendering_settings.profile.name(),
            Some(&self.control_endpoint),
            unix_ms()?,
        );
        fs::write(path, serde_json::to_vec_pretty(&payload)?)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    fn drain_control_commands(&self) {
        while let Ok(command) = self.control_rx.try_recv() {
            let response = match command.request {
                Ok(request) => self.handle_control_request(request),
                Err(error) => json!({
                    "id": Value::Null,
                    "ok": false,
                    "error": format!("invalid dogfood control request: {error}"),
                }),
            };
            let _ = command.respond_to.send(response);
        }
    }

    fn handle_control_request(&self, request: DogfoodControlRequest) -> Value {
        let id = request.id.unwrap_or(Value::Null);
        let _params = request.params;
        match request.method.as_str() {
            "ping" => json!({
                "id": id,
                "ok": true,
                "result": {
                    "status": "ok",
                    "runtime": "saccade-dogfood-control-v0",
                    "same_webview_control": true,
                    "rendering_profile": self.rendering_settings.profile.name(),
                    "renderer_engine": self.rendering_settings.profile.engine(),
                    "servo_grid_enabled": self.rendering_settings.layout_grid_enabled,
                    "url": self.current_url.borrow().as_str(),
                    "title": self.page_title.borrow().clone(),
                    "load_state": self.load_state.get().label(),
                    "copilot_granted": self.copilot_granted.get(),
                    "has_webview": self.webview.borrow().is_some(),
                },
            }),
            "truth" => self.handle_control_probe(id, DogfoodControlProbeMethod::Truth),
            "actions" => self.handle_control_probe(id, DogfoodControlProbeMethod::Actions),
            "fill_agent_fields" => self.handle_control_fill_agent_fields(id, _params),
            "inspect_fields" => self.handle_control_inspect_fields(id, _params),
            "formmax_live_fill" => self.handle_control_formmax_live_fill(id, _params),
            "act" => self.handle_control_act(id, _params),
            other => json!({
                "id": id,
                "ok": false,
                "error": format!("unknown dogfood control method {other:?}"),
            }),
        }
    }

    fn handle_control_formmax_live_fill(&self, id: Value, params: Value) -> Value {
        if let Some(policy) = params.get("policy") {
            if policy
                .get("block_sensitive")
                .and_then(Value::as_bool)
                .is_some_and(|enabled| !enabled)
            {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": "formmax_live_fill requires block_sensitive=true",
                });
            }
            if policy
                .get("local_fixture_only")
                .and_then(Value::as_bool)
                .is_some_and(|enabled| !enabled)
            {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": "formmax_live_fill requires local_fixture_only=true",
                });
            }
        }

        match self.run_control_script_with_timeout(FORMMAX_LIVE_FILL_JS, FORMMAX_CONTROL_TIMEOUT) {
            Ok(value) => json!({
                "id": id,
                "ok": true,
                "result": self.control_formmax_live_fill_response(&value),
            }),
            Err(error) => json!({
                "id": id,
                "ok": false,
                "error": error,
            }),
        }
    }

    fn handle_control_fill_agent_fields(&self, id: Value, params: Value) -> Value {
        let Some(fields) = params.get("fields").and_then(Value::as_object) else {
            return json!({
                "id": id,
                "ok": false,
                "error": "fill_agent_fields requires object params.fields",
            });
        };
        if fields.is_empty() {
            return json!({
                "id": id,
                "ok": false,
                "error": "fill_agent_fields requires at least one field",
            });
        }
        let fields_json = match serde_json::to_string(fields) {
            Ok(value) => value,
            Err(error) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": format!("failed to serialize fields: {error}"),
                });
            }
        };
        let script = Box::leak(fill_agent_fields_script(&fields_json).into_boxed_str());
        match self.run_control_script(script) {
            Ok(value) => json!({
                "id": id,
                "ok": true,
                "result": self.control_fill_agent_fields_response(fields.len(), &value),
            }),
            Err(error) => json!({
                "id": id,
                "ok": false,
                "error": error,
            }),
        }
    }

    fn handle_control_inspect_fields(&self, id: Value, params: Value) -> Value {
        let Some(fields) = params.get("fields").and_then(Value::as_array) else {
            return json!({
                "id": id,
                "ok": false,
                "error": "inspect_fields requires array params.fields",
            });
        };
        if fields.is_empty() {
            return json!({
                "id": id,
                "ok": false,
                "error": "inspect_fields requires at least one field",
            });
        }
        if fields.iter().any(|field| field.as_str().is_none()) {
            return json!({
                "id": id,
                "ok": false,
                "error": "inspect_fields field ids must be strings",
            });
        }
        let fields_json = match serde_json::to_string(fields) {
            Ok(value) => value,
            Err(error) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": format!("failed to serialize field ids: {error}"),
                });
            }
        };
        let script = Box::leak(inspect_fields_script(&fields_json).into_boxed_str());
        match self.run_control_script(script) {
            Ok(value) => json!({
                "id": id,
                "ok": true,
                "result": self.control_inspect_fields_response(fields.len(), &value),
            }),
            Err(error) => json!({
                "id": id,
                "ok": false,
                "error": error,
            }),
        }
    }

    fn handle_control_act(&self, id: Value, params: Value) -> Value {
        let Some(action_id) = params.get("action_id").and_then(Value::as_str) else {
            return json!({
                "id": id,
                "ok": false,
                "error": "act requires string action_id",
            });
        };
        let Some(basis) = params.get("basis_page_revision").and_then(Value::as_u64) else {
            return json!({
                "id": id,
                "ok": false,
                "error": "act requires integer basis_page_revision",
            });
        };
        let current_revision = self.control_page_revision.get();
        if basis != current_revision {
            return json!({
                "id": id,
                "ok": false,
                "error": format!("stale action basis: requested {basis}, current {current_revision}"),
            });
        }

        let before_probe = match self.run_control_probe() {
            Ok(value) => value,
            Err(error) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": error,
                });
            }
        };
        let Some(action) = before_probe
            .get("actions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|action| action_id_for(action) == action_id)
            .cloned()
        else {
            return json!({
                "id": id,
                "ok": false,
                "error": format!("unknown action_id {action_id:?}"),
            });
        };
        if !action_enabled(&action) {
            return json!({
                "id": id,
                "ok": false,
                "error": format!("action {action_id:?} is not enabled"),
            });
        }
        if action
            .pointer("/sensitivity/kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind != "none")
        {
            return json!({
                "id": id,
                "ok": false,
                "error": format!("action {action_id:?} targets a sensitive field and requires user control"),
            });
        }
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return json!({
                "id": id,
                "ok": false,
                "error": "act requires an active WebView",
            });
        };
        self.click_control_action(&webview, &action);

        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(160) {
            self.servo.spin_event_loop();
            thread::sleep(Duration::from_millis(5));
        }
        let after_probe = match self.run_control_probe() {
            Ok(value) => value,
            Err(error) => {
                return json!({
                    "id": id,
                    "ok": false,
                    "error": error,
                });
            }
        };
        if probe_changed(&before_probe, &after_probe) {
            self.control_page_revision
                .set(self.control_page_revision.get().saturating_add(1));
        }

        json!({
            "id": id,
            "ok": true,
            "result": self.control_act_response(action_id, basis, &before_probe, &after_probe),
        })
    }

    fn handle_control_probe(&self, id: Value, method: DogfoodControlProbeMethod) -> Value {
        match self.run_control_probe() {
            Ok(probe) => json!({
                "id": id,
                "ok": true,
                "result": self.control_probe_response(&probe, method),
            }),
            Err(error) => json!({
                "id": id,
                "ok": false,
                "error": error,
            }),
        }
    }

    fn run_control_probe(&self) -> std::result::Result<Value, String> {
        self.run_control_script(PROBE_JS)
    }

    fn run_control_script(&self, script: &'static str) -> std::result::Result<Value, String> {
        self.run_control_script_with_timeout(script, Duration::from_secs(5))
    }

    fn run_control_script_with_timeout(
        &self,
        script: &'static str,
        timeout: Duration,
    ) -> std::result::Result<Value, String> {
        let webview = self
            .webview
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| "dogfood control probe requires an active WebView".to_string())?;
        let (tx, rx) = mpsc::channel();
        webview.evaluate_javascript(script, move |result| {
            let value = match result {
                Ok(JSValue::String(value)) => Ok(value),
                Ok(value) => Ok(format!("{value:?}")),
                Err(error) => Err(format!("{error:?}")),
            };
            let _ = tx.send(value);
        });

        let started = Instant::now();
        while started.elapsed() < timeout {
            self.servo.spin_event_loop();
            match rx.try_recv() {
                Ok(Ok(value)) => {
                    return serde_json::from_str(&value).map_err(|error| {
                        format!("failed to parse dogfood control probe: {error}")
                    });
                }
                Ok(Err(error)) => return Err(format!("dogfood control probe failed: {error}")),
                Err(mpsc::TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("dogfood control probe callback disconnected".into());
                }
            }
        }
        Err("dogfood control script timed out".into())
    }

    fn control_probe_response(&self, probe: &Value, method: DogfoodControlProbeMethod) -> Value {
        let actions = action_map(probe);
        let sensitive_count = sensitive_action_count(&actions);
        let body_text_length = probe
            .get("bodyTextLength")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let body_child_count = probe
            .get("bodyChildCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let page_revision = probe
            .get("pageRevision")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .unwrap_or(1)
            .max(self.control_page_revision.get());
        let engine = match method {
            DogfoodControlProbeMethod::Truth => "saccade-dogfood-control-truth-v0",
            DogfoodControlProbeMethod::Actions => "saccade-dogfood-control-actions-v0",
        };
        let summary = match method {
            DogfoodControlProbeMethod::Truth => {
                "dogfood current-tab redacted truth collected from same live WebView"
            }
            DogfoodControlProbeMethod::Actions => {
                "dogfood current-tab action map collected from same live WebView"
            }
        };
        let runtime_geometry = self
            .webview
            .borrow()
            .as_ref()
            .map(|webview| self.runtime_geometry(webview))
            .unwrap_or(Value::Null);
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": engine,
            "summary": summary,
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "renderer_engine": self.rendering_settings.profile.engine(),
            "servo_grid_enabled": self.rendering_settings.layout_grid_enabled,
            "legacy_grid_override": self.rendering_settings.legacy_grid_override,
            "experimental_prefs": self.rendering_settings.experimental_prefs(),
            "runtime_geometry": runtime_geometry,
            "url": probe.get("url").cloned().unwrap_or_else(|| json!(self.current_url.borrow().as_str())),
            "title": probe.get("title").cloned().unwrap_or_else(|| self.page_title.borrow().clone().map(Value::String).unwrap_or(Value::Null)),
            "page_revision": page_revision,
            "dom_page_revision": probe.get("pageRevision").cloned().unwrap_or(Value::Null),
            "actions": actions,
            "findings": [],
            "visual_health": {
                "blank_page": body_text_length == 0 && body_child_count == 0,
                "screenshot": null,
            },
            "truth": {
                "body_text_length": body_text_length,
                "body_child_count": body_child_count,
                "viewport": probe.get("viewport").cloned().unwrap_or(Value::Null),
                "layout_probes": probe.get("layoutProbes").cloned().unwrap_or(Value::Null),
                "sensitive_fields": sensitive_count,
                "findings": [],
            },
            "artifacts": {
                "report": null,
                "replay": null,
            },
        })
    }

    fn control_formmax_live_fill_response(&self, result: &Value) -> Value {
        let rows = result.get("rows").and_then(Value::as_u64).unwrap_or(0);
        let pages = result.get("pages").and_then(Value::as_u64).unwrap_or(0);
        let filled = result.get("filled").and_then(Value::as_u64).unwrap_or(0);
        let blocked_sensitive = result
            .get("blocked_sensitive")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let receipt_verified = result
            .get("receipt_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let validation_errors = result
            .get("validation_errors")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let replay_events = result
            .get("replay_events")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                result
                    .get("events")
                    .and_then(Value::as_array)
                    .map(|events| events.len() as u64)
                    .unwrap_or(0)
            });
        let receipt_row_count = result
            .pointer("/receipt/row_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);

        if filled > 0 {
            self.control_page_revision
                .set(self.control_page_revision.get().saturating_add(1));
        }

        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": "saccade-dogfood-control-formmax-live-v0",
            "summary": "FORMMAX capacity fixture filled and verified inside the same dogfood WebView",
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "page_revision": self.control_page_revision.get(),
            "rows": rows,
            "pages": pages,
            "filled": filled,
            "blocked_sensitive": blocked_sensitive,
            "receipt_verified": receipt_verified,
            "validation_errors": validation_errors,
            "replay_events": replay_events,
            "receipt": {
                "row_count": receipt_row_count,
                "validation": result.pointer("/receipt/validation").cloned().unwrap_or(Value::Null),
                "sensitive_fields_present": result
                    .pointer("/receipt/sensitive_fields_present")
                    .cloned()
                    .unwrap_or(Value::Null),
            },
            "policy": {
                "block_sensitive": true,
                "local_fixture_only": true,
                "same_live_tab": true,
                "values_logged": false,
            },
            "artifacts": {
                "report": null,
                "replay": null,
            },
        })
    }

    fn control_fill_agent_fields_response(&self, requested: usize, fill_result: &Value) -> Value {
        let filled = fill_result
            .get("filled")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !filled.is_empty() {
            self.control_page_revision
                .set(self.control_page_revision.get().saturating_add(1));
        }
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": "saccade-dogfood-control-fill-v0",
            "summary": "agent-owned non-sensitive fields filled in the same dogfood WebView",
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "page_revision": self.control_page_revision.get(),
            "requested": requested,
            "filled": fill_result.get("filled").cloned().unwrap_or_else(|| json!([])),
            "rejected": fill_result.get("rejected").cloned().unwrap_or_else(|| json!([])),
            "sensitive_fields_seen": fill_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
            "artifacts": {
                "report": null,
                "replay": null,
            },
        })
    }

    fn control_inspect_fields_response(&self, requested: usize, inspect_result: &Value) -> Value {
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": "saccade-dogfood-control-inspect-fields-v0",
            "summary": "explicit field inspection completed in the same dogfood WebView with sensitive values masked",
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "page_revision": self.control_page_revision.get(),
            "requested": requested,
            "fields": inspect_result.get("fields").cloned().unwrap_or_else(|| json!([])),
            "sensitive_fields_seen": inspect_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
            "artifacts": {
                "report": null,
                "replay": null,
            },
        })
    }

    fn control_act_response(
        &self,
        action_id: &str,
        basis_page_revision: u64,
        before_probe: &Value,
        after_probe: &Value,
    ) -> Value {
        let actions = action_map(after_probe);
        let sensitive_count = sensitive_action_count(&actions);
        let changed = probe_changed(before_probe, after_probe);
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": "saccade-dogfood-control-act-v0",
            "summary": "action dispatched through the same dogfood WebView",
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "page_revision": self.control_page_revision.get(),
            "actions": actions,
            "verification": {
                "mode": "dogfood_control_native_click_v0",
                "action_id": action_id,
                "action_sent": true,
                "changed": changed,
                "no_effect": !changed,
                "basis_page_revision": basis_page_revision,
                "new_page_revision": self.control_page_revision.get(),
                "body_text_length_changed": before_probe.get("bodyTextLength") != after_probe.get("bodyTextLength"),
                "body_child_count_changed": before_probe.get("bodyChildCount") != after_probe.get("bodyChildCount"),
                "dom_page_revision_before": before_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
                "dom_page_revision_after": after_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
            },
            "truth": {
                "sensitive_fields": sensitive_count,
            },
            "artifacts": {
                "report": null,
                "replay": null,
            },
        })
    }

    fn click_control_action(&self, webview: &WebView, action: &Value) {
        let rect = action.get("rect").unwrap_or(&Value::Null);
        let x = (value_f64(rect, "left") + value_f64(rect, "width") / 2.0) as f32;
        let y = (value_f64(rect, "top") + value_f64(rect, "height") / 2.0) as f32;
        let point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(x, y));
        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(point)));
        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
            MouseButtonAction::Down,
            MouseButton::Left,
            point,
        )));
        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
            MouseButtonAction::Up,
            MouseButton::Left,
            point,
        )));
    }

    fn runtime_geometry(&self, webview: &WebView) -> Value {
        let window_size = self.window.inner_size();
        let context_size = self.rendering_context.size2d();
        let webview_size = webview.size();
        json!({
            "window_inner": {
                "width": window_size.width,
                "height": window_size.height,
            },
            "rendering_context_device": {
                "width": context_size.width,
                "height": context_size.height,
            },
            "webview_device": {
                "width": webview_size.width,
                "height": webview_size.height,
            },
            "hidpi_scale_factor": webview.hidpi_scale_factor().0,
        })
    }
}

impl WebViewDelegate for DogfoodBrowserState {
    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        *self.current_url.borrow_mut() = url;
        *self.page_title.borrow_mut() = None;
        self.load_state.set(BrowserLoadState::Loading);
        self.update_window_title();
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        *self.page_title.borrow_mut() = title;
        self.update_window_title();
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        self.load_state.set(status.into());
        self.update_window_title();
        if status == LoadStatus::Complete {
            self.window.request_redraw();
        }
    }

    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }

    fn show_embedder_control(&self, _webview: WebView, embedder_control: EmbedderControl) {
        if let EmbedderControl::SelectElement(mut select) = embedder_control {
            let choices = flatten_select_choices(&select);
            let selected_options = select.selected_options();
            let cursor = choices
                .iter()
                .position(|choice| selected_options.contains(&choice.id) && !choice.disabled)
                .or_else(|| choices.iter().position(|choice| !choice.disabled))
                .unwrap_or(0);

            if let Some(choice) = choices.get(cursor) {
                select.select(vec![choice.id]);
            }

            *self.active_select.borrow_mut() = Some(ActiveSelect {
                control: select,
                choices,
                cursor,
            });
            self.update_window_title();
        }
    }
}

enum DogfoodBrowserApp {
    Initial {
        waker: Waker,
        config: DogfoodBrowserConfig,
        rendering_settings: RenderingProfileSettings,
        control_bridge: Option<DogfoodControlBridge>,
    },
    Running {
        state: Rc<DogfoodBrowserState>,
    },
    Finished,
}

impl DogfoodBrowserApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        config: DogfoodBrowserConfig,
        rendering_settings: RenderingProfileSettings,
        control_bridge: DogfoodControlBridge,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            config,
            rendering_settings,
            control_bridge: Some(control_bridge),
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Running { state } = self else {
            return;
        };

        if state
            .auto_close_after
            .is_some_and(|timeout| state.started_at.elapsed() >= timeout)
        {
            state.close_webview();
            event_loop.exit();
            *self = Self::Finished;
        }
    }
}

impl ApplicationHandler<WakerEvent> for DogfoodBrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            config,
            rendering_settings,
            control_bridge,
        } = self
        else {
            return;
        };
        let Some(control_bridge) = control_bridge.take() else {
            event_loop.exit();
            return;
        };

        let display_handle = match event_loop.display_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("failed to get display handle: {error}");
                event_loop.exit();
                return;
            }
        };

        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title(format!("Saccade - {}", config.url))
                .with_inner_size(LogicalSize::new(config.width.max(1), config.height.max(1))),
        ) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("failed to create Saccade window: {error}");
                event_loop.exit();
                return;
            }
        };

        let window_handle = match window.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("failed to get window handle: {error}");
                event_loop.exit();
                return;
            }
        };

        let initial_physical_size = window.inner_size();
        let rendering_context =
            match WindowRenderingContext::new(display_handle, window_handle, initial_physical_size)
            {
                Ok(context) => Rc::new(context),
                Err(error) => {
                    eprintln!("failed to create rendering context: {error:?}");
                    event_loop.exit();
                    return;
                }
            };

        if let Err(error) = rendering_context.make_current() {
            eprintln!("failed to make GL context current: {error:?}");
            event_loop.exit();
            return;
        }

        let mut servo_builder = ServoBuilder::default()
            .preferences(rendering_settings.servo_preferences())
            .event_loop_waker(Box::new(waker.clone()));
        if let Some(profile_dir) = config.profile_dir.clone() {
            let mut opts = Opts::default();
            opts.config_dir = Some(profile_dir);
            opts.temporary_storage = false;
            servo_builder = servo_builder.opts(opts);
        }
        let servo = servo_builder.build();
        servo.setup_logging();

        let state = Rc::new(DogfoodBrowserState {
            window,
            servo,
            rendering_context,
            url: config.url.clone(),
            current_url: RefCell::new(config.url.clone()),
            webview: RefCell::new(None),
            cursor_x: Cell::new(0.0),
            cursor_y: Cell::new(0.0),
            cursor_move_count: Cell::new(0),
            last_cursor_move_at: Cell::new(None),
            modifiers: Cell::new(ModifiersState::empty()),
            load_state: Cell::new(BrowserLoadState::Starting),
            page_title: RefCell::new(None),
            address_entry: RefCell::new(None),
            address_error: Cell::new(false),
            copilot_granted: Cell::new(false),
            copilot_grant_error: RefCell::new(None),
            copilot_grant_path: config.copilot_grant_path.clone(),
            control_page_revision: Cell::new(1),
            control_endpoint: control_bridge.endpoint.clone(),
            control_rx: control_bridge.rx,
            active_select: RefCell::new(None),
            started_at: Instant::now(),
            auto_close_after: config.auto_close_after,
            rendering_settings: rendering_settings.clone(),
            pointer_trace: env_flag("SACCADE_TRACE_POINTER"),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.url.clone())
            .hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32))
            .delegate(state.clone())
            .build();
        *state.webview.borrow_mut() = Some(webview);
        if config.auto_grant_copilot {
            state.grant_current_tab_to_copilot();
        }
        state.update_window_title();

        *self = Self::Running { state };
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: WakerEvent) {
        if let Self::Running { state } = self {
            state.servo.spin_event_loop();
            state.drain_control_commands();
        }
        self.after_spin(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if let Self::Running { state } = self {
            state.servo.spin_event_loop();
            state.drain_control_commands();

            match event {
                WindowEvent::CloseRequested => {
                    state.close_webview();
                    event_loop.exit();
                    *self = Self::Finished;
                }
                WindowEvent::RedrawRequested => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.paint();
                        state.rendering_context.present();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview
                            .set_hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32));
                        webview.resize(new_size);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    state.store_cursor_page_position(position);
                    state
                        .cursor_move_count
                        .set(state.cursor_move_count.get().saturating_add(1));
                    state.last_cursor_move_at.set(Some(Instant::now()));
                    state.trace_cursor_moved(position);
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::MouseInput {
                    state: button_state,
                    button,
                    ..
                } => {
                    state.trace_pointer_event(
                        "mouse_input",
                        format_args!("state={button_state:?} button={button:?}"),
                    );
                    if button_state == ElementState::Pressed {
                        if state.handle_mouse_navigation_button(button) {
                            return;
                        }
                        state.recover_page_focus_from_pointer();
                    }
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                            mouse_button_action(button_state),
                            servo_mouse_button(button),
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    state.trace_pointer_event("mouse_wheel", format_args!("delta={delta:?}"));
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        let (x, y, mode) = wheel_delta(delta);
                        webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                            WheelDelta { x, y, z: 0.0, mode },
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    state.modifiers.set(modifiers.state());
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if state.handle_address_entry_key(&event)
                        || state.handle_active_select_key(&event)
                        || state.handle_browser_shortcut(&event)
                    {
                        return;
                    }

                    if let (Some(webview), Some(keyboard_event)) = (
                        state.webview.borrow().as_ref().cloned(),
                        servo_keyboard_event(&event),
                    ) {
                        webview.notify_input_event(InputEvent::Keyboard(keyboard_event));
                    }
                }
                _ => {}
            }
        }
        self.after_spin(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Running { state } = self {
            state.drain_control_commands();
        }
        self.after_spin(event_loop);
    }
}

fn flatten_select_choices(select: &SelectElement) -> Vec<SelectChoice> {
    let mut choices = Vec::new();
    for option_or_group in select.options() {
        match option_or_group {
            SelectElementOptionOrOptgroup::Option(option) => {
                choices.push(SelectChoice {
                    id: option.id,
                    label: option.label.clone(),
                    disabled: option.is_disabled,
                });
            }
            SelectElementOptionOrOptgroup::Optgroup { label, options } => {
                for option in options {
                    choices.push(SelectChoice {
                        id: option.id,
                        label: format!("{label} / {}", option.label),
                        disabled: option.is_disabled,
                    });
                }
            }
        }
    }
    choices
}

fn next_selectable_choice(
    choices: &[SelectChoice],
    cursor: usize,
    direction: isize,
) -> Option<usize> {
    if choices.is_empty() {
        return None;
    }

    let len = choices.len();
    for step in 1..=len {
        let next = if direction >= 0 {
            (cursor + step) % len
        } else {
            (cursor + len - step) % len
        };
        if !choices[next].disabled {
            return Some(next);
        }
    }
    None
}

fn wheel_delta(delta: MouseScrollDelta) -> (f64, f64, WheelMode) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            ((x * 76.0) as f64, (y * 76.0) as f64, WheelMode::DeltaLine)
        }
        MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y, WheelMode::DeltaPixel),
    }
}

fn mouse_button_action(state: ElementState) -> MouseButtonAction {
    match state {
        ElementState::Pressed => MouseButtonAction::Down,
        ElementState::Released => MouseButtonAction::Up,
    }
}

fn servo_mouse_button(button: WinitMouseButton) -> MouseButton {
    match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Back,
        WinitMouseButton::Forward => MouseButton::Forward,
        WinitMouseButton::Other(value) => MouseButton::Other(value),
    }
}

fn servo_keyboard_event(event: &KeyEvent) -> Option<KeyboardEvent> {
    let state = match event.state {
        ElementState::Pressed => KeyState::Down,
        ElementState::Released => KeyState::Up,
    };
    let key = servo_key(event)?;
    Some(KeyboardEvent::from_state_and_key(state, key))
}

fn servo_key(event: &KeyEvent) -> Option<ServoKey> {
    if event.state == ElementState::Pressed {
        if let Some(text) = event.text.as_ref() {
            let text = text.to_string();
            if !text.is_empty() && !text.chars().any(char::is_control) {
                return Some(ServoKey::Character(text));
            }
        }
    }

    match &event.logical_key {
        WinitKey::Character(text) => {
            let text = text.to_string();
            if text.is_empty() {
                None
            } else {
                Some(ServoKey::Character(text))
            }
        }
        WinitKey::Named(WinitNamedKey::Space) => Some(ServoKey::Character(" ".to_string())),
        WinitKey::Named(named) => map_named_key(*named).map(ServoKey::Named),
        WinitKey::Unidentified(_) | WinitKey::Dead(_) => None,
    }
}

fn character_key(event: &KeyEvent) -> Option<String> {
    match &event.logical_key {
        WinitKey::Character(text) => Some(text.to_string()),
        _ => None,
    }
}

fn typed_text(event: &KeyEvent) -> Option<String> {
    if let Some(text) = event.text.as_ref() {
        if !text.is_empty() && !text.chars().any(char::is_control) {
            return Some(text.to_string());
        }
    }

    match &event.logical_key {
        WinitKey::Character(text) if !text.is_empty() => Some(text.to_string()),
        WinitKey::Named(WinitNamedKey::Space) => Some(" ".to_string()),
        _ => None,
    }
}

fn parse_location_input(input: &str) -> Result<Url, url::ParseError> {
    let trimmed = input.trim();
    if has_url_scheme(trimmed) {
        return Url::parse(trimmed);
    }

    let prefix = if looks_like_local_address(trimmed) {
        "http://"
    } else {
        "https://"
    };
    Url::parse(&format!("{prefix}{trimmed}"))
}

fn value_f64(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn has_url_scheme(input: &str) -> bool {
    if input.contains("://") {
        return true;
    }
    let Some(index) = input.find(':') else {
        return false;
    };
    matches!(
        input[..index].to_ascii_lowercase().as_str(),
        "about" | "data" | "file" | "http" | "https"
    )
}

fn looks_like_local_address(input: &str) -> bool {
    let lowercase = input.to_ascii_lowercase();
    lowercase == "localhost"
        || lowercase.starts_with("localhost:")
        || lowercase.starts_with("127.")
        || lowercase.starts_with("0.0.0.0")
        || lowercase.starts_with("[::1]")
}

fn current_tab_copilot_grant_payload(
    url: &str,
    title: Option<&str>,
    profile: &str,
    control_endpoint: Option<&DogfoodControlEndpoint>,
    written_unix_ms: u128,
) -> Value {
    let control_endpoint = control_endpoint
        .map(|endpoint| {
            json!({
                "protocol": "saccade-dogfood-control-v0",
                "scheme": "tcp",
                "host": endpoint.addr.ip().to_string(),
                "port": endpoint.addr.port(),
            })
        })
        .unwrap_or(Value::Null);
    json!({
        "status": "granted",
        "runtime": "saccade-dogfood-browser-v0",
        "grant_type": "current_tab_copilot",
        "selected_tab_seen": true,
        "grant_required": true,
        "grant_given": true,
        "owner": "Human",
        "read_grant": "FullTruth",
        "agent_input_grant": true,
        "url": url,
        "title": title,
        "rendering_profile": profile,
        "shortcut": "Cmd+Shift+G",
        "mcp_tool": "saccade.tabs.grant_current",
        "control_endpoint": control_endpoint,
        "transport_status": "url_grant_artifact_v0",
        "note": "MCP v0 should call saccade.tabs.grant_current with this artifact. The control endpoint supports same-WebView truth/actions/fill/inspect/act/formmax.",
        "written_unix_ms": written_unix_ms,
    })
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis())
}

fn map_named_key(key: WinitNamedKey) -> Option<ServoNamedKey> {
    match key {
        WinitNamedKey::Enter => Some(ServoNamedKey::Enter),
        WinitNamedKey::Tab => Some(ServoNamedKey::Tab),
        WinitNamedKey::Escape => Some(ServoNamedKey::Escape),
        WinitNamedKey::Backspace => Some(ServoNamedKey::Backspace),
        WinitNamedKey::Delete => Some(ServoNamedKey::Delete),
        WinitNamedKey::ArrowDown => Some(ServoNamedKey::ArrowDown),
        WinitNamedKey::ArrowLeft => Some(ServoNamedKey::ArrowLeft),
        WinitNamedKey::ArrowRight => Some(ServoNamedKey::ArrowRight),
        WinitNamedKey::ArrowUp => Some(ServoNamedKey::ArrowUp),
        WinitNamedKey::End => Some(ServoNamedKey::End),
        WinitNamedKey::Home => Some(ServoNamedKey::Home),
        WinitNamedKey::PageDown => Some(ServoNamedKey::PageDown),
        WinitNamedKey::PageUp => Some(ServoNamedKey::PageUp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_title_includes_url_load_and_nav_state() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Complete,
            can_go_back: true,
            can_go_forward: false,
            page_title: Some("Example Domain"),
            current_url: "https://example.com/",
            copilot_label: "off Cmd+Shift+G",
            address_entry: None,
            active_select_label: None,
        });

        assert!(title.contains("Saccade [servo-modern]"));
        assert!(title.contains("copilot=off Cmd+Shift+G"));
        assert!(title.contains("load=complete"));
        assert!(title.contains("back=y"));
        assert!(title.contains("fwd=n"));
        assert!(title.contains("Example Domain"));
        assert!(title.contains("https://example.com/"));
    }

    #[test]
    fn shell_title_marks_select_mode_without_hiding_url() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Loading,
            can_go_back: false,
            can_go_forward: true,
            page_title: Some("Ignored while select is active"),
            current_url: "https://example.com/form",
            copilot_label: "granted",
            address_entry: None,
            active_select_label: Some("us-east"),
        });

        assert!(title.contains("copilot=granted"));
        assert!(title.contains("select=us-east"));
        assert!(title.contains("back=n"));
        assert!(title.contains("fwd=y"));
        assert!(title.contains("reload=Cmd+R"));
        assert!(title.contains("https://example.com/form"));
    }

    #[test]
    fn shell_title_marks_address_entry_mode() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Complete,
            can_go_back: true,
            can_go_forward: true,
            page_title: Some("Ignored while address entry is active"),
            current_url: "https://example.com/old",
            copilot_label: "granted",
            address_entry: Some(AddressEntryTitle {
                input: "https://example.com/new",
                invalid: false,
            }),
            active_select_label: Some("Ignored"),
        });

        assert!(title.contains("copilot=granted location> https://example.com/new"));
        assert!(title.contains("Enter open Esc cancel"));
        assert!(title.contains("https://example.com/old"));
        assert!(!title.contains("select="));
    }

    #[test]
    fn shell_title_marks_invalid_address_entry() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Complete,
            can_go_back: false,
            can_go_forward: false,
            page_title: None,
            current_url: "https://example.com/",
            copilot_label: "error",
            address_entry: Some(AddressEntryTitle {
                input: "https://",
                invalid: true,
            }),
            active_select_label: None,
        });

        assert!(title.contains("copilot=error location invalid> https://"));
    }

    #[test]
    fn current_tab_copilot_grant_payload_has_policy_boundary() {
        let payload = current_tab_copilot_grant_payload(
            "https://example.com/form",
            Some("Example Form"),
            "servo-modern",
            Some(&DogfoodControlEndpoint {
                addr: "127.0.0.1:49321"
                    .parse()
                    .expect("test address should parse"),
            }),
            123,
        );

        assert_eq!(payload["status"], "granted");
        assert_eq!(payload["owner"], "Human");
        assert_eq!(payload["read_grant"], "FullTruth");
        assert_eq!(payload["agent_input_grant"], true);
        assert_eq!(payload["mcp_tool"], "saccade.tabs.grant_current");
        assert_eq!(payload["url"], "https://example.com/form");
        assert_eq!(payload["title"], "Example Form");
        assert_eq!(
            payload["control_endpoint"]["protocol"],
            "saccade-dogfood-control-v0"
        );
        assert_eq!(payload["control_endpoint"]["host"], "127.0.0.1");
        assert_eq!(payload["control_endpoint"]["port"], 49321);
        assert_eq!(payload["written_unix_ms"], 123);
    }

    #[test]
    fn location_input_adds_default_scheme() {
        let url = parse_location_input("ign.com").expect("bare host should parse");

        assert_eq!(url.as_str(), "https://ign.com/");
    }

    #[test]
    fn location_input_uses_http_for_localhost() {
        let url = parse_location_input("localhost:3000/path").expect("local host should parse");

        assert_eq!(url.as_str(), "http://localhost:3000/path");
    }

    #[test]
    fn location_input_preserves_explicit_scheme() {
        let url = parse_location_input("file:///tmp/example.html").expect("file URL should parse");

        assert_eq!(url.as_str(), "file:///tmp/example.html");
    }

    #[test]
    fn location_input_rejects_empty_value() {
        assert!(parse_location_input("   ").is_err());
    }
}

#[derive(Clone)]
struct Waker(EventLoopProxy<WakerEvent>);

#[derive(Debug)]
struct WakerEvent;

impl Waker {
    fn new(event_loop: &EventLoop<WakerEvent>) -> Self {
        Self(event_loop.create_proxy())
    }
}

impl servo::EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(Self(self.0.clone()))
    }

    fn wake(&self) {
        let _ = self.0.send_event(WakerEvent);
    }
}
