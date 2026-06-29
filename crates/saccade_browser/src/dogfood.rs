use std::cell::{Cell, RefCell};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use anyhow::{Context, Result};
use euclid::{Point2D, Scale};
use glow::HasContext;
use servo::{
    CSSPixel, EmbedderControl, InputEvent, JSValue, Key as ServoKey, KeyState, KeyboardEvent,
    LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey as ServoNamedKey, Opts, RenderingContext, SelectElement,
    SelectElementOptionOrOptgroup, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate,
    WebViewPoint, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect as SkiaRect, Stroke, Transform,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
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
const SHELL_TOOLBAR_HEIGHT: f32 = 44.0;
const SHELL_TOOLBAR_MARGIN: f32 = 8.0;
const SHELL_TOOLBAR_BUTTON: f32 = 32.0;
const SHELL_TOOLBAR_GAP: f32 = 6.0;
const SHELL_TOOLBAR_GRANT_WIDTH: f32 = 104.0;
const SHELL_TOOLBAR_TEXT_PX: f32 = 1.6;
#[allow(dead_code)]
const SHELL_TOOLBAR_LABEL_PX: f32 = 1.45;
const SHELL_TOOLBAR_TEXT_SIZE: f32 = 13.5;
const SHELL_TOOLBAR_LABEL_SIZE: f32 = 13.0;
const TOOLBAR_FONT_PATHS: &[&str] = &[
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/SFCompact.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Supplemental/Verdana.ttf",
    "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf",
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
];

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

#[derive(Debug, Clone)]
struct AddressEntryState {
    text: String,
    selection_anchor: usize,
    selection_focus: usize,
}

impl AddressEntryState {
    fn new_selected(text: String) -> Self {
        let end = text.len();
        Self {
            text,
            selection_anchor: 0,
            selection_focus: end,
        }
    }

    fn selection_range(&self) -> (usize, usize) {
        (
            self.selection_anchor.min(self.selection_focus),
            self.selection_anchor.max(self.selection_focus),
        )
    }

    fn has_selection(&self) -> bool {
        self.selection_anchor != self.selection_focus
    }

    fn caret(&self) -> usize {
        self.selection_focus
    }

    fn select_all(&mut self) {
        self.selection_anchor = 0;
        self.selection_focus = self.text.len();
    }

    fn replace_selection(&mut self, replacement: &str) {
        let (start, end) = self.selection_range();
        self.text.replace_range(start..end, replacement);
        self.collapse_to(start + replacement.len());
    }

    fn backspace(&mut self) {
        if self.has_selection() {
            self.replace_selection("");
            return;
        }

        let caret = self.caret();
        if caret == 0 {
            return;
        }
        let previous = previous_char_boundary(&self.text, caret);
        self.text.replace_range(previous..caret, "");
        self.collapse_to(previous);
    }

    fn delete_forward(&mut self) {
        if self.has_selection() {
            self.replace_selection("");
            return;
        }

        let caret = self.caret();
        if caret >= self.text.len() {
            return;
        }
        let next = next_char_boundary(&self.text, caret);
        self.text.replace_range(caret..next, "");
        self.collapse_to(caret);
    }

    fn move_left(&mut self, extend: bool) {
        let next = if self.has_selection() && !extend {
            self.selection_range().0
        } else {
            previous_char_boundary(&self.text, self.caret())
        };
        self.move_focus_to(next, extend);
    }

    fn move_right(&mut self, extend: bool) {
        let next = if self.has_selection() && !extend {
            self.selection_range().1
        } else {
            next_char_boundary(&self.text, self.caret())
        };
        self.move_focus_to(next, extend);
    }

    fn move_to_start(&mut self, extend: bool) {
        self.move_focus_to(0, extend);
    }

    fn move_to_end(&mut self, extend: bool) {
        self.move_focus_to(self.text.len(), extend);
    }

    fn move_focus_to(&mut self, index: usize, extend: bool) {
        let index = clamp_to_char_boundary(&self.text, index);
        if extend {
            self.selection_focus = index;
        } else {
            self.collapse_to(index);
        }
    }

    fn collapse_to(&mut self, index: usize) {
        let index = clamp_to_char_boundary(&self.text, index);
        self.selection_anchor = index;
        self.selection_focus = index;
    }
}

#[derive(Debug, Clone, Copy)]
struct ToolbarRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl ToolbarRect {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    fn inset(self, amount: f32) -> Self {
        Self {
            x: self.x + amount,
            y: self.y + amount,
            width: (self.width - amount * 2.0).max(0.0),
            height: (self.height - amount * 2.0).max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ToolbarColor(f32, f32, f32, f32);

impl ToolbarColor {
    fn to_rgba8(self) -> (u8, u8, u8, u8) {
        fn channel(value: f32) -> u8 {
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        }

        (
            channel(self.0),
            channel(self.1),
            channel(self.2),
            channel(self.3),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct ShellToolbarMetrics {
    bounds: ToolbarRect,
    back: ToolbarRect,
    forward: ToolbarRect,
    reload: ToolbarRect,
    address: ToolbarRect,
    grant: ToolbarRect,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ShellToolbarTarget {
    Back,
    Forward,
    Reload,
    Address,
    Grant,
    Strip,
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
    address_entry: RefCell<Option<AddressEntryState>>,
    address_error: Cell<bool>,
    copilot_granted: Cell<bool>,
    copilot_grant_error: RefCell<Option<String>>,
    copilot_grant_path: Option<PathBuf>,
    control_page_revision: Cell<u64>,
    control_endpoint: DogfoodControlEndpoint,
    control_rx: Receiver<DogfoodControlCommand>,
    active_select: RefCell<Option<ActiveSelect>>,
    toolbar_mouse_capture: Cell<bool>,
    toolbar_draw_error_reported: Cell<bool>,
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

    fn logical_window_size(&self) -> (f32, f32) {
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let scale = if scale > 0.0 { scale } else { 1.0 };
        (size.width as f32 / scale, size.height as f32 / scale)
    }

    fn toolbar_metrics(&self) -> ShellToolbarMetrics {
        let (window_width, _) = self.logical_window_size();
        let bounds = ToolbarRect {
            x: 0.0,
            y: 0.0,
            width: window_width.max(1.0),
            height: SHELL_TOOLBAR_HEIGHT,
        };
        let button_y = (SHELL_TOOLBAR_HEIGHT - SHELL_TOOLBAR_BUTTON) / 2.0;
        let back = ToolbarRect {
            x: SHELL_TOOLBAR_MARGIN,
            y: button_y,
            width: SHELL_TOOLBAR_BUTTON,
            height: SHELL_TOOLBAR_BUTTON,
        };
        let forward = ToolbarRect {
            x: back.x + SHELL_TOOLBAR_BUTTON + SHELL_TOOLBAR_GAP,
            y: button_y,
            width: SHELL_TOOLBAR_BUTTON,
            height: SHELL_TOOLBAR_BUTTON,
        };
        let reload = ToolbarRect {
            x: forward.x + SHELL_TOOLBAR_BUTTON + SHELL_TOOLBAR_GAP,
            y: button_y,
            width: SHELL_TOOLBAR_BUTTON,
            height: SHELL_TOOLBAR_BUTTON,
        };
        let grant_width = if window_width >= 520.0 {
            SHELL_TOOLBAR_GRANT_WIDTH
        } else {
            SHELL_TOOLBAR_BUTTON
        };
        let grant = ToolbarRect {
            x: (window_width - SHELL_TOOLBAR_MARGIN - grant_width).max(0.0),
            y: button_y,
            width: grant_width,
            height: SHELL_TOOLBAR_BUTTON,
        };
        let address_x = reload.x + SHELL_TOOLBAR_BUTTON + SHELL_TOOLBAR_GAP * 1.5;
        let address_right = (grant.x - SHELL_TOOLBAR_GAP).max(address_x);
        let address = ToolbarRect {
            x: address_x,
            y: button_y,
            width: (address_right - address_x).max(0.0),
            height: SHELL_TOOLBAR_BUTTON,
        };

        ShellToolbarMetrics {
            bounds,
            back,
            forward,
            reload,
            address,
            grant,
        }
    }

    fn toolbar_target_at(&self, x: f32, y: f32) -> Option<ShellToolbarTarget> {
        let metrics = self.toolbar_metrics();
        if !metrics.bounds.contains(x, y) {
            return None;
        }
        if metrics.back.contains(x, y) {
            return Some(ShellToolbarTarget::Back);
        }
        if metrics.forward.contains(x, y) {
            return Some(ShellToolbarTarget::Forward);
        }
        if metrics.reload.contains(x, y) {
            return Some(ShellToolbarTarget::Reload);
        }
        if metrics.grant.contains(x, y) {
            return Some(ShellToolbarTarget::Grant);
        }
        if metrics.address.width > 0.0 && metrics.address.contains(x, y) {
            return Some(ShellToolbarTarget::Address);
        }
        Some(ShellToolbarTarget::Strip)
    }

    fn cursor_toolbar_target(&self) -> Option<ShellToolbarTarget> {
        self.toolbar_target_at(self.cursor_x.get(), self.cursor_y.get())
    }

    fn cursor_over_toolbar(&self) -> bool {
        self.cursor_toolbar_target().is_some()
    }

    fn handle_toolbar_mouse_input(
        &self,
        button_state: ElementState,
        button: WinitMouseButton,
    ) -> bool {
        if self.toolbar_mouse_capture.get() {
            if button_state == ElementState::Released {
                self.toolbar_mouse_capture.set(false);
            }
            return true;
        }

        let Some(target) = self.cursor_toolbar_target() else {
            return false;
        };
        if button_state != ElementState::Pressed {
            return false;
        }

        self.toolbar_mouse_capture.set(true);
        if button != WinitMouseButton::Left {
            return true;
        }

        match target {
            ShellToolbarTarget::Back => {
                self.navigate_back();
            }
            ShellToolbarTarget::Forward => {
                self.navigate_forward();
            }
            ShellToolbarTarget::Reload => {
                self.reload_current_page();
            }
            ShellToolbarTarget::Address => {
                self.focus_address_entry_from_pointer();
            }
            ShellToolbarTarget::Grant => {
                self.grant_current_tab_to_copilot();
            }
            ShellToolbarTarget::Strip => {}
        }
        true
    }

    fn draw_toolbar_overlay(&self) -> std::result::Result<(), String> {
        self.rendering_context
            .make_current()
            .map_err(|error| format!("toolbar make_current failed: {error:?}"))?;
        let gl = self.rendering_context.glow_gl_api();
        let physical_size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let metrics = self.toolbar_metrics();
        let (can_go_back, can_go_forward) = self.webview_nav_state();
        let address_active = self.address_entry.borrow().is_some();
        let address_error = self.address_error.get();
        let copilot_error = self.copilot_grant_error.borrow().is_some();
        let copilot_granted = self.copilot_granted.get();
        let pixmap = self.render_toolbar_canvas(
            physical_size,
            scale,
            metrics,
            can_go_back,
            can_go_forward,
            address_active,
            address_error,
            copilot_granted,
            copilot_error,
        )?;

        self.rendering_context.prepare_for_rendering();
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);
        }
        draw_toolbar_pixmap_overlay(&gl, physical_size, &pixmap)?;
        Ok(())
    }

    fn render_toolbar_canvas(
        &self,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        metrics: ShellToolbarMetrics,
        can_go_back: bool,
        can_go_forward: bool,
        address_active: bool,
        address_error: bool,
        copilot_granted: bool,
        copilot_error: bool,
    ) -> std::result::Result<Pixmap, String> {
        let width = physical_size.width.max(1);
        let height = ((SHELL_TOOLBAR_HEIGHT * scale).ceil() as u32).max(1);
        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| "failed to allocate toolbar pixmap".to_string())?;

        fill_canvas_rect_physical(
            &mut pixmap,
            0.0,
            0.0,
            width as f32,
            height as f32,
            ToolbarColor(0.955, 0.962, 0.972, 1.0),
        );
        fill_canvas_rect(
            &mut pixmap,
            scale,
            ToolbarRect {
                x: 0.0,
                y: SHELL_TOOLBAR_HEIGHT - 1.0,
                width: metrics.bounds.width,
                height: 1.0,
            },
            ToolbarColor(0.78, 0.81, 0.86, 1.0),
        );

        self.draw_canvas_toolbar_button(&mut pixmap, scale, metrics.back, can_go_back);
        self.draw_canvas_back_icon(&mut pixmap, scale, metrics.back, can_go_back);
        self.draw_canvas_toolbar_button(&mut pixmap, scale, metrics.forward, can_go_forward);
        self.draw_canvas_forward_icon(&mut pixmap, scale, metrics.forward, can_go_forward);
        self.draw_canvas_toolbar_button(&mut pixmap, scale, metrics.reload, true);
        self.draw_canvas_reload_icon(&mut pixmap, scale, metrics.reload);
        self.draw_canvas_address_bar(
            &mut pixmap,
            scale,
            metrics.address,
            address_active,
            address_error,
        );
        self.draw_canvas_grant_button(
            &mut pixmap,
            scale,
            metrics.grant,
            copilot_granted,
            copilot_error,
        );

        Ok(pixmap)
    }

    fn draw_canvas_toolbar_button(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let border = if enabled {
            ToolbarColor(0.68, 0.72, 0.78, 1.0)
        } else {
            ToolbarColor(0.80, 0.83, 0.88, 1.0)
        };
        let fill = if enabled {
            ToolbarColor(0.992, 0.995, 1.0, 1.0)
        } else {
            ToolbarColor(0.930, 0.940, 0.955, 1.0)
        };
        fill_canvas_soft_bordered_rect(pixmap, scale, rect, border, fill);
    }

    fn draw_canvas_back_icon(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let color = toolbar_glyph_color(enabled);
        let x = rect.x * scale;
        let y = rect.y * scale;
        stroke_canvas_polyline(
            pixmap,
            &[
                (x + 20.0 * scale, y + 9.5 * scale),
                (x + 12.0 * scale, y + 16.0 * scale),
                (x + 20.0 * scale, y + 22.5 * scale),
            ],
            2.5 * scale,
            color,
        );
    }

    fn draw_canvas_forward_icon(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let color = toolbar_glyph_color(enabled);
        let x = rect.x * scale;
        let y = rect.y * scale;
        stroke_canvas_polyline(
            pixmap,
            &[
                (x + 12.0 * scale, y + 9.5 * scale),
                (x + 20.0 * scale, y + 16.0 * scale),
                (x + 12.0 * scale, y + 22.5 * scale),
            ],
            2.5 * scale,
            color,
        );
    }

    fn draw_canvas_reload_icon(&self, pixmap: &mut Pixmap, scale: f32, rect: ToolbarRect) {
        let color = toolbar_glyph_color(true);
        let x = rect.x * scale;
        let y = rect.y * scale;
        let mut pb = PathBuilder::new();
        pb.move_to(x + 21.5 * scale, y + 10.5 * scale);
        pb.cubic_to(
            x + 17.0 * scale,
            y + 7.0 * scale,
            x + 10.0 * scale,
            y + 9.0 * scale,
            x + 9.0 * scale,
            y + 15.5 * scale,
        );
        pb.cubic_to(
            x + 8.0 * scale,
            y + 23.0 * scale,
            x + 17.5 * scale,
            y + 26.0 * scale,
            x + 22.5 * scale,
            y + 20.5 * scale,
        );
        if let Some(path) = pb.finish() {
            let mut stroke = Stroke::default();
            stroke.width = 2.4 * scale;
            stroke.line_cap = LineCap::Round;
            stroke.line_join = LineJoin::Round;
            pixmap.stroke_path(
                &path,
                &canvas_paint(color),
                &stroke,
                Transform::identity(),
                None,
            );
        }
        fill_canvas_triangle(
            pixmap,
            [
                (x + 22.0 * scale, y + 8.0 * scale),
                (x + 24.0 * scale, y + 14.0 * scale),
                (x + 17.8 * scale, y + 12.7 * scale),
            ],
            color,
        );
    }

    fn draw_canvas_address_bar(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        active: bool,
        error: bool,
    ) {
        if rect.width <= 0.0 {
            return;
        }
        let border = if error {
            ToolbarColor(0.90, 0.24, 0.23, 1.0)
        } else if active {
            ToolbarColor(0.24, 0.46, 0.84, 1.0)
        } else {
            ToolbarColor(0.70, 0.75, 0.82, 1.0)
        };
        let fill = if error {
            ToolbarColor(1.0, 0.975, 0.975, 1.0)
        } else if active {
            ToolbarColor(0.965, 0.982, 1.0, 1.0)
        } else {
            ToolbarColor(0.995, 0.997, 1.0, 1.0)
        };
        fill_canvas_soft_bordered_rect(pixmap, scale, rect, border, fill);

        let status_color = match self.load_state.get() {
            BrowserLoadState::Starting | BrowserLoadState::Loading => {
                Some(ToolbarColor(0.95, 0.62, 0.18, 1.0))
            }
            BrowserLoadState::HeadParsed => Some(ToolbarColor(0.32, 0.53, 0.92, 1.0)),
            BrowserLoadState::Complete => None,
        };
        if let Some(status_color) = status_color {
            fill_canvas_rect(
                pixmap,
                scale,
                ToolbarRect {
                    x: rect.x + 2.0,
                    y: rect.y + rect.height - 3.0,
                    width: (rect.width - 4.0).max(0.0),
                    height: 2.0,
                },
                status_color,
            );
        }

        let is_secure = self
            .address_entry
            .borrow()
            .as_ref()
            .map(|entry| entry.text.trim_start().starts_with("https://"))
            .unwrap_or_else(|| self.current_url.borrow().as_str().starts_with("https://"));
        if is_secure {
            self.draw_canvas_lock_icon(
                pixmap,
                scale,
                ToolbarRect {
                    x: rect.x + 7.0,
                    y: rect.y + 8.0,
                    width: 14.0,
                    height: 16.0,
                },
                if active {
                    ToolbarColor(0.10, 0.36, 0.20, 1.0)
                } else {
                    ToolbarColor(0.26, 0.48, 0.34, 1.0)
                },
            );
        } else {
            self.draw_canvas_search_icon(
                pixmap,
                scale,
                ToolbarRect {
                    x: rect.x + 7.0,
                    y: rect.y + 8.0,
                    width: 14.0,
                    height: 16.0,
                },
                ToolbarColor(0.40, 0.45, 0.52, 1.0),
            );
        }

        let entry = self.address_entry.borrow().clone();
        let current_url = self.current_url.borrow().to_string();
        let (raw_text, text_color, placeholder) = if let Some(entry) = entry.as_ref() {
            (
                entry.text.as_str(),
                if error {
                    ToolbarColor(0.74, 0.12, 0.14, 1.0)
                } else {
                    ToolbarColor(0.09, 0.12, 0.18, 1.0)
                },
                false,
            )
        } else if current_url.trim().is_empty() || current_url == "about:blank" {
            (
                "Search or enter website name",
                ToolbarColor(0.45, 0.49, 0.56, 1.0),
                true,
            )
        } else {
            (
                current_url.as_str(),
                ToolbarColor(0.16, 0.19, 0.25, 1.0),
                false,
            )
        };
        let text_x = rect.x + 28.0;
        let text_y = rect.y + (rect.height - SHELL_TOOLBAR_TEXT_SIZE) / 2.0 - 0.5;
        let max_text_width = (rect.width - 43.0).max(0.0);
        let display_text =
            fit_toolbar_canvas_text(raw_text, max_text_width, SHELL_TOOLBAR_TEXT_SIZE);
        let visible_selection = entry.as_ref().and_then(|entry| {
            let (start, end) = entry.selection_range();
            visible_selection_range(&display_text, start, end)
        });
        if let Some((selection_start, selection_end)) = visible_selection {
            let selection_x = text_x
                + toolbar_canvas_text_width_lossy(
                    &display_text[..selection_start],
                    SHELL_TOOLBAR_TEXT_SIZE,
                );
            let selection_width = toolbar_canvas_text_width_lossy(
                &display_text[selection_start..selection_end],
                SHELL_TOOLBAR_TEXT_SIZE,
            )
            .max(2.0);
            fill_canvas_rounded_rect(
                pixmap,
                scale,
                ToolbarRect {
                    x: selection_x - 1.5,
                    y: text_y - 2.0,
                    width: selection_width + 3.0,
                    height: SHELL_TOOLBAR_TEXT_SIZE + 5.0,
                },
                3.0,
                ToolbarColor(0.24, 0.46, 0.84, 0.92),
            );
            let mut segment_x = text_x;
            segment_x = draw_toolbar_canvas_text(
                pixmap,
                scale,
                segment_x,
                text_y,
                &display_text[..selection_start],
                SHELL_TOOLBAR_TEXT_SIZE,
                text_color,
            );
            segment_x = draw_toolbar_canvas_text(
                pixmap,
                scale,
                segment_x,
                text_y,
                &display_text[selection_start..selection_end],
                SHELL_TOOLBAR_TEXT_SIZE,
                ToolbarColor(1.0, 1.0, 1.0, 1.0),
            );
            draw_toolbar_canvas_text(
                pixmap,
                scale,
                segment_x,
                text_y,
                &display_text[selection_end..],
                SHELL_TOOLBAR_TEXT_SIZE,
                text_color,
            );
        } else {
            draw_toolbar_canvas_text(
                pixmap,
                scale,
                text_x,
                text_y,
                &display_text,
                SHELL_TOOLBAR_TEXT_SIZE,
                text_color,
            );
        }
        if active && !placeholder && visible_selection.is_none() {
            let caret_index = entry
                .as_ref()
                .map(AddressEntryState::caret)
                .unwrap_or(display_text.len());
            let caret_index =
                clamp_to_char_boundary(&display_text, caret_index.min(display_text.len()));
            let caret_x = (text_x
                + toolbar_canvas_text_width_lossy(
                    &display_text[..caret_index],
                    SHELL_TOOLBAR_TEXT_SIZE,
                ))
            .min(rect.x + rect.width - 10.0);
            fill_canvas_rect(
                pixmap,
                scale,
                ToolbarRect {
                    x: caret_x + 1.0,
                    y: rect.y + 8.0,
                    width: 1.5,
                    height: rect.height - 16.0,
                },
                if error {
                    ToolbarColor(0.74, 0.12, 0.14, 1.0)
                } else {
                    ToolbarColor(0.20, 0.44, 0.88, 1.0)
                },
            );
        }
    }

    fn draw_canvas_search_icon(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        color: ToolbarColor,
    ) {
        let x = rect.x * scale;
        let y = rect.y * scale;
        let radius = 4.6 * scale;
        if let Some(circle) = PathBuilder::from_circle(x + 6.5 * scale, y + 6.5 * scale, radius) {
            let mut stroke = Stroke::default();
            stroke.width = 1.8 * scale;
            pixmap.stroke_path(
                &circle,
                &canvas_paint(color),
                &stroke,
                Transform::identity(),
                None,
            );
        }
        stroke_canvas_polyline(
            pixmap,
            &[
                (x + 10.2 * scale, y + 10.2 * scale),
                (x + 14.0 * scale, y + 14.0 * scale),
            ],
            1.9 * scale,
            color,
        );
    }

    fn draw_canvas_lock_icon(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        color: ToolbarColor,
    ) {
        let x = rect.x * scale;
        let y = rect.y * scale;
        let mut shackle = PathBuilder::new();
        shackle.move_to(x + 3.0 * scale, y + 7.2 * scale);
        shackle.cubic_to(
            x + 3.0 * scale,
            y + 1.8 * scale,
            x + 11.0 * scale,
            y + 1.8 * scale,
            x + 11.0 * scale,
            y + 7.2 * scale,
        );
        if let Some(path) = shackle.finish() {
            let mut stroke = Stroke::default();
            stroke.width = 1.8 * scale;
            stroke.line_cap = LineCap::Round;
            pixmap.stroke_path(
                &path,
                &canvas_paint(color),
                &stroke,
                Transform::identity(),
                None,
            );
        }
        fill_canvas_rounded_rect_physical(
            pixmap,
            x + 2.0 * scale,
            y + 7.0 * scale,
            10.0 * scale,
            8.0 * scale,
            2.2 * scale,
            color,
        );
    }

    fn draw_canvas_grant_button(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        rect: ToolbarRect,
        granted: bool,
        error: bool,
    ) {
        let (border, fill, glyph, label, label_color) = if error {
            (
                ToolbarColor(0.78, 0.24, 0.24, 1.0),
                ToolbarColor(1.0, 0.94, 0.94, 1.0),
                ToolbarColor(0.78, 0.20, 0.20, 1.0),
                "Error",
                ToolbarColor(0.60, 0.12, 0.12, 1.0),
            )
        } else if granted {
            (
                ToolbarColor(0.28, 0.60, 0.42, 1.0),
                ToolbarColor(0.92, 0.985, 0.95, 1.0),
                ToolbarColor(0.18, 0.52, 0.34, 1.0),
                "On",
                ToolbarColor(0.12, 0.34, 0.24, 1.0),
            )
        } else {
            (
                ToolbarColor(0.68, 0.72, 0.78, 1.0),
                ToolbarColor(0.992, 0.995, 1.0, 1.0),
                ToolbarColor(0.42, 0.46, 0.54, 1.0),
                "Agent",
                ToolbarColor(0.20, 0.24, 0.30, 1.0),
            )
        };
        fill_canvas_soft_bordered_rect(pixmap, scale, rect, border, fill);

        let cx = (rect.x + 13.0) * scale;
        let cy = (rect.y + rect.height / 2.0) * scale;
        if let Some(outer) = PathBuilder::from_circle(cx, cy, 4.7 * scale) {
            pixmap.fill_path(
                &outer,
                &canvas_paint(glyph),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        if granted {
            stroke_canvas_polyline(
                pixmap,
                &[
                    (cx - 3.0 * scale, cy - 0.2 * scale),
                    (cx - 0.8 * scale, cy + 2.2 * scale),
                    (cx + 3.3 * scale, cy - 2.7 * scale),
                ],
                1.5 * scale,
                ToolbarColor(1.0, 1.0, 1.0, 1.0),
            );
        } else if error {
            stroke_canvas_polyline(
                pixmap,
                &[
                    (cx - 2.5 * scale, cy - 2.5 * scale),
                    (cx + 2.5 * scale, cy + 2.5 * scale),
                ],
                1.6 * scale,
                ToolbarColor(1.0, 1.0, 1.0, 1.0),
            );
            stroke_canvas_polyline(
                pixmap,
                &[
                    (cx + 2.5 * scale, cy - 2.5 * scale),
                    (cx - 2.5 * scale, cy + 2.5 * scale),
                ],
                1.6 * scale,
                ToolbarColor(1.0, 1.0, 1.0, 1.0),
            );
        }

        if rect.width >= 72.0 {
            draw_toolbar_canvas_text(
                pixmap,
                scale,
                rect.x + 24.0,
                rect.y + (rect.height - SHELL_TOOLBAR_LABEL_SIZE) / 2.0 - 0.5,
                label,
                SHELL_TOOLBAR_LABEL_SIZE,
                label_color,
            );
        }
    }

    #[allow(dead_code)]
    fn draw_toolbar_button(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let border = if enabled {
            ToolbarColor(0.68, 0.72, 0.78, 1.0)
        } else {
            ToolbarColor(0.80, 0.83, 0.88, 1.0)
        };
        let fill = if enabled {
            ToolbarColor(0.992, 0.995, 1.0, 1.0)
        } else {
            ToolbarColor(0.930, 0.940, 0.955, 1.0)
        };
        draw_soft_bordered_rect(gl, physical_size, scale, rect, border, fill);
    }

    #[allow(dead_code)]
    fn draw_back_icon(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let color = toolbar_glyph_color(enabled);
        for segment in [
            ToolbarRect {
                x: rect.x + 17.0,
                y: rect.y + 9.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 13.0,
                y: rect.y + 13.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 9.0,
                y: rect.y + 16.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 13.0,
                y: rect.y + 19.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 17.0,
                y: rect.y + 23.0,
                width: 4.0,
                height: 4.0,
            },
        ] {
            fill_logical_rect(gl, physical_size, scale, segment, color);
        }
    }

    #[allow(dead_code)]
    fn draw_forward_icon(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        enabled: bool,
    ) {
        let color = toolbar_glyph_color(enabled);
        for segment in [
            ToolbarRect {
                x: rect.x + 11.0,
                y: rect.y + 9.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 15.0,
                y: rect.y + 13.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 19.0,
                y: rect.y + 16.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 15.0,
                y: rect.y + 19.0,
                width: 4.0,
                height: 4.0,
            },
            ToolbarRect {
                x: rect.x + 11.0,
                y: rect.y + 23.0,
                width: 4.0,
                height: 4.0,
            },
        ] {
            fill_logical_rect(gl, physical_size, scale, segment, color);
        }
    }

    #[allow(dead_code)]
    fn draw_reload_icon(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
    ) {
        let color = toolbar_glyph_color(true);
        for segment in [
            ToolbarRect {
                x: rect.x + 10.0,
                y: rect.y + 9.0,
                width: 13.0,
                height: 3.0,
            },
            ToolbarRect {
                x: rect.x + 20.0,
                y: rect.y + 9.0,
                width: 3.0,
                height: 10.0,
            },
            ToolbarRect {
                x: rect.x + 9.0,
                y: rect.y + 20.0,
                width: 13.0,
                height: 3.0,
            },
            ToolbarRect {
                x: rect.x + 9.0,
                y: rect.y + 13.0,
                width: 3.0,
                height: 10.0,
            },
        ] {
            fill_logical_rect(gl, physical_size, scale, segment, color);
        }
    }

    #[allow(dead_code)]
    fn draw_address_bar(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        active: bool,
        error: bool,
    ) {
        if rect.width <= 0.0 {
            return;
        }
        let border = if error {
            ToolbarColor(0.90, 0.24, 0.23, 1.0)
        } else if active {
            ToolbarColor(0.24, 0.46, 0.84, 1.0)
        } else {
            ToolbarColor(0.70, 0.75, 0.82, 1.0)
        };
        let fill = if error {
            ToolbarColor(1.0, 0.975, 0.975, 1.0)
        } else if active {
            ToolbarColor(0.965, 0.982, 1.0, 1.0)
        } else {
            ToolbarColor(0.995, 0.997, 1.0, 1.0)
        };
        draw_soft_bordered_rect(gl, physical_size, scale, rect, border, fill);
        let status_color = match self.load_state.get() {
            BrowserLoadState::Starting | BrowserLoadState::Loading => {
                Some(ToolbarColor(0.95, 0.62, 0.18, 1.0))
            }
            BrowserLoadState::HeadParsed => Some(ToolbarColor(0.32, 0.53, 0.92, 1.0)),
            BrowserLoadState::Complete => None,
        };
        if let Some(status_color) = status_color {
            fill_logical_rect(
                gl,
                physical_size,
                scale,
                ToolbarRect {
                    x: rect.x + 2.0,
                    y: rect.y + rect.height - 3.0,
                    width: (rect.width - 4.0).max(0.0),
                    height: 2.0,
                },
                status_color,
            );
        }
        let is_secure = self
            .address_entry
            .borrow()
            .as_ref()
            .map(|entry| entry.text.trim_start().starts_with("https://"))
            .unwrap_or_else(|| self.current_url.borrow().as_str().starts_with("https://"));
        if is_secure {
            self.draw_lock_icon(
                gl,
                physical_size,
                scale,
                ToolbarRect {
                    x: rect.x + 7.0,
                    y: rect.y + 8.0,
                    width: 14.0,
                    height: 16.0,
                },
                if active {
                    ToolbarColor(0.10, 0.36, 0.20, 1.0)
                } else {
                    ToolbarColor(0.26, 0.48, 0.34, 1.0)
                },
            );
        } else {
            self.draw_search_icon(
                gl,
                physical_size,
                scale,
                ToolbarRect {
                    x: rect.x + 7.0,
                    y: rect.y + 8.0,
                    width: 14.0,
                    height: 16.0,
                },
                ToolbarColor(0.40, 0.45, 0.52, 1.0),
            );
        }
        let entry = self.address_entry.borrow().clone();
        let current_url = self.current_url.borrow().to_string();
        let (raw_text, text_color, placeholder) = if let Some(entry) = entry.as_ref() {
            (
                entry.text.as_str(),
                if error {
                    ToolbarColor(0.74, 0.12, 0.14, 1.0)
                } else {
                    ToolbarColor(0.09, 0.12, 0.18, 1.0)
                },
                false,
            )
        } else if current_url.trim().is_empty() || current_url == "about:blank" {
            (
                "Search or enter website name",
                ToolbarColor(0.45, 0.49, 0.56, 1.0),
                true,
            )
        } else {
            (
                current_url.as_str(),
                ToolbarColor(0.16, 0.19, 0.25, 1.0),
                false,
            )
        };
        let text_x = rect.x + 27.0;
        let text_y = rect.y + (rect.height - 7.0 * SHELL_TOOLBAR_TEXT_PX) / 2.0;
        let max_text_width = (rect.width - 42.0).max(0.0);
        let display_text = fit_toolbar_text(raw_text, max_text_width, SHELL_TOOLBAR_TEXT_PX);
        let text_end = draw_toolbar_text(
            gl,
            physical_size,
            scale,
            text_x,
            text_y,
            &display_text,
            SHELL_TOOLBAR_TEXT_PX,
            text_color,
        );
        if active && !placeholder {
            let caret_x = text_end.min(rect.x + rect.width - 10.0);
            fill_logical_rect(
                gl,
                physical_size,
                scale,
                ToolbarRect {
                    x: caret_x + 1.0,
                    y: text_y,
                    width: 2.0,
                    height: 7.0 * SHELL_TOOLBAR_TEXT_PX,
                },
                if error {
                    ToolbarColor(0.74, 0.12, 0.14, 1.0)
                } else {
                    ToolbarColor(0.20, 0.44, 0.88, 1.0)
                },
            );
        }
    }

    #[allow(dead_code)]
    fn draw_search_icon(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        color: ToolbarColor,
    ) {
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 2.0,
                y: rect.y + 2.0,
                width: 8.0,
                height: 2.0,
            },
            color,
        );
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 2.0,
                y: rect.y + 2.0,
                width: 2.0,
                height: 8.0,
            },
            color,
        );
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 8.0,
                y: rect.y + 2.0,
                width: 2.0,
                height: 8.0,
            },
            color,
        );
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 2.0,
                y: rect.y + 8.0,
                width: 8.0,
                height: 2.0,
            },
            color,
        );
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 10.0,
                y: rect.y + 10.0,
                width: 5.0,
                height: 2.0,
            },
            color,
        );
    }

    #[allow(dead_code)]
    fn draw_lock_icon(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        color: ToolbarColor,
    ) {
        for segment in [
            ToolbarRect {
                x: rect.x + 4.0,
                y: rect.y + 1.0,
                width: 6.0,
                height: 2.0,
            },
            ToolbarRect {
                x: rect.x + 2.0,
                y: rect.y + 3.0,
                width: 2.0,
                height: 5.0,
            },
            ToolbarRect {
                x: rect.x + 10.0,
                y: rect.y + 3.0,
                width: 2.0,
                height: 5.0,
            },
            ToolbarRect {
                x: rect.x + 2.0,
                y: rect.y + 8.0,
                width: 10.0,
                height: 7.0,
            },
        ] {
            fill_logical_rect(gl, physical_size, scale, segment, color);
        }
    }

    #[allow(dead_code)]
    fn draw_grant_button(
        &self,
        gl: &glow::Context,
        physical_size: PhysicalSize<u32>,
        scale: f32,
        rect: ToolbarRect,
        granted: bool,
        error: bool,
    ) {
        let (border, fill, glyph, label, label_color) = if error {
            (
                ToolbarColor(0.78, 0.24, 0.24, 1.0),
                ToolbarColor(1.0, 0.94, 0.94, 1.0),
                ToolbarColor(0.78, 0.20, 0.20, 1.0),
                "Error",
                ToolbarColor(0.60, 0.12, 0.12, 1.0),
            )
        } else if granted {
            (
                ToolbarColor(0.28, 0.60, 0.42, 1.0),
                ToolbarColor(0.92, 0.985, 0.95, 1.0),
                ToolbarColor(0.18, 0.52, 0.34, 1.0),
                "On",
                ToolbarColor(0.12, 0.34, 0.24, 1.0),
            )
        } else {
            (
                ToolbarColor(0.68, 0.72, 0.78, 1.0),
                ToolbarColor(0.992, 0.995, 1.0, 1.0),
                ToolbarColor(0.42, 0.46, 0.54, 1.0),
                "Agent",
                ToolbarColor(0.20, 0.24, 0.30, 1.0),
            )
        };
        draw_soft_bordered_rect(gl, physical_size, scale, rect, border, fill);
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 9.0,
                y: rect.y + 10.0,
                width: 7.0,
                height: 7.0,
            },
            glyph,
        );
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + 11.0,
                y: rect.y + 17.0,
                width: 3.0,
                height: 5.0,
            },
            glyph,
        );
        if rect.width >= 72.0 {
            draw_toolbar_text(
                gl,
                physical_size,
                scale,
                rect.x + 23.0,
                rect.y + (rect.height - 7.0 * SHELL_TOOLBAR_LABEL_PX) / 2.0,
                label,
                SHELL_TOOLBAR_LABEL_PX,
                label_color,
            );
        }
    }

    fn report_toolbar_draw_error_once(&self, error: String) {
        if self.toolbar_draw_error_reported.replace(true) {
            return;
        }
        eprintln!("SACCADE_TOOLBAR_OVERLAY disabled: {error}");
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
        let (can_go_back, can_go_forward) = self.webview_nav_state();

        self.window.set_title(&format_shell_title(ShellTitleParts {
            profile: self.rendering_settings.profile.name(),
            load_state: self.load_state.get(),
            can_go_back,
            can_go_forward,
            page_title: page_title.as_deref(),
            current_url: current_url.as_str(),
            copilot_label: copilot_label.as_str(),
            address_entry: address_entry.as_ref().map(|entry| AddressEntryTitle {
                input: entry.text.as_str(),
                invalid: self.address_error.get(),
            }),
            active_select_label: active_select_label.as_deref(),
        }));
        self.window.request_redraw();
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

    fn webview_nav_state(&self) -> (bool, bool) {
        self.webview
            .borrow()
            .as_ref()
            .map(|webview| (webview.can_go_back(), webview.can_go_forward()))
            .unwrap_or((false, false))
    }

    fn bump_control_page_revision(&self) {
        self.control_page_revision
            .set(self.control_page_revision.get().saturating_add(1));
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
        *self.address_entry.borrow_mut() = Some(AddressEntryState::new_selected(
            self.current_url.borrow().to_string(),
        ));
        self.address_error.set(false);
        self.update_window_title();
    }

    fn focus_address_entry_from_pointer(&self) {
        if self.address_entry.borrow().is_none() {
            self.begin_address_entry();
            return;
        }

        let metrics = self.toolbar_metrics();
        let text_x = metrics.address.x + 28.0;
        let max_text_width = (metrics.address.width - 43.0).max(0.0);
        let click_x = self.cursor_x.get();
        if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
            let index = address_text_index_for_x(
                &entry.text,
                click_x,
                text_x,
                max_text_width,
                SHELL_TOOLBAR_TEXT_SIZE,
            );
            entry.collapse_to(index);
        }
        self.address_error.set(false);
        self.update_window_title();
    }

    fn cancel_address_entry(&self) {
        self.address_entry.borrow_mut().take();
        self.address_error.set(false);
        self.update_window_title();
    }

    fn submit_address_entry(&self) {
        let Some(entry) = self.address_entry.borrow().clone() else {
            return;
        };
        let Ok(url) = parse_location_input(&entry.text) else {
            self.address_error.set(true);
            self.update_window_title();
            return;
        };

        self.navigate_to_url(url);
    }

    fn navigate_to_url(&self, url: Url) {
        self.address_entry.borrow_mut().take();
        self.active_select.borrow_mut().take();
        self.address_error.set(false);
        self.load_state.set(BrowserLoadState::Loading);
        *self.page_title.borrow_mut() = None;
        *self.current_url.borrow_mut() = url.clone();
        self.bump_control_page_revision();
        if let Some(webview) = self.webview.borrow().as_ref().cloned() {
            webview.load(url);
        }
        self.update_window_title();
    }

    fn reload_current_page(&self) -> bool {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return false;
        };
        self.load_state.set(BrowserLoadState::Loading);
        self.bump_control_page_revision();
        webview.reload();
        self.update_window_title();
        true
    }

    fn navigate_back(&self) -> bool {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return false;
        };
        if webview.can_go_back() {
            self.load_state.set(BrowserLoadState::Loading);
            self.bump_control_page_revision();
            webview.go_back(1);
            self.update_window_title();
            return true;
        }
        self.update_window_title();
        false
    }

    fn navigate_forward(&self) -> bool {
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return false;
        };
        if webview.can_go_forward() {
            self.load_state.set(BrowserLoadState::Loading);
            self.bump_control_page_revision();
            webview.go_forward(1);
            self.update_window_title();
            return true;
        }
        self.update_window_title();
        false
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

        let modifiers = self.modifiers.get();
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
                    entry.backspace();
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            WinitKey::Named(WinitNamedKey::Delete) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.delete_forward();
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowLeft) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.move_left(modifiers.shift_key());
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowRight) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.move_right(modifiers.shift_key());
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            WinitKey::Named(WinitNamedKey::Home) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.move_to_start(modifiers.shift_key());
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            WinitKey::Named(WinitNamedKey::End) => {
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.move_to_end(modifiers.shift_key());
                }
                self.address_error.set(false);
                self.update_window_title();
                true
            }
            _ => {
                if address_select_all_shortcut(event, modifiers)
                    || address_focus_shortcut(event, modifiers)
                {
                    if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                        entry.select_all();
                    }
                    self.address_error.set(false);
                    self.update_window_title();
                    return true;
                }
                if modifiers.super_key() || modifiers.control_key() || modifiers.alt_key() {
                    return true;
                }
                let Some(text) = typed_text(event) else {
                    return true;
                };
                if let Some(entry) = self.address_entry.borrow_mut().as_mut() {
                    entry.replace_selection(&text);
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
                    "toolbar": self.control_toolbar_status_response(),
                },
            }),
            "shell_status" => json!({
                "id": id,
                "ok": true,
                "result": self.control_shell_status_response(
                    "saccade-dogfood-control-shell-status-v0",
                    "dogfood browser shell status collected from the same live WebView",
                ),
            }),
            "navigate" => self.handle_control_navigate(id, _params),
            "back" => self.handle_control_back(id),
            "forward" => self.handle_control_forward(id),
            "reload" => self.handle_control_reload(id),
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

    fn handle_control_navigate(&self, id: Value, params: Value) -> Value {
        let Some(url) = params.get("url").and_then(Value::as_str) else {
            return json!({
                "id": id,
                "ok": false,
                "error": "navigate requires string params.url",
            });
        };
        match parse_location_input(url) {
            Ok(url) => {
                self.navigate_to_url(url);
                json!({
                    "id": id,
                    "ok": true,
                    "result": self.control_shell_status_response(
                        "saccade-dogfood-control-shell-navigate-v0",
                        "dogfood browser shell navigation dispatched through the same live WebView",
                    ),
                })
            }
            Err(error) => json!({
                "id": id,
                "ok": false,
                "error": format!("invalid navigation URL: {error}"),
            }),
        }
    }

    fn handle_control_back(&self, id: Value) -> Value {
        let changed = self.navigate_back();
        let mut result = self.control_shell_status_response(
            "saccade-dogfood-control-shell-back-v0",
            "dogfood browser shell back command dispatched through the same live WebView",
        );
        if let Some(object) = result.as_object_mut() {
            object.insert("changed".into(), json!(changed));
        }
        json!({
            "id": id,
            "ok": true,
            "result": result,
        })
    }

    fn handle_control_forward(&self, id: Value) -> Value {
        let changed = self.navigate_forward();
        let mut result = self.control_shell_status_response(
            "saccade-dogfood-control-shell-forward-v0",
            "dogfood browser shell forward command dispatched through the same live WebView",
        );
        if let Some(object) = result.as_object_mut() {
            object.insert("changed".into(), json!(changed));
        }
        json!({
            "id": id,
            "ok": true,
            "result": result,
        })
    }

    fn handle_control_reload(&self, id: Value) -> Value {
        let changed = self.reload_current_page();
        let mut result = self.control_shell_status_response(
            "saccade-dogfood-control-shell-reload-v0",
            "dogfood browser shell reload command dispatched through the same live WebView",
        );
        if let Some(object) = result.as_object_mut() {
            object.insert("changed".into(), json!(changed));
        }
        json!({
            "id": id,
            "ok": true,
            "result": result,
        })
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

    fn control_shell_status_response(&self, engine: &str, summary: &str) -> Value {
        let current_url = self.current_url.borrow().to_string();
        let page_title = self.page_title.borrow().clone();
        let (can_go_back, can_go_forward) = self.webview_nav_state();
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v0",
            "engine": engine,
            "summary": summary,
            "same_webview_control": true,
            "rendering_profile": self.rendering_settings.profile.name(),
            "renderer_engine": self.rendering_settings.profile.engine(),
            "servo_grid_enabled": self.rendering_settings.layout_grid_enabled,
            "url": current_url,
            "title": page_title,
            "load_state": self.load_state.get().label(),
            "page_revision": self.control_page_revision.get(),
            "can_go_back": can_go_back,
            "can_go_forward": can_go_forward,
            "copilot_granted": self.copilot_granted.get(),
            "toolbar": self.control_toolbar_status_response(),
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

    fn control_toolbar_status_response(&self) -> Value {
        let metrics = self.toolbar_metrics();
        let (can_go_back, can_go_forward) = self.webview_nav_state();
        let address_entry = self.address_entry.borrow().clone();
        let address_selection = address_entry.as_ref().map(|entry| {
            let (start, end) = entry.selection_range();
            json!({
                "start": start,
                "end": end,
                "caret": entry.caret(),
                "has_selection": entry.has_selection(),
            })
        });
        json!({
            "visible": true,
            "clickable": true,
            "draw_mode": "native_gl_antialiased_toolbar_v3",
            "page_dom_injected": false,
            "webview_resized_for_toolbar": false,
            "height_css_px": SHELL_TOOLBAR_HEIGHT,
            "targets": {
                "back": {
                    "enabled": can_go_back,
                    "rect": toolbar_rect_json(metrics.back),
                },
                "forward": {
                    "enabled": can_go_forward,
                    "rect": toolbar_rect_json(metrics.forward),
                },
                "reload": {
                    "enabled": self.webview.borrow().is_some(),
                    "rect": toolbar_rect_json(metrics.reload),
                },
                "address": {
                    "enabled": self.webview.borrow().is_some(),
                    "active": address_entry.is_some(),
                    "invalid": self.address_error.get(),
                    "selection": address_selection,
                    "rect": toolbar_rect_json(metrics.address),
                },
                "copilot_grant": {
                    "enabled": true,
                    "granted": self.copilot_granted.get(),
                    "error": self.copilot_grant_error.borrow().is_some(),
                    "rect": toolbar_rect_json(metrics.grant),
                },
            },
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
            toolbar_mouse_capture: Cell::new(false),
            toolbar_draw_error_reported: Cell::new(false),
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
                        if let Err(error) = state.draw_toolbar_overlay() {
                            state.report_toolbar_draw_error_once(error);
                        }
                        state.rendering_context.present();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview
                            .set_hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32));
                        webview.resize(new_size);
                        state.window.request_redraw();
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    state.store_cursor_page_position(position);
                    state
                        .cursor_move_count
                        .set(state.cursor_move_count.get().saturating_add(1));
                    state.last_cursor_move_at.set(Some(Instant::now()));
                    state.trace_cursor_moved(position);
                    if state.cursor_over_toolbar() {
                        return;
                    }
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
                    if state.handle_toolbar_mouse_input(button_state, button) {
                        return;
                    }
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
                    if state.cursor_over_toolbar() {
                        return;
                    }
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

fn canvas_paint(color: ToolbarColor) -> Paint<'static> {
    let (red, green, blue, alpha) = color.to_rgba8();
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(red, green, blue, alpha);
    paint
}

fn fill_canvas_rect(pixmap: &mut Pixmap, scale: f32, rect: ToolbarRect, color: ToolbarColor) {
    fill_canvas_rect_physical(
        pixmap,
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
        color,
    );
}

fn fill_canvas_rect_physical(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: ToolbarColor,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    if let Some(rect) = SkiaRect::from_xywh(x, y, width, height) {
        pixmap.fill_rect(rect, &canvas_paint(color), Transform::identity(), None);
    }
}

fn fill_canvas_soft_bordered_rect(
    pixmap: &mut Pixmap,
    scale: f32,
    rect: ToolbarRect,
    border: ToolbarColor,
    fill: ToolbarColor,
) {
    let radius = toolbar_control_radius(rect);
    fill_canvas_rounded_rect(pixmap, scale, rect, radius, border);
    let inner = rect.inset(1.0);
    fill_canvas_rounded_rect(pixmap, scale, inner, (radius - 1.0).max(0.0), fill);
}

fn fill_canvas_rounded_rect(
    pixmap: &mut Pixmap,
    scale: f32,
    rect: ToolbarRect,
    radius: f32,
    color: ToolbarColor,
) {
    fill_canvas_rounded_rect_physical(
        pixmap,
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
        radius * scale,
        color,
    );
}

fn fill_canvas_rounded_rect_physical(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: ToolbarColor,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let path = rounded_canvas_rect_path(x, y, width, height, radius);
    pixmap.fill_path(
        &path,
        &canvas_paint(color),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn rounded_canvas_rect_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> tiny_skia::Path {
    if radius <= 0.0 {
        return PathBuilder::from_rect(SkiaRect::from_xywh(x, y, width, height).unwrap());
    }

    let right = x + width;
    let bottom = y + height;
    let k = 0.552_284_8;
    let c = radius * k;
    let mut path = PathBuilder::new();
    path.move_to(x + radius, y);
    path.line_to(right - radius, y);
    path.cubic_to(
        right - radius + c,
        y,
        right,
        y + radius - c,
        right,
        y + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + c,
        right - radius + c,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(x + radius, bottom);
    path.cubic_to(
        x + radius - c,
        bottom,
        x,
        bottom - radius + c,
        x,
        bottom - radius,
    );
    path.line_to(x, y + radius);
    path.cubic_to(x, y + radius - c, x + radius - c, y, x + radius, y);
    path.close();
    path.finish().unwrap()
}

fn stroke_canvas_polyline(
    pixmap: &mut Pixmap,
    points: &[(f32, f32)],
    width: f32,
    color: ToolbarColor,
) {
    if points.len() < 2 || width <= 0.0 {
        return;
    }
    let mut path = PathBuilder::new();
    path.move_to(points[0].0, points[0].1);
    for point in points.iter().skip(1) {
        path.line_to(point.0, point.1);
    }
    if let Some(path) = path.finish() {
        let mut stroke = Stroke::default();
        stroke.width = width;
        stroke.line_cap = LineCap::Round;
        stroke.line_join = LineJoin::Round;
        pixmap.stroke_path(
            &path,
            &canvas_paint(color),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn fill_canvas_triangle(pixmap: &mut Pixmap, points: [(f32, f32); 3], color: ToolbarColor) {
    let mut path = PathBuilder::new();
    path.move_to(points[0].0, points[0].1);
    path.line_to(points[1].0, points[1].1);
    path.line_to(points[2].0, points[2].1);
    path.close();
    if let Some(path) = path.finish() {
        pixmap.fill_path(
            &path,
            &canvas_paint(color),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn toolbar_font() -> Option<&'static FontArc> {
    static TOOLBAR_FONT: OnceLock<Option<FontArc>> = OnceLock::new();
    TOOLBAR_FONT.get_or_init(load_toolbar_font).as_ref()
}

fn load_toolbar_font() -> Option<FontArc> {
    for path in TOOLBAR_FONT_PATHS {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Ok(font) = FontArc::try_from_vec(bytes)
            && toolbar_font_has_drawable_ascii(&font)
        {
            return Some(font);
        }
    }
    None
}

fn toolbar_font_has_drawable_ascii(font: &FontArc) -> bool {
    let scale = PxScale::from(16.0);
    let scaled = font.as_scaled(scale);
    ['A', 'g', '0'].into_iter().all(|ch| {
        let glyph_id = scaled.glyph_id(ch);
        scaled.h_advance(glyph_id) > 0.0
            && font
                .outline_glyph(glyph_id.with_scale_and_position(scale, point(0.0, 16.0)))
                .is_some()
    })
}

const TOOLBAR_TEXTURE_VERTEX_SHADER_150: &str = r#"#version 150
in vec2 a_pos;
in vec2 a_uv;
out vec2 v_uv;

void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const TOOLBAR_TEXTURE_FRAGMENT_SHADER_150: &str = r#"#version 150
uniform sampler2D u_texture;
in vec2 v_uv;
out vec4 out_color;

void main() {
    out_color = texture(u_texture, v_uv);
}
"#;

const TOOLBAR_TEXTURE_VERTEX_SHADER_100: &str = r#"attribute vec2 a_pos;
attribute vec2 a_uv;
varying vec2 v_uv;

void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const TOOLBAR_TEXTURE_FRAGMENT_SHADER_100: &str = r#"uniform sampler2D u_texture;
varying vec2 v_uv;

void main() {
    gl_FragColor = texture2D(u_texture, v_uv);
}
"#;

fn draw_toolbar_pixmap_overlay(
    gl: &glow::Context,
    physical_size: PhysicalSize<u32>,
    pixmap: &Pixmap,
) -> std::result::Result<(), String> {
    if physical_size.width == 0
        || physical_size.height == 0
        || pixmap.width() == 0
        || pixmap.height() == 0
    {
        return Ok(());
    }

    let toolbar_bottom = 1.0 - (pixmap.height() as f32 / physical_size.height as f32) * 2.0;
    let vertices: [f32; 24] = [
        -1.0,
        1.0,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
        0.0,
        -1.0,
        toolbar_bottom,
        0.0,
        1.0,
        -1.0,
        toolbar_bottom,
        0.0,
        1.0,
        1.0,
        1.0,
        1.0,
        0.0,
        1.0,
        toolbar_bottom,
        1.0,
        1.0,
    ];

    unsafe {
        let _state_guard = GlStateGuard::capture(gl);
        let program = compile_toolbar_texture_program(gl)?;
        let texture = gl.create_texture()?;
        let vertex_array = gl.create_vertex_array()?;
        let vertex_buffer = gl.create_buffer()?;

        gl.viewport(
            0,
            0,
            physical_size.width as i32,
            physical_size.height as i32,
        );
        gl.disable(glow::SCISSOR_TEST);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA as i32,
            pixmap.width() as i32,
            pixmap.height() as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixmap.data())),
        );

        gl.use_program(Some(program));
        let texture_uniform = gl.get_uniform_location(program, "u_texture");
        gl.uniform_1_i32(texture_uniform.as_ref(), 0);

        gl.bind_vertex_array(Some(vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            f32_slice_as_u8(&vertices),
            glow::STATIC_DRAW,
        );

        let position_location = gl
            .get_attrib_location(program, "a_pos")
            .ok_or_else(|| "toolbar texture shader missing a_pos".to_string())?;
        let uv_location = gl
            .get_attrib_location(program, "a_uv")
            .ok_or_else(|| "toolbar texture shader missing a_uv".to_string())?;
        let stride = (4 * std::mem::size_of::<f32>()) as i32;
        gl.enable_vertex_attrib_array(position_location);
        gl.vertex_attrib_pointer_f32(position_location, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(uv_location);
        gl.vertex_attrib_pointer_f32(
            uv_location,
            2,
            glow::FLOAT,
            false,
            stride,
            (2 * std::mem::size_of::<f32>()) as i32,
        );

        gl.enable(glow::BLEND);
        gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        gl.draw_arrays(glow::TRIANGLES, 0, 6);
        gl.disable(glow::BLEND);

        gl.disable_vertex_attrib_array(position_location);
        gl.disable_vertex_attrib_array(uv_location);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_vertex_array(None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);
        gl.delete_buffer(vertex_buffer);
        gl.delete_vertex_array(vertex_array);
        gl.delete_texture(texture);
        gl.delete_program(program);
    }

    Ok(())
}

struct GlStateGuard<'a> {
    gl: &'a glow::Context,
    active_texture: i32,
    texture0_binding_2d: Option<<glow::Context as glow::HasContext>::Texture>,
    sampler0_binding: Option<<glow::Context as glow::HasContext>::Sampler>,
    current_program: Option<<glow::Context as glow::HasContext>::Program>,
    vertex_array: Option<<glow::Context as glow::HasContext>::VertexArray>,
    array_buffer: Option<<glow::Context as glow::HasContext>::Buffer>,
    draw_framebuffer: Option<<glow::Context as glow::HasContext>::Framebuffer>,
    read_framebuffer: Option<<glow::Context as glow::HasContext>::Framebuffer>,
    renderbuffer: Option<<glow::Context as glow::HasContext>::Renderbuffer>,
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    blend_enabled: bool,
    depth_test_enabled: bool,
    stencil_test_enabled: bool,
    cull_face_enabled: bool,
    scissor_test_enabled: bool,
    blend_src_rgb: i32,
    blend_dst_rgb: i32,
    blend_src_alpha: i32,
    blend_dst_alpha: i32,
    blend_equation_rgb: i32,
    blend_equation_alpha: i32,
    depth_writemask: bool,
    color_writemask: [bool; 4],
}

impl<'a> GlStateGuard<'a> {
    fn capture(gl: &'a glow::Context) -> Self {
        unsafe {
            let active_texture = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
            gl.active_texture(glow::TEXTURE0);
            let texture0_binding_2d = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            let sampler0_binding = gl.get_parameter_sampler(glow::SAMPLER_BINDING);
            gl.active_texture(active_texture as u32);

            let mut viewport = [0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            let mut scissor_box = [0; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor_box);

            Self {
                gl,
                active_texture,
                texture0_binding_2d,
                sampler0_binding,
                current_program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                vertex_array: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                array_buffer: gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
                draw_framebuffer: gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING),
                read_framebuffer: gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING),
                renderbuffer: gl.get_parameter_renderbuffer(glow::RENDERBUFFER_BINDING),
                viewport,
                scissor_box,
                blend_enabled: gl.is_enabled(glow::BLEND),
                depth_test_enabled: gl.is_enabled(glow::DEPTH_TEST),
                stencil_test_enabled: gl.is_enabled(glow::STENCIL_TEST),
                cull_face_enabled: gl.is_enabled(glow::CULL_FACE),
                scissor_test_enabled: gl.is_enabled(glow::SCISSOR_TEST),
                blend_src_rgb: gl.get_parameter_i32(glow::BLEND_SRC_RGB),
                blend_dst_rgb: gl.get_parameter_i32(glow::BLEND_DST_RGB),
                blend_src_alpha: gl.get_parameter_i32(glow::BLEND_SRC_ALPHA),
                blend_dst_alpha: gl.get_parameter_i32(glow::BLEND_DST_ALPHA),
                blend_equation_rgb: gl.get_parameter_i32(glow::BLEND_EQUATION_RGB),
                blend_equation_alpha: gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA),
                depth_writemask: gl.get_parameter_bool(glow::DEPTH_WRITEMASK),
                color_writemask: gl.get_parameter_bool_array(glow::COLOR_WRITEMASK),
            }
        }
    }

    fn restore_capability(gl: &glow::Context, capability: u32, enabled: bool) {
        unsafe {
            if enabled {
                gl.enable(capability);
            } else {
                gl.disable(capability);
            }
        }
    }
}

impl Drop for GlStateGuard<'_> {
    fn drop(&mut self) {
        unsafe {
            self.gl.use_program(self.current_program);
            self.gl.bind_vertex_array(self.vertex_array);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, self.array_buffer);
            self.gl
                .bind_framebuffer(glow::DRAW_FRAMEBUFFER, self.draw_framebuffer);
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, self.read_framebuffer);
            self.gl
                .bind_renderbuffer(glow::RENDERBUFFER, self.renderbuffer);

            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, self.texture0_binding_2d);
            self.gl.bind_sampler(0, self.sampler0_binding);
            self.gl.active_texture(self.active_texture as u32);

            self.gl.viewport(
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            );
            self.gl.scissor(
                self.scissor_box[0],
                self.scissor_box[1],
                self.scissor_box[2],
                self.scissor_box[3],
            );
            self.gl.blend_func_separate(
                self.blend_src_rgb as u32,
                self.blend_dst_rgb as u32,
                self.blend_src_alpha as u32,
                self.blend_dst_alpha as u32,
            );
            self.gl.blend_equation_separate(
                self.blend_equation_rgb as u32,
                self.blend_equation_alpha as u32,
            );
            self.gl.depth_mask(self.depth_writemask);
            self.gl.color_mask(
                self.color_writemask[0],
                self.color_writemask[1],
                self.color_writemask[2],
                self.color_writemask[3],
            );

            Self::restore_capability(self.gl, glow::BLEND, self.blend_enabled);
            Self::restore_capability(self.gl, glow::DEPTH_TEST, self.depth_test_enabled);
            Self::restore_capability(self.gl, glow::STENCIL_TEST, self.stencil_test_enabled);
            Self::restore_capability(self.gl, glow::CULL_FACE, self.cull_face_enabled);
            Self::restore_capability(self.gl, glow::SCISSOR_TEST, self.scissor_test_enabled);
        }
    }
}

unsafe fn compile_toolbar_texture_program(
    gl: &glow::Context,
) -> std::result::Result<glow::NativeProgram, String> {
    unsafe {
        let mut last_error = String::new();
        for (vertex_source, fragment_source) in [
            (
                TOOLBAR_TEXTURE_VERTEX_SHADER_150,
                TOOLBAR_TEXTURE_FRAGMENT_SHADER_150,
            ),
            (
                TOOLBAR_TEXTURE_VERTEX_SHADER_100,
                TOOLBAR_TEXTURE_FRAGMENT_SHADER_100,
            ),
        ] {
            match compile_toolbar_texture_program_with_sources(gl, vertex_source, fragment_source) {
                Ok(program) => return Ok(program),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
}

unsafe fn compile_toolbar_texture_program_with_sources(
    gl: &glow::Context,
    vertex_source: &str,
    fragment_source: &str,
) -> std::result::Result<glow::NativeProgram, String> {
    unsafe {
        let vertex_shader = compile_toolbar_shader(gl, glow::VERTEX_SHADER, vertex_source)?;
        let fragment_shader =
            match compile_toolbar_shader(gl, glow::FRAGMENT_SHADER, fragment_source) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_shader(vertex_shader);
                    return Err(error);
                }
            };
        let program = match gl.create_program() {
            Ok(program) => program,
            Err(error) => {
                gl.delete_shader(vertex_shader);
                gl.delete_shader(fragment_shader);
                return Err(error);
            }
        };
        gl.attach_shader(program, vertex_shader);
        gl.attach_shader(program, fragment_shader);
        gl.link_program(program);
        gl.detach_shader(program, vertex_shader);
        gl.detach_shader(program, fragment_shader);
        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);

        if gl.get_program_link_status(program) {
            Ok(program)
        } else {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            Err(format!("toolbar texture shader link failed: {log}"))
        }
    }
}

unsafe fn compile_toolbar_shader(
    gl: &glow::Context,
    kind: u32,
    source: &str,
) -> std::result::Result<glow::NativeShader, String> {
    unsafe {
        let shader = gl.create_shader(kind)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if gl.get_shader_compile_status(shader) {
            Ok(shader)
        } else {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            Err(format!("toolbar texture shader compile failed: {log}"))
        }
    }
}

fn f32_slice_as_u8(values: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

fn toolbar_glyph_color(enabled: bool) -> ToolbarColor {
    if enabled {
        ToolbarColor(0.11, 0.14, 0.19, 1.0)
    } else {
        ToolbarColor(0.58, 0.62, 0.68, 1.0)
    }
}

#[allow(dead_code)]
fn draw_toolbar_text(
    gl: &glow::Context,
    physical_size: PhysicalSize<u32>,
    scale: f32,
    x: f32,
    y: f32,
    text: &str,
    pixel: f32,
    color: ToolbarColor,
) -> f32 {
    let mut cursor_x = x;
    for ch in text.chars() {
        let pattern = toolbar_glyph_pattern(ch);
        for (row_index, row) in pattern.iter().enumerate() {
            for (column_index, column) in row.chars().enumerate() {
                if column != '#' {
                    continue;
                }
                fill_logical_rect(
                    gl,
                    physical_size,
                    scale,
                    ToolbarRect {
                        x: cursor_x + column_index as f32 * pixel,
                        y: y + row_index as f32 * pixel,
                        width: pixel,
                        height: pixel,
                    },
                    color,
                );
            }
        }
        cursor_x += toolbar_char_advance(ch, pixel);
    }
    cursor_x
}

fn fit_toolbar_text(text: &str, max_width: f32, pixel: f32) -> String {
    if max_width <= 0.0 || pixel <= 0.0 {
        return String::new();
    }

    let normalized = normalize_toolbar_text(text);
    if toolbar_text_width(&normalized, pixel) <= max_width {
        return normalized;
    }

    let ellipsis = "...";
    let ellipsis_width = toolbar_text_width(ellipsis, pixel);
    if ellipsis_width > max_width {
        let mut dots = String::new();
        while toolbar_text_width(&(dots.clone() + "."), pixel) <= max_width {
            dots.push('.');
        }
        return dots;
    }

    let mut fitted = String::new();
    let mut fitted_width = 0.0;
    for ch in normalized.chars() {
        let next_width = toolbar_char_advance(ch, pixel);
        if fitted_width + next_width + ellipsis_width > max_width {
            break;
        }
        fitted.push(ch);
        fitted_width += next_width;
    }
    fitted.push_str(ellipsis);
    fitted
}

fn normalize_toolbar_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut previous_space = false;
    for ch in text.trim().chars() {
        let ch = if ch.is_control() || ch.is_whitespace() {
            ' '
        } else {
            ch
        };
        if ch == ' ' {
            if !previous_space {
                normalized.push(ch);
                previous_space = true;
            }
            continue;
        }
        normalized.push(ch);
        previous_space = false;
    }
    normalized
}

fn toolbar_text_width(text: &str, pixel: f32) -> f32 {
    text.chars().map(|ch| toolbar_char_advance(ch, pixel)).sum()
}

fn toolbar_char_advance(ch: char, pixel: f32) -> f32 {
    match ch {
        ' ' => 4.0 * pixel,
        '.' | ':' | '/' | '-' | '_' => 4.0 * pixel,
        _ => 6.0 * pixel,
    }
}

fn fit_toolbar_canvas_text(text: &str, max_width: f32, font_size: f32) -> String {
    if max_width <= 0.0 || font_size <= 0.0 {
        return String::new();
    }

    let normalized = normalize_toolbar_text(text);
    let Some(full_width) = toolbar_canvas_text_width(&normalized, font_size) else {
        return fit_toolbar_text(text, max_width, SHELL_TOOLBAR_TEXT_PX);
    };
    if full_width <= max_width {
        return normalized;
    }

    let ellipsis = "...";
    let ellipsis_width = toolbar_canvas_text_width(ellipsis, font_size).unwrap_or(0.0);
    if ellipsis_width > max_width {
        let mut dots = String::new();
        while toolbar_canvas_text_width(&(dots.clone() + "."), font_size).unwrap_or(f32::MAX)
            <= max_width
        {
            dots.push('.');
        }
        return dots;
    }

    let mut fitted = String::new();
    let mut fitted_width = 0.0;
    for ch in normalized.chars() {
        let next_width = toolbar_canvas_text_width(&ch.to_string(), font_size).unwrap_or(0.0);
        if fitted_width + next_width + ellipsis_width > max_width {
            break;
        }
        fitted.push(ch);
        fitted_width += next_width;
    }
    fitted.push_str(ellipsis);
    fitted
}

fn toolbar_canvas_text_width(text: &str, font_size: f32) -> Option<f32> {
    let font = toolbar_font()?;
    let scaled = font.as_scaled(PxScale::from(font_size));
    let mut width = 0.0;
    let mut previous = None;
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(previous) = previous {
            width += scaled.kern(previous, glyph_id);
        }
        width += scaled.h_advance(glyph_id);
        previous = Some(glyph_id);
    }
    Some(width)
}

fn toolbar_canvas_text_width_lossy(text: &str, font_size: f32) -> f32 {
    toolbar_canvas_text_width(text, font_size)
        .unwrap_or_else(|| toolbar_text_width(text, SHELL_TOOLBAR_TEXT_PX))
}

fn visible_selection_range(
    display_text: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(usize, usize)> {
    if selection_start == selection_end || display_text.is_empty() {
        return None;
    }
    let start = clamp_to_char_boundary(display_text, selection_start.min(display_text.len()));
    let end = clamp_to_char_boundary(display_text, selection_end.min(display_text.len()));
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

fn address_text_index_for_x(
    text: &str,
    click_x: f32,
    text_x: f32,
    max_width: f32,
    font_size: f32,
) -> usize {
    let display_text = fit_toolbar_canvas_text(text, max_width, font_size);
    if display_text.is_empty() || click_x <= text_x {
        return 0;
    }

    let normalized = normalize_toolbar_text(text);
    let visible_text = if display_text.ends_with("...") && display_text.len() < normalized.len() {
        &display_text[..display_text.len() - 3]
    } else {
        display_text.as_str()
    };
    if visible_text.is_empty() {
        return 0;
    }

    let relative_x = click_x - text_x;
    let mut char_start_x = 0.0;
    for (index, ch) in visible_text.char_indices() {
        let next_index = index + ch.len_utf8();
        let char_width =
            toolbar_canvas_text_width_lossy(&visible_text[index..next_index], font_size);
        if relative_x <= char_start_x + char_width / 2.0 {
            return clamp_to_char_boundary(text, index.min(text.len()));
        }
        char_start_x += char_width;
    }

    clamp_to_char_boundary(text, visible_text.len().min(text.len()))
}

fn draw_toolbar_canvas_text(
    pixmap: &mut Pixmap,
    scale: f32,
    x: f32,
    y: f32,
    text: &str,
    font_size: f32,
    color: ToolbarColor,
) -> f32 {
    let Some(font) = toolbar_font() else {
        return draw_toolbar_canvas_bitmap_text(
            pixmap,
            scale,
            x,
            y,
            text,
            SHELL_TOOLBAR_TEXT_PX,
            color,
        );
    };

    let px_size = font_size * scale;
    let scaled = font.as_scaled(PxScale::from(px_size));
    let mut cursor_x = x * scale;
    let baseline_y = y * scale + scaled.ascent();
    let mut previous = None;

    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(previous) = previous {
            cursor_x += scaled.kern(previous, glyph_id);
        }
        let glyph =
            glyph_id.with_scale_and_position(PxScale::from(px_size), point(cursor_x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let bounds_x = bounds.min.x as i32;
            let bounds_y = bounds.min.y as i32;
            outlined.draw(|pixel_x, pixel_y, coverage| {
                let x = bounds_x + pixel_x as i32;
                let y = bounds_y + pixel_y as i32;
                if x < 0 || y < 0 {
                    return;
                }
                blend_toolbar_canvas_pixel(
                    pixmap,
                    x as u32,
                    y as u32,
                    color,
                    coverage.clamp(0.0, 1.0),
                );
            });
        }
        cursor_x += scaled.h_advance(glyph_id);
        previous = Some(glyph_id);
    }

    cursor_x / scale
}

fn draw_toolbar_canvas_bitmap_text(
    pixmap: &mut Pixmap,
    scale: f32,
    x: f32,
    y: f32,
    text: &str,
    pixel: f32,
    color: ToolbarColor,
) -> f32 {
    let mut cursor_x = x;
    for ch in text.chars() {
        let pattern = toolbar_glyph_pattern(ch);
        for (row_index, row) in pattern.iter().enumerate() {
            for (column_index, column) in row.chars().enumerate() {
                if column != '#' {
                    continue;
                }
                fill_canvas_rect(
                    pixmap,
                    scale,
                    ToolbarRect {
                        x: cursor_x + column_index as f32 * pixel,
                        y: y + row_index as f32 * pixel,
                        width: pixel,
                        height: pixel,
                    },
                    color,
                );
            }
        }
        cursor_x += toolbar_char_advance(ch, pixel);
    }
    cursor_x
}

fn blend_toolbar_canvas_pixel(
    pixmap: &mut Pixmap,
    x: u32,
    y: u32,
    color: ToolbarColor,
    coverage: f32,
) {
    if x >= pixmap.width() || y >= pixmap.height() || coverage <= 0.0 {
        return;
    }
    let idx = ((y * pixmap.width() + x) * 4) as usize;
    let data = pixmap.data_mut();
    let src_alpha = (color.3 * coverage).clamp(0.0, 1.0);
    let inv_alpha = 1.0 - src_alpha;
    let src_r = color.0.clamp(0.0, 1.0) * src_alpha;
    let src_g = color.1.clamp(0.0, 1.0) * src_alpha;
    let src_b = color.2.clamp(0.0, 1.0) * src_alpha;
    let dst_r = data[idx] as f32 / 255.0;
    let dst_g = data[idx + 1] as f32 / 255.0;
    let dst_b = data[idx + 2] as f32 / 255.0;
    let dst_a = data[idx + 3] as f32 / 255.0;

    data[idx] = ((src_r + dst_r * inv_alpha) * 255.0).round() as u8;
    data[idx + 1] = ((src_g + dst_g * inv_alpha) * 255.0).round() as u8;
    data[idx + 2] = ((src_b + dst_b * inv_alpha) * 255.0).round() as u8;
    data[idx + 3] = ((src_alpha + dst_a * inv_alpha) * 255.0).round() as u8;
}

fn toolbar_lowercase_glyph_pattern(ch: char) -> [&'static str; 7] {
    match ch {
        'a' => [
            "     ", "     ", " ### ", "    #", " ####", "#   #", " ####",
        ],
        'b' => [
            "#    ", "#    ", "# ## ", "##  #", "#   #", "#   #", "#### ",
        ],
        'c' => [
            "     ", "     ", " ####", "#    ", "#    ", "#    ", " ####",
        ],
        'd' => [
            "    #", "    #", " ## #", "#  ##", "#   #", "#   #", " ####",
        ],
        'e' => [
            "     ", "     ", " ### ", "#   #", "#####", "#    ", " ####",
        ],
        'f' => [
            "  ###", " #   ", " #   ", "#### ", " #   ", " #   ", " #   ",
        ],
        'g' => [
            "     ", " ####", "#   #", "#   #", " ####", "    #", " ### ",
        ],
        'h' => [
            "#    ", "#    ", "# ## ", "##  #", "#   #", "#   #", "#   #",
        ],
        'i' => [
            "  #  ", "     ", " ##  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        'j' => [
            "   # ", "     ", "  ## ", "   # ", "   # ", "#  # ", " ##  ",
        ],
        'k' => [
            "#    ", "#    ", "#  # ", "# #  ", "##   ", "# #  ", "#  # ",
        ],
        'l' => [
            " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        'm' => [
            "     ", "     ", "## # ", "# # #", "# # #", "#   #", "#   #",
        ],
        'n' => [
            "     ", "     ", "# ## ", "##  #", "#   #", "#   #", "#   #",
        ],
        'o' => [
            "     ", "     ", " ### ", "#   #", "#   #", "#   #", " ### ",
        ],
        'p' => [
            "     ", "     ", "#### ", "#   #", "#### ", "#    ", "#    ",
        ],
        'q' => [
            "     ", "     ", " ####", "#   #", " ####", "    #", "    #",
        ],
        'r' => [
            "     ", "     ", "# ## ", "##  #", "#    ", "#    ", "#    ",
        ],
        's' => [
            "     ", "     ", " ####", "#    ", " ### ", "    #", "#### ",
        ],
        't' => [
            " #   ", " #   ", "#### ", " #   ", " #   ", " #   ", "  ## ",
        ],
        'u' => [
            "     ", "     ", "#   #", "#   #", "#   #", "#  ##", " ## #",
        ],
        'v' => [
            "     ", "     ", "#   #", "#   #", "#   #", " # # ", "  #  ",
        ],
        'w' => [
            "     ", "     ", "#   #", "#   #", "# # #", "# # #", " # # ",
        ],
        'x' => [
            "     ", "     ", "#   #", " # # ", "  #  ", " # # ", "#   #",
        ],
        'y' => [
            "     ", "     ", "#   #", "#   #", " ####", "    #", " ### ",
        ],
        'z' => [
            "     ", "     ", "#####", "   # ", "  #  ", " #   ", "#####",
        ],
        _ => [
            " ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  ",
        ],
    }
}

fn toolbar_glyph_pattern(ch: char) -> [&'static str; 7] {
    if ch.is_ascii_lowercase() {
        return toolbar_lowercase_glyph_pattern(ch);
    }
    match ch.to_ascii_uppercase() {
        'A' => [
            " ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'B' => [
            "#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### ",
        ],
        'C' => [
            " ####", "#    ", "#    ", "#    ", "#    ", "#    ", " ####",
        ],
        'D' => [
            "#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### ",
        ],
        'E' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####",
        ],
        'F' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    ",
        ],
        'G' => [
            " ####", "#    ", "#    ", "#  ##", "#   #", "#   #", " ####",
        ],
        'H' => [
            "#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'I' => [
            "#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "#####",
        ],
        'J' => [
            "#####", "    #", "    #", "    #", "#   #", "#   #", " ### ",
        ],
        'K' => [
            "#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #",
        ],
        'L' => [
            "#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####",
        ],
        'M' => [
            "#   #", "## ##", "# # #", "# # #", "#   #", "#   #", "#   #",
        ],
        'N' => [
            "#   #", "##  #", "# # #", "#  ##", "#   #", "#   #", "#   #",
        ],
        'O' => [
            " ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'P' => [
            "#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    ",
        ],
        'Q' => [
            " ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #",
        ],
        'R' => [
            "#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #",
        ],
        'S' => [
            " ####", "#    ", "#    ", " ### ", "    #", "    #", "#### ",
        ],
        'T' => [
            "#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'U' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'V' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  ",
        ],
        'W' => [
            "#   #", "#   #", "#   #", "# # #", "# # #", "## ##", "#   #",
        ],
        'X' => [
            "#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #",
        ],
        'Y' => [
            "#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'Z' => [
            "#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####",
        ],
        '0' => [
            " ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### ",
        ],
        '1' => [
            "  #  ", " ##  ", "# #  ", "  #  ", "  #  ", "  #  ", "#####",
        ],
        '2' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####",
        ],
        '3' => [
            "#### ", "    #", "    #", " ### ", "    #", "    #", "#### ",
        ],
        '4' => [
            "#   #", "#   #", "#   #", "#####", "    #", "    #", "    #",
        ],
        '5' => [
            "#####", "#    ", "#    ", "#### ", "    #", "    #", "#### ",
        ],
        '6' => [
            " ### ", "#    ", "#    ", "#### ", "#   #", "#   #", " ### ",
        ],
        '7' => [
            "#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   ",
        ],
        '8' => [
            " ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### ",
        ],
        '9' => [
            " ### ", "#   #", "#   #", " ####", "    #", "    #", " ### ",
        ],
        ' ' => [
            "     ", "     ", "     ", "     ", "     ", "     ", "     ",
        ],
        '.' => [
            "     ", "     ", "     ", "     ", "     ", " ##  ", " ##  ",
        ],
        ':' => [
            "     ", " ##  ", " ##  ", "     ", " ##  ", " ##  ", "     ",
        ],
        '/' => [
            "    #", "    #", "   # ", "  #  ", " #   ", "#    ", "#    ",
        ],
        '\\' => [
            "#    ", "#    ", " #   ", "  #  ", "   # ", "    #", "    #",
        ],
        '-' => [
            "     ", "     ", "     ", "#### ", "     ", "     ", "     ",
        ],
        '_' => [
            "     ", "     ", "     ", "     ", "     ", "     ", "#####",
        ],
        '?' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  ",
        ],
        '&' => [
            " ##  ", "#  # ", "# #  ", " ##  ", "# # #", "#  # ", " ## #",
        ],
        '=' => [
            "     ", "#####", "     ", "#####", "     ", "     ", "     ",
        ],
        '%' => [
            "##  #", "## # ", "  #  ", " #   ", "#  ##", "#  ##", "     ",
        ],
        '#' => [
            " # # ", "#####", " # # ", " # # ", "#####", " # # ", "     ",
        ],
        '+' => [
            "     ", "  #  ", "  #  ", "#####", "  #  ", "  #  ", "     ",
        ],
        '@' => [
            " ### ", "#   #", "# ###", "# # #", "# ###", "#    ", " ### ",
        ],
        '~' => [
            "     ", "     ", " ## #", "# ## ", "     ", "     ", "     ",
        ],
        '\'' => [
            " ##  ", " ##  ", " #   ", "     ", "     ", "     ", "     ",
        ],
        '"' => [
            "# #  ", "# #  ", "# #  ", "     ", "     ", "     ", "     ",
        ],
        ',' => [
            "     ", "     ", "     ", "     ", " ##  ", " ##  ", " #   ",
        ],
        ';' => [
            "     ", " ##  ", " ##  ", "     ", " ##  ", " ##  ", " #   ",
        ],
        '(' => [
            "   # ", "  #  ", " #   ", " #   ", " #   ", "  #  ", "   # ",
        ],
        ')' => [
            " #   ", "  #  ", "   # ", "   # ", "   # ", "  #  ", " #   ",
        ],
        '[' => [
            " ### ", " #   ", " #   ", " #   ", " #   ", " #   ", " ### ",
        ],
        ']' => [
            " ### ", "   # ", "   # ", "   # ", "   # ", "   # ", " ### ",
        ],
        '!' => [
            "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "     ", "  #  ",
        ],
        _ => [
            " ### ", "#   #", "    #", "   # ", "  #  ", "     ", "  #  ",
        ],
    }
}

fn toolbar_rect_json(rect: ToolbarRect) -> Value {
    json!({
        "x": rect.x,
        "y": rect.y,
        "width": rect.width,
        "height": rect.height,
    })
}

#[allow(dead_code)]
fn draw_soft_bordered_rect(
    gl: &glow::Context,
    physical_size: PhysicalSize<u32>,
    scale: f32,
    rect: ToolbarRect,
    border: ToolbarColor,
    fill: ToolbarColor,
) {
    let radius = toolbar_control_radius(rect);
    fill_rounded_rect(gl, physical_size, scale, rect, radius, border);
    let inner = rect.inset(1.0);
    fill_rounded_rect(
        gl,
        physical_size,
        scale,
        inner,
        (radius - 1.0).max(0.0),
        fill,
    );
}

fn toolbar_control_radius(rect: ToolbarRect) -> f32 {
    (rect.width.min(rect.height) / 2.0).min(14.0)
}

#[allow(dead_code)]
fn fill_rounded_rect(
    gl: &glow::Context,
    physical_size: PhysicalSize<u32>,
    scale: f32,
    rect: ToolbarRect,
    radius: f32,
    color: ToolbarColor,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0);
    if radius <= 0.0 {
        fill_logical_rect(gl, physical_size, scale, rect, color);
        return;
    }

    let row_count = rect.height.ceil().max(1.0) as u32;
    for row in 0..row_count {
        let y = row as f32;
        let row_height = (rect.height - y).min(1.0).max(0.0);
        if row_height <= 0.0 {
            continue;
        }
        let center_y = y + row_height / 2.0;
        let inset = rounded_rect_row_inset(center_y, rect.height, radius);
        fill_logical_rect(
            gl,
            physical_size,
            scale,
            ToolbarRect {
                x: rect.x + inset,
                y: rect.y + y,
                width: (rect.width - inset * 2.0).max(0.0),
                height: row_height,
            },
            color,
        );
    }
}

fn rounded_rect_row_inset(center_y: f32, height: f32, radius: f32) -> f32 {
    let radius = radius.min(height / 2.0).max(0.0);
    if radius <= 0.0 || height <= 0.0 {
        return 0.0;
    }

    let circle_center_y = if center_y < radius {
        radius
    } else if center_y > height - radius {
        height - radius
    } else {
        return 0.0;
    };
    let dy = (center_y - circle_center_y).abs();
    radius - (radius * radius - dy * dy).max(0.0).sqrt()
}

#[allow(dead_code)]
fn fill_logical_rect(
    gl: &glow::Context,
    physical_size: PhysicalSize<u32>,
    scale: f32,
    rect: ToolbarRect,
    color: ToolbarColor,
) {
    if rect.width <= 0.0
        || rect.height <= 0.0
        || physical_size.width == 0
        || physical_size.height == 0
    {
        return;
    }

    let scale = if scale > 0.0 { scale } else { 1.0 };
    let mut x = (rect.x * scale).round() as i32;
    let mut y_from_top = (rect.y * scale).round() as i32;
    let mut width = (rect.width * scale).round().max(1.0) as i32;
    let mut height = (rect.height * scale).round().max(1.0) as i32;
    let physical_width = physical_size.width as i32;
    let physical_height = physical_size.height as i32;

    if x < 0 {
        width += x;
        x = 0;
    }
    if y_from_top < 0 {
        height += y_from_top;
        y_from_top = 0;
    }
    if x + width > physical_width {
        width = physical_width - x;
    }
    if y_from_top + height > physical_height {
        height = physical_height - y_from_top;
    }
    if width <= 0 || height <= 0 {
        return;
    }

    let y = physical_height - y_from_top - height;
    unsafe {
        gl.scissor(x, y, width, height);
        gl.clear_color(color.0, color.1, color.2, color.3);
        gl.clear(glow::COLOR_BUFFER_BIT);
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

fn address_select_all_shortcut(event: &KeyEvent, modifiers: ModifiersState) -> bool {
    if !(modifiers.super_key() || modifiers.control_key()) || modifiers.alt_key() {
        return false;
    }
    matches!(
        &event.logical_key,
        WinitKey::Character(text) if text.eq_ignore_ascii_case("a")
    )
}

fn address_focus_shortcut(event: &KeyEvent, modifiers: ModifiersState) -> bool {
    if !modifiers.super_key() || modifiers.alt_key() {
        return false;
    }
    matches!(
        &event.logical_key,
        WinitKey::Character(text) if text.eq_ignore_ascii_case("l")
    )
}

fn clamp_to_char_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn previous_char_boundary(text: &str, index: usize) -> usize {
    let index = clamp_to_char_boundary(text, index);
    if index == 0 {
        return 0;
    }
    text[..index]
        .char_indices()
        .last()
        .map(|(offset, _)| offset)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    let index = clamp_to_char_boundary(text, index);
    if index >= text.len() {
        return text.len();
    }
    text[index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| index + offset)
        .unwrap_or(text.len())
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
        "note": "MCP v0 should call saccade.tabs.grant_current with this artifact. The control endpoint supports same-WebView shell navigation, truth/actions/fill/inspect/act/formmax.",
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
    fn toolbar_text_fit_keeps_short_location_visible() {
        let text = fit_toolbar_text("https://example.com/", 240.0, SHELL_TOOLBAR_TEXT_PX);

        assert_eq!(text, "https://example.com/");
    }

    #[test]
    fn toolbar_text_fit_ellipsizes_long_location() {
        let text = fit_toolbar_text(
            "https://example.com/really/long/path/that/will/not/fit",
            96.0,
            SHELL_TOOLBAR_TEXT_PX,
        );

        assert!(text.ends_with("..."));
        assert!(toolbar_text_width(&text, SHELL_TOOLBAR_TEXT_PX) <= 96.5);
    }

    #[test]
    fn toolbar_glyphs_cover_common_location_characters() {
        for ch in "https://127.0.0.1:4173/path?q=a-b_c".chars() {
            assert!(
                toolbar_glyph_pattern(ch)
                    .iter()
                    .any(|row| row.contains('#'))
                    || ch == ' ',
                "missing visible toolbar glyph for {ch:?}"
            );
        }
    }

    #[test]
    fn toolbar_glyphs_keep_lowercase_distinct() {
        assert_ne!(toolbar_glyph_pattern('h'), toolbar_glyph_pattern('H'));
        assert_eq!(
            normalize_toolbar_text("  https://Example.com/\n"),
            "https://Example.com/"
        );
    }

    #[test]
    fn rounded_toolbar_inset_is_zero_in_middle_and_positive_at_edges() {
        let height = 32.0;
        let radius = 14.0;

        assert_eq!(rounded_rect_row_inset(16.0, height, radius), 0.0);
        assert!(rounded_rect_row_inset(0.5, height, radius) > 0.0);
        assert!(rounded_rect_row_inset(31.5, height, radius) > 0.0);
        assert!(
            (rounded_rect_row_inset(0.5, height, radius)
                - rounded_rect_row_inset(31.5, height, radius))
            .abs()
                < 0.001
        );
    }

    #[test]
    fn toolbar_font_loader_returns_drawable_ascii_when_available() {
        let Some(font) = toolbar_font() else {
            assert!(!cfg!(target_os = "macos"));
            return;
        };

        assert!(toolbar_font_has_drawable_ascii(font));
        assert!(
            toolbar_canvas_text_width("https://example.com/", SHELL_TOOLBAR_TEXT_SIZE).unwrap()
                > 80.0
        );
    }

    #[test]
    fn address_entry_starts_selected_and_typing_replaces_selection() {
        let mut entry = AddressEntryState::new_selected("https://example.com/".to_string());

        assert_eq!(entry.selection_range(), (0, "https://example.com/".len()));
        assert!(entry.has_selection());

        entry.replace_selection("ign.com");

        assert_eq!(entry.text, "ign.com");
        assert_eq!(entry.caret(), "ign.com".len());
        assert!(!entry.has_selection());
    }

    #[test]
    fn address_entry_backspace_delete_and_arrows_preserve_utf8_boundaries() {
        let mut entry = AddressEntryState::new_selected("abé".to_string());
        entry.move_right(false);
        entry.backspace();

        assert_eq!(entry.text, "ab");
        assert_eq!(entry.caret(), 2);

        entry.move_left(false);
        entry.delete_forward();
        assert_eq!(entry.text, "a");
        assert_eq!(entry.caret(), 1);
    }

    #[test]
    fn address_entry_collapsed_caret_inserts_without_replacing() {
        let mut entry = AddressEntryState::new_selected("example.com".to_string());
        entry.collapse_to("example".len());
        entry.replace_selection("-dev");

        assert_eq!(entry.text, "example-dev.com");
        assert_eq!(entry.caret(), "example-dev".len());
        assert!(!entry.has_selection());
    }

    #[test]
    fn visible_selection_clips_to_display_text() {
        assert_eq!(
            visible_selection_range("https://example...", 0, 200),
            Some((0, 18))
        );
        assert_eq!(visible_selection_range("abc", 3, 3), None);
    }

    #[test]
    fn address_text_index_for_x_tracks_visible_caret_position() {
        let text = "abc";
        let origin = 100.0;
        let font_size = SHELL_TOOLBAR_TEXT_SIZE;
        let first_width = toolbar_canvas_text_width_lossy("a", font_size);

        assert_eq!(
            address_text_index_for_x(text, origin - 8.0, origin, 300.0, font_size),
            0
        );
        assert_eq!(
            address_text_index_for_x(text, origin + first_width * 0.25, origin, 300.0, font_size),
            0
        );
        assert_eq!(
            address_text_index_for_x(text, origin + first_width * 0.75, origin, 300.0, font_size),
            1
        );
        assert_eq!(
            address_text_index_for_x(text, origin + 1000.0, origin, 300.0, font_size),
            text.len()
        );
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
