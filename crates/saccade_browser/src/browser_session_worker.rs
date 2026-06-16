use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use euclid::{Point2D, Scale};
use serde::Deserialize;
use serde_json::{Value, json};
use servo::{
    CSSPixel, DeviceIntRect, DeviceIntSize, EmbedderControl, InputEvent, JSValue, Key as ServoKey,
    KeyState, KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent,
    MouseMoveEvent, NamedKey as ServoNamedKey, RenderingContext, SelectElement,
    SelectElementOptionOrOptgroup, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate,
    WebViewPoint, WheelDelta, WheelEvent, WheelMode, WindowRenderingContext,
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

use crate::{RenderingProfile, RenderingProfileSettings};

const DEFAULT_WINDOW_WIDTH: u32 = 1600;
const DEFAULT_WINDOW_HEIGHT: u32 = 1000;
const LOAD_SETTLE_DELAY: Duration = Duration::from_millis(300);
const ACT_VERIFY_DELAY: Duration = Duration::from_millis(160);

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

#[derive(Clone)]
pub struct BrowserSessionWorkerConfig {
    pub url: Url,
    pub rendering_profile: Option<RenderingProfile>,
    pub width: u32,
    pub height: u32,
    pub profile_dir: Option<PathBuf>,
}

impl BrowserSessionWorkerConfig {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            rendering_profile: None,
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            profile_dir: None,
        }
    }
}

pub fn run_browser_session_worker(
    url: Url,
    rendering_profile: Option<RenderingProfile>,
) -> Result<()> {
    let mut config = BrowserSessionWorkerConfig::new(url);
    config.rendering_profile = rendering_profile;
    run_browser_session_worker_with_config(config)
}

pub fn run_browser_session_worker_with_config(config: BrowserSessionWorkerConfig) -> Result<()> {
    let rendering_settings = RenderingProfile::resolve_with_default(
        config.rendering_profile,
        RenderingProfile::ServoModern,
    )?;
    if rendering_settings.profile == RenderingProfile::ChromeReference {
        return run_unsupported_worker(config.url, rendering_settings);
    }
    if let Some(profile_dir) = config.profile_dir.as_ref() {
        fs::create_dir_all(profile_dir)
            .with_context(|| format!("failed to create profile dir {}", profile_dir.display()))?;
    }

    let artifacts = WorkerArtifacts::new()?;
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let (tx, rx) = mpsc::channel();
    let reader_proxy = event_loop.create_proxy();
    thread::spawn(move || read_commands(tx, reader_proxy));

    let mut app = WorkerApp::new(
        &event_loop,
        config.url,
        rx,
        artifacts,
        rendering_settings,
        config.width,
        config.height,
        config.profile_dir,
    );
    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;
    Ok(())
}

struct WorkerArtifacts {
    run_id: String,
    output_dir: PathBuf,
    report_path: PathBuf,
    replay_path: PathBuf,
}

impl WorkerArtifacts {
    fn new() -> Result<Self> {
        let run_id = format!("worker_{}_{}", unix_ms()?, std::process::id());
        let output_dir = PathBuf::from("runs")
            .join("browser_session_worker")
            .join(&run_id);
        fs::create_dir_all(&output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;
        Ok(Self {
            run_id,
            report_path: output_dir.join("report.json"),
            replay_path: output_dir.join("replay.jsonl"),
            output_dir,
        })
    }
}

#[derive(Debug)]
enum WorkerInput {
    Request(std::result::Result<WorkerRequest, String>),
    Eof,
}

#[derive(Debug, Deserialize)]
struct WorkerRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

enum ActiveRequest {
    Probe {
        id: Value,
        method: ProbeMethod,
    },
    ActWait {
        id: Value,
        action_id: String,
        before_probe: Value,
        before_revision: u64,
        dispatched_at: Instant,
    },
    ActProbe {
        id: Value,
        action_id: String,
        before_probe: Value,
        before_revision: u64,
    },
    FillAgentFields {
        id: Value,
        requested: usize,
    },
    InspectFields {
        id: Value,
        requested: usize,
    },
    InspectEditors {
        id: Value,
    },
    WebglRuntimeProbe {
        id: Value,
    },
    WebglPageProbe {
        id: Value,
    },
    TypeFocusedPreflight {
        id: Value,
        text: String,
    },
    TypeFocusedWait {
        id: Value,
        chars_requested: usize,
        before_probe: Value,
        dispatched_at: Instant,
    },
    TypeFocusedInsert {
        id: Value,
        chars_requested: usize,
        before_probe: Value,
    },
    TypeFocusedVerify {
        id: Value,
        chars_requested: usize,
        before_probe: Value,
    },
    FormmaxLiveFill {
        id: Value,
    },
}

#[derive(Clone, Copy)]
enum ProbeMethod {
    Truth,
    Actions,
    Audit,
}

struct WorkerState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webview: RefCell<Option<WebView>>,
    target_url: Url,
    run_id: String,
    output_dir: PathBuf,
    report_path: PathBuf,
    replay_path: PathBuf,
    command_rx: Receiver<WorkerInput>,
    queue: RefCell<VecDeque<WorkerRequest>>,
    eof_requested: Cell<bool>,
    loaded_once: Cell<bool>,
    loaded_at: Cell<Option<Instant>>,
    page_revision: Cell<u64>,
    pending_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    latest_probe: RefCell<Option<Value>>,
    current: RefCell<Option<ActiveRequest>>,
    cursor_x: Cell<f32>,
    cursor_y: Cell<f32>,
    cursor_move_count: Cell<u64>,
    last_cursor_move_at: Cell<Option<Instant>>,
    modifiers: Cell<ModifiersState>,
    active_select: RefCell<Option<ActiveSelect>>,
    screenshots: RefCell<Vec<String>>,
    screenshot_skipped_sensitive: Cell<u64>,
    rendering_settings: RenderingProfileSettings,
    pointer_trace: bool,
    stdout: RefCell<io::Stdout>,
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

impl WorkerState {
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

    fn trace_cursor_moved(&self, position: PhysicalPosition<f64>) {
        if !self.pointer_trace {
            return;
        }
        let scale = self.window.scale_factor();
        let logical = position.to_logical::<f64>(scale);
        let inner = self.window.inner_size();
        eprintln!(
            "SACCADE_POINTER_TRACE runtime=browser_session_worker event=cursor_moved raw_physical=({:.1},{:.1}) logical_if_css=({:.1},{:.1}) stored_page=({:.1},{:.1}) hidpi={:.3} inner_device={}x{} move_count={}",
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
            "SACCADE_POINTER_TRACE runtime=browser_session_worker event={} stored_page=({:.1},{:.1}) cursor_age_ms={:?} move_count={} detail={}",
            event,
            self.cursor_x.get(),
            self.cursor_y.get(),
            age_ms,
            self.cursor_move_count.get(),
            detail,
        );
    }

    fn handle_browser_shortcut(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed || !self.modifiers.get().super_key() {
            return false;
        }
        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return false;
        };
        match character_key(event).as_deref() {
            Some("r") | Some("R") => {
                webview.reload();
                true
            }
            Some("[") => {
                if webview.can_go_back() {
                    webview.go_back(1);
                }
                true
            }
            Some("]") => {
                if webview.can_go_forward() {
                    webview.go_forward(1);
                }
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
        self.window.set_title(&format!(
            "Saccade worker select | {}",
            active.choices[next].label
        ));
    }

    fn submit_active_select(&self) {
        let Some(mut active) = self.active_select.borrow_mut().take() else {
            return;
        };
        if let Some(choice) = active.choices.get(active.cursor) {
            active.control.select(vec![choice.id]);
        }
        active.control.submit();
    }

    fn dismiss_active_select(&self) {
        self.active_select.borrow_mut().take();
    }
}

impl WebViewDelegate for WorkerState {
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
                self.window
                    .set_title(&format!("Saccade worker select | {}", choice.label));
            }
            *self.active_select.borrow_mut() = Some(ActiveSelect {
                control: select,
                choices,
                cursor,
            });
        }
    }
}

enum WorkerApp {
    Initial {
        waker: Waker,
        target_url: Url,
        command_rx: Option<Receiver<WorkerInput>>,
        artifacts: Option<WorkerArtifacts>,
        rendering_settings: RenderingProfileSettings,
        width: u32,
        height: u32,
        profile_dir: Option<PathBuf>,
    },
    Running {
        state: Rc<WorkerState>,
    },
    Finished,
}

impl WorkerApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        target_url: Url,
        command_rx: Receiver<WorkerInput>,
        artifacts: WorkerArtifacts,
        rendering_settings: RenderingProfileSettings,
        width: u32,
        height: u32,
        profile_dir: Option<PathBuf>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            target_url,
            command_rx: Some(command_rx),
            artifacts: Some(artifacts),
            rendering_settings,
            width,
            height,
            profile_dir,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        drain_commands(&state);

        let Some(webview) = state.webview.borrow().clone() else {
            return;
        };
        sync_worker_render_surface_to_window(&state, &webview);
        if webview.load_status() == LoadStatus::Complete && !state.loaded_once.get() {
            state.loaded_once.set(true);
            state.loaded_at.set(Some(Instant::now()));
        }
        if !state.loaded_once.get() {
            state.window.request_redraw();
            return;
        }
        if state
            .loaded_at
            .get()
            .is_some_and(|loaded_at| loaded_at.elapsed() < LOAD_SETTLE_DELAY)
        {
            state.window.request_redraw();
            return;
        }

        process_current(&state, &webview, event_loop);
        if state.current.borrow().is_none() {
            start_next_request(&state, &webview, event_loop);
        }
        if state.eof_requested.get()
            && state.queue.borrow().is_empty()
            && state.current.borrow().is_none()
        {
            shutdown_worker(&state, event_loop);
            *self = Self::Finished;
            return;
        }
        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for WorkerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            target_url,
            command_rx,
            artifacts,
            rendering_settings,
            width,
            height,
            profile_dir,
        } = self
        else {
            return;
        };
        let initial_size = LogicalSize::new((*width).max(1), (*height).max(1));

        let display_handle = match event_loop.display_handle() {
            Ok(handle) => handle,
            Err(error) => {
                emit_renderer_crash(
                    rendering_settings,
                    target_url,
                    "display_handle",
                    &error.to_string(),
                );
                eprintln!("browser session worker display handle error: {error}");
                event_loop.exit();
                return;
            }
        };
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Saccade Browser Session Worker")
                .with_inner_size(initial_size),
        ) {
            Ok(window) => window,
            Err(error) => {
                emit_renderer_crash(
                    rendering_settings,
                    target_url,
                    "window_create",
                    &error.to_string(),
                );
                eprintln!("browser session worker window error: {error}");
                event_loop.exit();
                return;
            }
        };
        let window_handle = match window.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                emit_renderer_crash(
                    rendering_settings,
                    target_url,
                    "window_handle",
                    &error.to_string(),
                );
                eprintln!("browser session worker window handle error: {error}");
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
                    emit_renderer_crash(
                        rendering_settings,
                        target_url,
                        "rendering_context",
                        &format!("{error:?}"),
                    );
                    eprintln!("browser session worker rendering context error: {error:?}");
                    event_loop.exit();
                    return;
                }
            };
        if let Err(error) = rendering_context.make_current() {
            emit_renderer_crash(
                rendering_settings,
                target_url,
                "gl_make_current",
                &format!("{error:?}"),
            );
            eprintln!("browser session worker GL context error: {error:?}");
            event_loop.exit();
            return;
        }

        let mut servo_builder = ServoBuilder::default()
            .preferences(rendering_settings.servo_preferences())
            .event_loop_waker(Box::new(waker.clone()));
        if let Some(profile_dir) = profile_dir.clone() {
            let mut opts = servo::Opts::default();
            opts.config_dir = Some(profile_dir);
            opts.temporary_storage = false;
            servo_builder = servo_builder.opts(opts);
        }
        let servo = servo_builder.build();
        servo.setup_logging();

        let state = Rc::new(WorkerState {
            window,
            servo,
            rendering_context,
            webview: RefCell::new(None),
            target_url: target_url.clone(),
            run_id: artifacts
                .as_ref()
                .expect("browser session artifacts should exist")
                .run_id
                .clone(),
            output_dir: artifacts
                .as_ref()
                .expect("browser session artifacts should exist")
                .output_dir
                .clone(),
            report_path: artifacts
                .as_ref()
                .expect("browser session artifacts should exist")
                .report_path
                .clone(),
            replay_path: artifacts
                .take()
                .expect("browser session artifacts should exist")
                .replay_path,
            command_rx: command_rx
                .take()
                .expect("browser session command_rx should exist"),
            queue: RefCell::new(VecDeque::new()),
            eof_requested: Cell::new(false),
            loaded_once: Cell::new(false),
            loaded_at: Cell::new(None),
            page_revision: Cell::new(1),
            pending_probe: Rc::new(RefCell::new(None)),
            latest_probe: RefCell::new(None),
            current: RefCell::new(None),
            cursor_x: Cell::new(0.0),
            cursor_y: Cell::new(0.0),
            cursor_move_count: Cell::new(0),
            last_cursor_move_at: Cell::new(None),
            modifiers: Cell::new(ModifiersState::empty()),
            active_select: RefCell::new(None),
            screenshots: RefCell::new(Vec::new()),
            screenshot_skipped_sensitive: Cell::new(0),
            rendering_settings: rendering_settings.clone(),
            pointer_trace: env_flag("SACCADE_TRACE_POINTER"),
            stdout: RefCell::new(io::stdout()),
        });
        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.target_url.clone())
            .hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32))
            .delegate(state.clone())
            .build();
        *state.webview.borrow_mut() = Some(webview);
        log_replay(
            &state,
            json!({
                "kind": "browser_worker_started",
                "run_id": state.run_id.as_str(),
                "url": state.target_url.as_str(),
                "page_revision": state.page_revision.get(),
                "rendering_profile": state.rendering_settings.profile.name(),
                "engine": state.rendering_settings.profile.engine(),
                "servo_grid_enabled": state.rendering_settings.layout_grid_enabled,
                "legacy_grid_override": state.rendering_settings.legacy_grid_override,
                "experimental_prefs": state.rendering_settings.experimental_prefs(),
            }),
        );
        write_report(
            &state,
            &json!({
                "status": "starting",
                "summary": "browser session worker started",
            }),
        );

        *self = Self::Running { state };
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: WakerEvent) {
        if let Self::Running { state } = self {
            state.servo.spin_event_loop();
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
            match event {
                WindowEvent::CloseRequested => {
                    shutdown_worker(state, event_loop);
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
                        resize_worker_render_surface(&state, webview, new_size, "window_event");
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
                    if state.handle_active_select_key(&event)
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
        self.after_spin(event_loop);
    }
}

fn read_commands(tx: mpsc::Sender<WorkerInput>, proxy: EventLoopProxy<WakerEvent>) {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let input = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => WorkerInput::Request(
                serde_json::from_str::<WorkerRequest>(&line).map_err(|error| error.to_string()),
            ),
            Err(error) => WorkerInput::Request(Err(error.to_string())),
        };
        if tx.send(input).is_err() {
            return;
        }
        let _ = proxy.send_event(WakerEvent);
    }
    let _ = tx.send(WorkerInput::Eof);
    let _ = proxy.send_event(WakerEvent);
}

fn run_unsupported_worker(url: Url, rendering_settings: RenderingProfileSettings) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = serde_json::from_str::<WorkerRequest>(&line);
        let (id, method) = match request {
            Ok(request) => (request.id.unwrap_or(Value::Null), request.method),
            Err(error) => {
                let response = json!({
                    "id": Value::Null,
                    "ok": false,
                    "error": format!("invalid worker request: {error}"),
                });
                writeln!(stdout, "{response}")?;
                stdout.flush()?;
                continue;
            }
        };
        if method == "close" {
            let response = json!({
                "id": id,
                "ok": true,
                "result": {
                    "status": "ok",
                    "summary": "unsupported rendering-profile worker closing",
                    "rendering_profile": rendering_settings.profile.name(),
                },
            });
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
            break;
        }
        let response = json!({
            "id": id,
            "ok": true,
            "result": renderer_crash_result(
                &rendering_settings,
                &url,
                "unsupported_profile",
                "chrome-reference is a configuration stub; no Chrome worker is implemented in R1",
            ),
        });
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

fn emit_renderer_crash(
    rendering_settings: &RenderingProfileSettings,
    url: &Url,
    stage: &str,
    message: &str,
) {
    let response = json!({
        "id": Value::Null,
        "ok": true,
        "result": renderer_crash_result(rendering_settings, url, stage, message),
    });
    println!("{response}");
}

fn renderer_crash_result(
    rendering_settings: &RenderingProfileSettings,
    url: &Url,
    stage: &str,
    message: &str,
) -> Value {
    json!({
        "status": "renderer_crash",
        "runtime": "browser_session_worker_v0",
        "rendering_profile": rendering_settings.profile.name(),
        "fixture": url.as_str(),
        "engine": rendering_settings.profile.engine(),
        "pref": if rendering_settings.layout_grid_enabled {
            Value::String("layout.grid.enabled".to_string())
        } else {
            Value::Null
        },
        "stage": stage,
        "message": message,
        "fallback_recommended": rendering_settings.fallback_recommended(),
    })
}

fn drain_commands(state: &Rc<WorkerState>) {
    while let Ok(input) = state.command_rx.try_recv() {
        match input {
            WorkerInput::Request(Ok(request)) => state.queue.borrow_mut().push_back(request),
            WorkerInput::Request(Err(error)) => {
                respond_error(
                    state,
                    Value::Null,
                    format!("invalid worker request: {error}"),
                );
            }
            WorkerInput::Eof => {
                state.eof_requested.set(true);
            }
        }
    }
}

fn sync_worker_render_surface_to_window(state: &Rc<WorkerState>, webview: &WebView) {
    let window_size = state.window.inner_size();
    let context_size = state.rendering_context.size2d();
    if context_size.width != window_size.width || context_size.height != window_size.height {
        resize_worker_render_surface(state, webview, window_size, "window_poll");
    }
}

fn resize_worker_render_surface(
    state: &Rc<WorkerState>,
    webview: &WebView,
    new_size: PhysicalSize<u32>,
    source: &str,
) {
    let old_size = state.rendering_context.size2d();
    webview.set_hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32));
    webview.resize(new_size);
    state.servo.spin_event_loop();
    state.window.request_redraw();
    log_replay(
        state,
        json!({
            "kind": "browser_render_surface_resized",
            "run_id": state.run_id.as_str(),
            "source": source,
            "old": {
                "width": old_size.width,
                "height": old_size.height,
            },
            "new": {
                "width": new_size.width,
                "height": new_size.height,
            },
            "runtime_geometry": runtime_geometry(state, webview),
        }),
    );
}

fn runtime_geometry(state: &Rc<WorkerState>, webview: &WebView) -> Value {
    let window_size = state.window.inner_size();
    let context_size = state.rendering_context.size2d();
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

fn start_next_request(state: &Rc<WorkerState>, webview: &WebView, event_loop: &ActiveEventLoop) {
    let Some(request) = state.queue.borrow_mut().pop_front() else {
        return;
    };
    let id = request.id.unwrap_or(Value::Null);
    match request.method.as_str() {
        "ping" => respond_ok(
            state,
            id,
            json!({
                "status": "ok",
                "runtime": "browser_session_worker_v0",
                "rendering_profile": state.rendering_settings.profile.name(),
                "renderer_engine": state.rendering_settings.profile.engine(),
                "servo_grid_enabled": state.rendering_settings.layout_grid_enabled,
                "url": state.target_url.as_str(),
                "page_revision": state.page_revision.get(),
                "runtime_geometry": runtime_geometry(state, webview),
            }),
        ),
        "truth" => {
            request_probe(state, webview);
            *state.current.borrow_mut() = Some(ActiveRequest::Probe {
                id,
                method: ProbeMethod::Truth,
            });
        }
        "actions" => {
            request_probe(state, webview);
            *state.current.borrow_mut() = Some(ActiveRequest::Probe {
                id,
                method: ProbeMethod::Actions,
            });
        }
        "audit" => {
            request_probe(state, webview);
            *state.current.borrow_mut() = Some(ActiveRequest::Probe {
                id,
                method: ProbeMethod::Audit,
            });
        }
        "act" => start_act_request(state, webview, id, request.params),
        "fill_agent_fields" => start_fill_agent_fields_request(state, webview, id, request.params),
        "inspect_fields" => start_inspect_fields_request(state, webview, id, request.params),
        "inspect_editors" => start_inspect_editors_request(state, webview, id),
        "webgl_runtime_probe" => start_webgl_runtime_probe_request(state, webview, id),
        "webgl_page_probe" => start_webgl_page_probe_request(state, webview, id),
        "type_focused_text" => start_type_focused_text_request(state, webview, id, request.params),
        "formmax_live_fill" => start_formmax_live_fill_request(state, webview, id, request.params),
        "close" => {
            write_response(
                state,
                json!({
                    "id": id,
                    "ok": true,
                    "result": {
                        "status": "ok",
                        "summary": "browser session worker closing",
                    },
                }),
            );
            shutdown_worker(state, event_loop);
        }
        other => respond_error(state, id, format!("unknown worker method {other:?}")),
    }
}

fn shutdown_worker(state: &Rc<WorkerState>, event_loop: &ActiveEventLoop) {
    state.active_select.borrow_mut().take();
    state.webview.borrow_mut().take();
    event_loop.exit();
}

fn start_act_request(state: &Rc<WorkerState>, webview: &WebView, id: Value, params: Value) {
    let action_id = match params.get("action_id").and_then(Value::as_str) {
        Some(value) => value.to_string(),
        None => {
            respond_error(state, id, "act requires string action_id".into());
            return;
        }
    };
    let basis = match params.get("basis_page_revision").and_then(Value::as_u64) {
        Some(value) => value,
        None => {
            respond_error(state, id, "act requires integer basis_page_revision".into());
            return;
        }
    };
    let current_revision = state.page_revision.get();
    if basis != current_revision {
        respond_error(
            state,
            id,
            format!("stale action basis: requested {basis}, current {current_revision}"),
        );
        return;
    }
    let Some(before_probe) = state.latest_probe.borrow().clone() else {
        respond_error(state, id, "call truth/actions before act".into());
        return;
    };
    let Some(action) = before_probe
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|action| action_id_for(action) == action_id)
        .cloned()
    else {
        respond_error(state, id, format!("unknown action_id {action_id:?}"));
        return;
    };
    if !action_enabled(&action) {
        respond_error(state, id, format!("action {action_id:?} is not enabled"));
        return;
    }

    click_action(state, webview, &action);
    *state.current.borrow_mut() = Some(ActiveRequest::ActWait {
        id,
        action_id,
        before_probe,
        before_revision: current_revision,
        dispatched_at: Instant::now(),
    });
}

fn start_type_focused_text_request(
    state: &Rc<WorkerState>,
    webview: &WebView,
    id: Value,
    params: Value,
) {
    let Some(text) = params.get("text").and_then(Value::as_str) else {
        respond_error(state, id, "type_focused_text requires string text".into());
        return;
    };
    if text.is_empty() {
        respond_error(
            state,
            id,
            "type_focused_text requires non-empty text".into(),
        );
        return;
    }
    let max_chars = params
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(4000) as usize;
    if text.chars().count() > max_chars {
        respond_error(
            state,
            id,
            format!("type_focused_text text exceeds max_chars={max_chars}"),
        );
        return;
    }
    if let Some(policy) = params.get("policy") {
        if policy
            .get("active_element_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            respond_error(
                state,
                id,
                "type_focused_text requires active_element_only=true".into(),
            );
            return;
        }
        if policy
            .get("block_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            respond_error(
                state,
                id,
                "type_focused_text requires block_sensitive=true".into(),
            );
            return;
        }
    }

    request_type_focused_probe(state, webview);
    *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedPreflight {
        id,
        text: text.to_string(),
    });
}

fn request_type_focused_probe(state: &Rc<WorkerState>, webview: &WebView) {
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(TYPE_FOCUSED_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
}

fn request_type_focused_contenteditable_insert(
    state: &Rc<WorkerState>,
    webview: &WebView,
    text: &str,
) -> std::result::Result<(), String> {
    let text_json = serde_json::to_string(text)
        .map_err(|error| format!("failed to serialize focused text: {error}"))?;
    let script = type_focused_contenteditable_insert_script(&text_json);
    let script: &'static str = Box::leak(script.into_boxed_str());
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(script, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    state.window.request_redraw();
    Ok(())
}

fn process_current(state: &Rc<WorkerState>, webview: &WebView, _event_loop: &ActiveEventLoop) {
    let Some(current) = state.current.borrow_mut().take() else {
        return;
    };
    match current {
        ActiveRequest::Probe { id, method } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => {
                    *state.latest_probe.borrow_mut() = Some(value.clone());
                    respond_ok(state, id, probe_response(state, webview, &value, method));
                }
                Err(error) => respond_error(state, id, format!("failed to parse probe: {error}")),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::Probe { id, method });
            }
        },
        ActiveRequest::ActWait {
            id,
            action_id,
            before_probe,
            before_revision,
            dispatched_at,
        } => {
            if dispatched_at.elapsed() >= ACT_VERIFY_DELAY {
                request_probe(state, webview);
                *state.current.borrow_mut() = Some(ActiveRequest::ActProbe {
                    id,
                    action_id,
                    before_probe,
                    before_revision,
                });
            } else {
                *state.current.borrow_mut() = Some(ActiveRequest::ActWait {
                    id,
                    action_id,
                    before_probe,
                    before_revision,
                    dispatched_at,
                });
            }
        }
        ActiveRequest::ActProbe {
            id,
            action_id,
            before_probe,
            before_revision,
        } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(after_probe) => {
                    let after_revision = before_revision + 1;
                    state.page_revision.set(after_revision);
                    *state.latest_probe.borrow_mut() = Some(after_probe.clone());
                    respond_ok(
                        state,
                        id,
                        act_response(
                            state,
                            webview,
                            action_id,
                            before_revision,
                            after_revision,
                            &before_probe,
                            &after_probe,
                        ),
                    );
                }
                Err(error) => respond_error(state, id, format!("failed to parse probe: {error}")),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::ActProbe {
                    id,
                    action_id,
                    before_probe,
                    before_revision,
                });
            }
        },
        ActiveRequest::FillAgentFields { id, requested } => {
            match finish_probe(&state.pending_probe) {
                Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                    Ok(value) => {
                        let filled = value
                            .get("filled")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                            .unwrap_or(0);
                        if filled > 0 {
                            state.page_revision.set(state.page_revision.get() + 1);
                        }
                        respond_ok(
                            state,
                            id,
                            fill_agent_fields_response(state, requested, &value),
                        );
                    }
                    Err(error) => {
                        respond_error(state, id, format!("failed to parse fill result: {error}"))
                    }
                },
                Some(Err(error)) => respond_error(state, id, error),
                None => {
                    *state.current.borrow_mut() =
                        Some(ActiveRequest::FillAgentFields { id, requested });
                }
            }
        }
        ActiveRequest::InspectFields { id, requested } => {
            match finish_probe(&state.pending_probe) {
                Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                    Ok(value) => {
                        respond_ok(state, id, inspect_fields_response(state, requested, &value))
                    }
                    Err(error) => respond_error(
                        state,
                        id,
                        format!("failed to parse inspect result: {error}"),
                    ),
                },
                Some(Err(error)) => respond_error(state, id, error),
                None => {
                    *state.current.borrow_mut() =
                        Some(ActiveRequest::InspectFields { id, requested });
                }
            }
        }
        ActiveRequest::InspectEditors { id } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => respond_ok(state, id, inspect_editors_response(state, &value)),
                Err(error) => respond_error(
                    state,
                    id,
                    format!("failed to parse inspect editors result: {error}"),
                ),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::InspectEditors { id });
            }
        },
        ActiveRequest::WebglRuntimeProbe { id } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => respond_ok(state, id, webgl_runtime_probe_response(state, &value)),
                Err(error) => respond_error(
                    state,
                    id,
                    format!("failed to parse WebGL runtime result: {error}"),
                ),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::WebglRuntimeProbe { id });
            }
        },
        ActiveRequest::WebglPageProbe { id } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => respond_ok(state, id, webgl_page_probe_response(state, &value)),
                Err(error) => respond_error(
                    state,
                    id,
                    format!("failed to parse WebGL page probe result: {error}"),
                ),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::WebglPageProbe { id });
            }
        },
        ActiveRequest::TypeFocusedPreflight { id, text } => {
            match finish_probe(&state.pending_probe) {
                Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                    Ok(value) => {
                        if value.get("ok").and_then(Value::as_bool) != Some(true) {
                            let reason = value
                                .get("reason")
                                .and_then(Value::as_str)
                                .unwrap_or("focused field is not writable");
                            respond_error(
                                state,
                                id,
                                format!("type_focused_text blocked: {reason}"),
                            );
                            return;
                        }
                        let chars_requested = text.chars().count();
                        if value.get("contentEditable").and_then(Value::as_bool) == Some(true) {
                            match request_type_focused_contenteditable_insert(state, webview, &text)
                            {
                                Ok(()) => {
                                    *state.current.borrow_mut() =
                                        Some(ActiveRequest::TypeFocusedInsert {
                                            id,
                                            chars_requested,
                                            before_probe: value,
                                        });
                                }
                                Err(error) => respond_error(state, id, error),
                            }
                        } else {
                            type_text_into_focused_field(state, webview, &text);
                            *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedWait {
                                id,
                                chars_requested,
                                before_probe: value,
                                dispatched_at: Instant::now(),
                            });
                        }
                    }
                    Err(error) => respond_error(
                        state,
                        id,
                        format!("failed to parse focused type preflight: {error}"),
                    ),
                },
                Some(Err(error)) => respond_error(state, id, error),
                None => {
                    *state.current.borrow_mut() =
                        Some(ActiveRequest::TypeFocusedPreflight { id, text });
                }
            }
        }
        ActiveRequest::TypeFocusedWait {
            id,
            chars_requested,
            before_probe,
            dispatched_at,
        } => {
            if dispatched_at.elapsed() >= ACT_VERIFY_DELAY {
                request_type_focused_probe(state, webview);
                *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedVerify {
                    id,
                    chars_requested,
                    before_probe,
                });
            } else {
                *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedWait {
                    id,
                    chars_requested,
                    before_probe,
                    dispatched_at,
                });
            }
        }
        ActiveRequest::TypeFocusedInsert {
            id,
            chars_requested,
            before_probe,
        } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => {
                    if value.get("ok").and_then(Value::as_bool) != Some(true) {
                        let reason = value
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("focused contenteditable insertion failed");
                        respond_error(
                            state,
                            id,
                            format!("type_focused_text insert blocked: {reason}"),
                        );
                        return;
                    }
                    if type_focused_changed(&before_probe, &value) {
                        state.page_revision.set(state.page_revision.get() + 1);
                    }
                    respond_ok(
                        state,
                        id,
                        type_focused_text_response(state, chars_requested, &before_probe, &value),
                    );
                }
                Err(error) => respond_error(
                    state,
                    id,
                    format!("failed to parse focused type insertion: {error}"),
                ),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedInsert {
                    id,
                    chars_requested,
                    before_probe,
                });
            }
        },
        ActiveRequest::TypeFocusedVerify {
            id,
            chars_requested,
            before_probe,
        } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => {
                    if value.get("ok").and_then(Value::as_bool) != Some(true) {
                        let reason = value
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("focused field verification failed");
                        respond_error(
                            state,
                            id,
                            format!("type_focused_text verify blocked: {reason}"),
                        );
                        return;
                    }
                    if type_focused_changed(&before_probe, &value) {
                        state.page_revision.set(state.page_revision.get() + 1);
                    }
                    respond_ok(
                        state,
                        id,
                        type_focused_text_response(state, chars_requested, &before_probe, &value),
                    );
                }
                Err(error) => respond_error(
                    state,
                    id,
                    format!("failed to parse focused type verification: {error}"),
                ),
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::TypeFocusedVerify {
                    id,
                    chars_requested,
                    before_probe,
                });
            }
        },
        ActiveRequest::FormmaxLiveFill { id } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => {
                    let filled = value.get("filled").and_then(Value::as_u64).unwrap_or(0);
                    if filled > 0 {
                        state.page_revision.set(state.page_revision.get() + 1);
                    }
                    respond_ok(state, id, formmax_live_fill_response(state, &value));
                }
                Err(error) => {
                    respond_error(
                        state,
                        id,
                        format!("failed to parse FORMMAX live fill result: {error}"),
                    );
                }
            },
            Some(Err(error)) => respond_error(state, id, error),
            None => {
                *state.current.borrow_mut() = Some(ActiveRequest::FormmaxLiveFill { id });
            }
        },
    }
}

fn start_formmax_live_fill_request(
    state: &Rc<WorkerState>,
    webview: &WebView,
    id: Value,
    params: Value,
) {
    if let Some(policy) = params.get("policy") {
        if policy
            .get("block_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            respond_error(
                state,
                id,
                "formmax_live_fill requires block_sensitive=true".into(),
            );
            return;
        }
        if policy
            .get("local_fixture_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            respond_error(
                state,
                id,
                "formmax_live_fill requires local_fixture_only=true".into(),
            );
            return;
        }
    }

    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(FORMMAX_LIVE_FILL_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::FormmaxLiveFill { id });
}

fn start_fill_agent_fields_request(
    state: &Rc<WorkerState>,
    webview: &WebView,
    id: Value,
    params: Value,
) {
    let Some(fields) = params.get("fields").and_then(Value::as_object) else {
        respond_error(
            state,
            id,
            "fill_agent_fields requires object params.fields".into(),
        );
        return;
    };
    let requested = fields.len();
    if requested == 0 {
        respond_error(
            state,
            id,
            "fill_agent_fields requires at least one field".into(),
        );
        return;
    }
    let fields_json = match serde_json::to_string(fields) {
        Ok(value) => value,
        Err(error) => {
            respond_error(state, id, format!("failed to serialize fields: {error}"));
            return;
        }
    };
    let script = fill_agent_fields_script(&fields_json);
    let script: &'static str = Box::leak(script.into_boxed_str());
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(script, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::FillAgentFields { id, requested });
}

fn start_inspect_fields_request(
    state: &Rc<WorkerState>,
    webview: &WebView,
    id: Value,
    params: Value,
) {
    let Some(fields) = params.get("fields").and_then(Value::as_array) else {
        respond_error(
            state,
            id,
            "inspect_fields requires array params.fields".into(),
        );
        return;
    };
    let requested = fields.len();
    if requested == 0 {
        respond_error(
            state,
            id,
            "inspect_fields requires at least one field".into(),
        );
        return;
    }
    if fields.iter().any(|field| field.as_str().is_none()) {
        respond_error(state, id, "inspect_fields field ids must be strings".into());
        return;
    }
    let fields_json = match serde_json::to_string(fields) {
        Ok(value) => value,
        Err(error) => {
            respond_error(state, id, format!("failed to serialize field ids: {error}"));
            return;
        }
    };
    let script = inspect_fields_script(&fields_json);
    let script: &'static str = Box::leak(script.into_boxed_str());
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(script, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::InspectFields { id, requested });
}

fn start_inspect_editors_request(state: &Rc<WorkerState>, webview: &WebView, id: Value) {
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(INSPECT_EDITORS_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::InspectEditors { id });
}

fn start_webgl_runtime_probe_request(state: &Rc<WorkerState>, webview: &WebView, id: Value) {
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(WEBGL_RUNTIME_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::WebglRuntimeProbe { id });
}

fn start_webgl_page_probe_request(state: &Rc<WorkerState>, webview: &WebView, id: Value) {
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(WEBGL_PAGE_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
    *state.current.borrow_mut() = Some(ActiveRequest::WebglPageProbe { id });
}

fn request_probe(state: &Rc<WorkerState>, webview: &WebView) {
    *state.pending_probe.borrow_mut() = None;
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
}

fn finish_probe(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
) -> Option<std::result::Result<String, String>> {
    pending.borrow_mut().take()
}

fn probe_response(
    state: &Rc<WorkerState>,
    webview: &WebView,
    probe: &Value,
    method: ProbeMethod,
) -> Value {
    let actions = action_map(probe);
    let sensitive_count = sensitive_action_count(&actions);
    let kind = match method {
        ProbeMethod::Truth => "truth_collected",
        ProbeMethod::Actions => "actions_collected",
        ProbeMethod::Audit => "audit_completed",
    };
    let screenshot = maybe_save_screenshot(
        state,
        webview,
        &format!("{}_rev{}.png", kind, state.page_revision.get()),
        sensitive_count,
    );
    let findings = match method {
        ProbeMethod::Audit => audit_findings(probe, &actions),
        ProbeMethod::Truth | ProbeMethod::Actions => Vec::new(),
    };
    let body_text_length = probe
        .get("bodyTextLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let body_child_count = probe
        .get("bodyChildCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    log_replay(
        state,
        json!({
            "kind": kind,
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "actions_seen": actions.len(),
            "findings": findings.len(),
            "sensitive_fields": sensitive_count,
            "body_text_length": body_text_length,
            "screenshot": screenshot.clone(),
        }),
    );
    let engine = match method {
        ProbeMethod::Audit => "saccade-browser-session-audit-v0",
        ProbeMethod::Truth | ProbeMethod::Actions => "saccade-browser-session-worker-v0",
    };
    let summary = match method {
        ProbeMethod::Truth => "browser session truth collected from live Servo tab".to_string(),
        ProbeMethod::Actions => {
            "browser session action map collected from live Servo tab".to_string()
        }
        ProbeMethod::Audit => format!(
            "live browser session audit found {} findings across {} actions",
            findings.len(),
            actions.len()
        ),
    };
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": engine,
        "summary": summary,
        "rendering_profile": state.rendering_settings.profile.name(),
        "renderer_engine": state.rendering_settings.profile.engine(),
        "servo_grid_enabled": state.rendering_settings.layout_grid_enabled,
        "legacy_grid_override": state.rendering_settings.legacy_grid_override,
        "experimental_prefs": state.rendering_settings.experimental_prefs(),
        "runtime_geometry": runtime_geometry(state, webview),
        "url": probe.get("url").cloned().unwrap_or_else(|| json!(state.target_url.as_str())),
        "title": probe.get("title").cloned().unwrap_or(Value::Null),
        "page_revision": state.page_revision.get(),
        "dom_page_revision": probe.get("pageRevision").cloned().unwrap_or(Value::Null),
        "actions": actions,
        "findings": findings.clone(),
        "visual_health": {
            "blank_page": body_text_length == 0 && body_child_count == 0,
            "screenshot": screenshot,
        },
        "truth": {
            "body_text_length": body_text_length,
            "body_child_count": body_child_count,
            "viewport": probe.get("viewport").cloned().unwrap_or(Value::Null),
            "layout_probes": probe.get("layoutProbes").cloned().unwrap_or(Value::Null),
            "sensitive_fields": sensitive_count,
            "findings": findings,
        },
        "artifacts": artifact_paths(state),
    })
}

fn act_response(
    state: &Rc<WorkerState>,
    webview: &WebView,
    action_id: String,
    before_revision: u64,
    after_revision: u64,
    before_probe: &Value,
    after_probe: &Value,
) -> Value {
    let changed = probe_changed(before_probe, after_probe);
    let actions = action_map(after_probe);
    let sensitive_count = sensitive_action_count(&actions);
    let screenshot = maybe_save_screenshot(
        state,
        webview,
        &format!("action_rev{after_revision}.png"),
        sensitive_count,
    );
    log_replay(
        state,
        json!({
            "kind": "action_verified",
            "run_id": state.run_id.as_str(),
            "action_id": action_id.as_str(),
            "basis_page_revision": before_revision,
            "new_page_revision": after_revision,
            "changed": changed,
            "dom_page_revision_before": before_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
            "dom_page_revision_after": after_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
            "sensitive_fields": sensitive_count,
            "screenshot": screenshot,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "action dispatched through live Servo browser session",
        "rendering_profile": state.rendering_settings.profile.name(),
        "renderer_engine": state.rendering_settings.profile.engine(),
        "servo_grid_enabled": state.rendering_settings.layout_grid_enabled,
        "legacy_grid_override": state.rendering_settings.legacy_grid_override,
        "experimental_prefs": state.rendering_settings.experimental_prefs(),
        "url": after_probe.get("url").cloned().unwrap_or_else(|| json!(state.target_url.as_str())),
        "title": after_probe.get("title").cloned().unwrap_or(Value::Null),
        "page_revision": after_revision,
        "actions": actions,
        "verification": {
            "mode": "browser_session_worker_native_click_v0",
            "action_id": action_id,
            "action_sent": true,
            "changed": changed,
            "no_effect": !changed,
            "basis_page_revision": before_revision,
            "new_page_revision": after_revision,
            "body_text_length_changed": before_probe.get("bodyTextLength") != after_probe.get("bodyTextLength"),
            "body_child_count_changed": before_probe.get("bodyChildCount") != after_probe.get("bodyChildCount"),
            "dom_page_revision_before": before_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
            "dom_page_revision_after": after_probe.get("pageRevision").cloned().unwrap_or(Value::Null),
        },
        "artifacts": artifact_paths(state),
    })
}

fn fill_agent_fields_response(
    state: &Rc<WorkerState>,
    requested: usize,
    fill_result: &Value,
) -> Value {
    let filled = fill_result
        .get("filled")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let rejected = fill_result
        .get("rejected")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    log_replay(
        state,
        json!({
            "kind": "agent_fields_filled",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "requested": requested,
            "filled": filled.len(),
            "filled_field_ids": filled,
            "rejected": rejected,
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "agent-owned non-sensitive fields filled in live Servo browser session",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "requested": requested,
        "filled": fill_result.get("filled").cloned().unwrap_or_else(|| json!([])),
        "rejected": fill_result.get("rejected").cloned().unwrap_or_else(|| json!([])),
        "sensitive_fields_seen": fill_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
        "artifacts": artifact_paths(state),
    })
}

fn inspect_fields_response(
    state: &Rc<WorkerState>,
    requested: usize,
    inspect_result: &Value,
) -> Value {
    let fields = inspect_result
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let inspected_field_ids: Vec<Value> = fields
        .iter()
        .filter_map(|field| field.get("id").and_then(Value::as_str))
        .map(|id| json!(id))
        .collect();
    let values_returned = fields
        .iter()
        .filter(|field| {
            field
                .get("value_returned")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let redacted = fields
        .iter()
        .filter(|field| {
            field
                .get("value_redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    log_replay(
        state,
        json!({
            "kind": "fields_inspected",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "requested": requested,
            "inspected_field_ids": inspected_field_ids,
            "values_returned": values_returned,
            "values_redacted": redacted,
            "sensitive_fields_seen": inspect_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "explicitly requested field inspection completed with sensitive values masked",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "requested": requested,
        "fields": inspect_result.get("fields").cloned().unwrap_or_else(|| json!([])),
        "sensitive_fields_seen": inspect_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
        "artifacts": artifact_paths(state),
    })
}

fn inspect_editors_response(state: &Rc<WorkerState>, inspect_result: &Value) -> Value {
    let editors = inspect_result
        .get("editors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let zero_rect_count = editors
        .iter()
        .filter(|editor| {
            editor
                .pointer("/rect/width")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                <= 0.0
                || editor
                    .pointer("/rect/height")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    <= 0.0
        })
        .count();
    let sensitive_count = editors
        .iter()
        .filter(|editor| {
            editor
                .get("sensitivity")
                .and_then(Value::as_str)
                .is_some_and(|sensitivity| sensitivity != "none")
        })
        .count();
    let visible_writable_count = editors
        .iter()
        .filter(|editor| editor_is_visible_writable(editor))
        .count();
    let visible_authoring_count = editors
        .iter()
        .filter(|editor| editor_is_visible_authoring(editor))
        .count();
    let route = editor_route(
        editors.len(),
        zero_rect_count,
        visible_writable_count,
        visible_authoring_count,
    );
    log_replay(
        state,
        json!({
            "kind": "editors_inspected",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "editor_count": editors.len(),
            "zero_rect_count": zero_rect_count,
            "visible_writable_count": visible_writable_count,
            "visible_authoring_count": visible_authoring_count,
            "sensitive_count": sensitive_count,
            "route_decision": route.get("decision").cloned().unwrap_or(Value::Null),
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "editor candidates inspected without returning text values",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "source_url": inspect_result.get("url").cloned().unwrap_or(Value::Null),
        "source_title": inspect_result.get("title").cloned().unwrap_or(Value::Null),
        "active_tag": inspect_result.get("activeTag").cloned().unwrap_or(Value::Null),
        "active_id": inspect_result.get("activeId").cloned().unwrap_or(Value::Null),
        "editor_count": editors.len(),
        "zero_rect_count": zero_rect_count,
        "visible_writable_count": visible_writable_count,
        "visible_authoring_count": visible_authoring_count,
        "sensitive_count": sensitive_count,
        "route": route,
        "editors": editors,
        "artifacts": artifact_paths(state),
    })
}

fn webgl_runtime_probe_response(state: &Rc<WorkerState>, runtime_result: &Value) -> Value {
    log_replay(
        state,
        json!({
            "kind": "webgl_runtime_probed",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "runtime_found": runtime_result.get("runtime").is_some(),
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-webgl-runtime-v0",
        "summary": "WebGL runtime status collected from live Servo tab",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "source_url": runtime_result.get("url").cloned().unwrap_or(Value::Null),
        "source_title": runtime_result.get("title").cloned().unwrap_or(Value::Null),
        "runtime_status": runtime_result.get("runtime").cloned().unwrap_or(Value::Null),
        "artifacts": artifact_paths(state),
    })
}

fn webgl_page_probe_response(state: &Rc<WorkerState>, page_probe: &Value) -> Value {
    let canvases = page_probe
        .get("canvases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let visible_canvas_count = canvases
        .iter()
        .filter(|canvas| {
            canvas
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let webgl_canvas_count = canvases
        .iter()
        .filter(|canvas| {
            canvas
                .pointer("/context/type")
                .and_then(Value::as_str)
                .is_some_and(|context_type| context_type == "webgl" || context_type == "webgl2")
        })
        .count();
    log_replay(
        state,
        json!({
            "kind": "webgl_page_probed",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "canvas_count": canvases.len(),
            "visible_canvas_count": visible_canvas_count,
            "webgl_canvas_count": webgl_canvas_count,
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-webgl-page-v0",
        "summary": "Canvas/WebGL page structure collected from live Servo tab without reading form values",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "source_url": page_probe.get("url").cloned().unwrap_or(Value::Null),
        "source_title": page_probe.get("title").cloned().unwrap_or(Value::Null),
        "canvas_count": canvases.len(),
        "visible_canvas_count": visible_canvas_count,
        "webgl_canvas_count": webgl_canvas_count,
        "page_probe": page_probe,
        "artifacts": artifact_paths(state),
    })
}

fn editor_is_visible_writable(editor: &Value) -> bool {
    let width = editor
        .pointer("/rect/width")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let height = editor
        .pointer("/rect/height")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let hidden = editor
        .get("hidden")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let disabled = editor
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let read_only = editor
        .get("readOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let sensitivity = editor
        .get("sensitivity")
        .and_then(Value::as_str)
        .unwrap_or("none");

    width > 0.0 && height > 0.0 && !hidden && !disabled && !read_only && sensitivity == "none"
}

fn editor_is_visible_authoring(editor: &Value) -> bool {
    if !editor_is_visible_writable(editor) || editor_is_search_field(editor) {
        return false;
    }

    let kind = editor.get("kind").and_then(Value::as_str).unwrap_or("");
    if matches!(
        kind,
        "textarea" | "contenteditable" | "role_textbox" | "js_editor_shell"
    ) {
        return true;
    }

    let haystack = [
        editor.get("id").and_then(Value::as_str).unwrap_or(""),
        editor.get("name").and_then(Value::as_str).unwrap_or(""),
        editor.get("label").and_then(Value::as_str).unwrap_or(""),
        editor
            .get("placeholder")
            .and_then(Value::as_str)
            .unwrap_or(""),
        editor
            .get("ariaLabel")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ]
    .join(" ")
    .to_ascii_lowercase();

    [
        "title",
        "description",
        "filename",
        "gist",
        "body",
        "content",
        "comment",
        "message",
        "post",
        "reply",
        "note",
        "snippet",
    ]
    .iter()
    .any(|token| haystack.contains(token))
}

fn editor_is_search_field(editor: &Value) -> bool {
    let field_type = editor.get("type").and_then(Value::as_str).unwrap_or("");
    if field_type == "search" {
        return true;
    }

    let haystack = [
        editor.get("id").and_then(Value::as_str).unwrap_or(""),
        editor.get("name").and_then(Value::as_str).unwrap_or(""),
        editor.get("label").and_then(Value::as_str).unwrap_or(""),
        editor
            .get("placeholder")
            .and_then(Value::as_str)
            .unwrap_or(""),
        editor
            .get("ariaLabel")
            .and_then(Value::as_str)
            .unwrap_or(""),
    ]
    .join(" ")
    .to_ascii_lowercase();

    haystack.contains("search")
}

fn editor_route(
    editor_count: usize,
    zero_rect_count: usize,
    visible_writable_count: usize,
    visible_authoring_count: usize,
) -> Value {
    let (decision, summary) = if editor_count == 0 {
        ("no_editors", "No editor-like fields were found.")
    } else if visible_authoring_count == 0 && visible_writable_count > 0 {
        (
            "route_login_or_non_authoring_page",
            "Writable controls exist, but none look like a content-authoring editor; this is likely a login, search, or navigation page.",
        )
    } else if visible_authoring_count == 0 && zero_rect_count > 0 {
        (
            "route_user_focus_or_chrome_live",
            "Only zero-rect or hidden editor candidates are available; do not target them automatically.",
        )
    } else if visible_authoring_count > 0 && zero_rect_count > 0 {
        (
            "usable_ignore_hidden_backing_fields",
            "Visible writable editor candidates exist; hidden zero-rect backing fields should be ignored.",
        )
    } else if visible_authoring_count > 0 {
        (
            "usable_visible_editors",
            "Visible writable editor candidates exist.",
        )
    } else {
        (
            "needs_review",
            "Editor candidates exist but none are clearly writable and visible.",
        )
    };

    json!({
        "decision": decision,
        "summary": summary,
        "editor_count": editor_count,
        "zero_rect_count": zero_rect_count,
        "visible_writable_count": visible_writable_count,
        "visible_authoring_count": visible_authoring_count,
    })
}

fn type_focused_text_response(
    state: &Rc<WorkerState>,
    chars_requested: usize,
    before_probe: &Value,
    after_probe: &Value,
) -> Value {
    let before_length = before_probe
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let after_length = after_probe
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let changed = type_focused_changed(before_probe, after_probe);
    let field = json!({
        "tag": after_probe.get("tag").cloned().unwrap_or(Value::Null),
        "type": after_probe.get("type").cloned().unwrap_or(Value::Null),
        "id_present": after_probe.get("idPresent").cloned().unwrap_or(Value::Null),
        "name_present": after_probe.get("namePresent").cloned().unwrap_or(Value::Null),
        "contenteditable": after_probe.get("contentEditable").cloned().unwrap_or(Value::Null),
        "sensitivity": after_probe.get("sensitivity").cloned().unwrap_or(Value::Null),
    });
    let insert_method = after_probe
        .get("insertMethod")
        .cloned()
        .unwrap_or(Value::Null);
    log_replay(
        state,
        json!({
            "kind": "focused_text_typed",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "chars_requested": chars_requested,
            "before_length": before_length,
            "after_length": after_length,
            "changed": changed,
            "insert_method": insert_method,
            "field": field,
            "values_logged": false,
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "text typed into the current focused non-sensitive field",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "chars_requested": chars_requested,
        "before_length": before_length,
        "after_length": after_length,
        "changed": changed,
        "insert_method": insert_method,
        "field": field,
        "policy": {
            "active_element_only": true,
            "block_sensitive": true,
            "values_logged": false,
        },
        "artifacts": artifact_paths(state),
    })
}

fn type_focused_changed(before_probe: &Value, after_probe: &Value) -> bool {
    let before_length = before_probe
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let after_length = after_probe
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let inserted = after_probe
        .get("inserted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    before_length != after_length || inserted
}

fn formmax_live_fill_response(state: &Rc<WorkerState>, result: &Value) -> Value {
    let events = result
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for event in &events {
        let mut event = event.clone();
        if let Some(object) = event.as_object_mut() {
            object.insert("run_id".into(), json!(state.run_id.as_str()));
            object.insert("page_revision".into(), json!(state.page_revision.get()));
            object.insert("values_logged".into(), json!(false));
        }
        log_replay(state, event);
    }

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
    let receipt_row_count = result
        .pointer("/receipt/row_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    log_replay(
        state,
        json!({
            "kind": "formmax_live_fill_summary",
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "rows": rows,
            "pages": pages,
            "filled": filled,
            "blocked_sensitive": blocked_sensitive,
            "receipt_verified": receipt_verified,
            "validation_errors": validation_errors,
            "values_logged": false,
        }),
    );

    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-formmax-live-v0",
        "summary": "FORMMAX capacity fixture filled and verified inside the live Servo browser session",
        "rendering_profile": state.rendering_settings.profile.name(),
        "page_revision": state.page_revision.get(),
        "rows": rows,
        "pages": pages,
        "filled": filled,
        "blocked_sensitive": blocked_sensitive,
        "receipt_verified": receipt_verified,
        "validation_errors": validation_errors,
        "replay_events": events.len() + 1,
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
        "artifacts": artifact_paths(state),
    })
}

pub(crate) fn probe_changed(before_probe: &Value, after_probe: &Value) -> bool {
    before_probe.get("bodyTextLength") != after_probe.get("bodyTextLength")
        || before_probe.get("bodyChildCount") != after_probe.get("bodyChildCount")
        || before_probe.get("pageRevision") != after_probe.get("pageRevision")
        || before_probe
            .get("actions")
            .and_then(Value::as_array)
            .map(|actions| action_labels(actions))
            != after_probe
                .get("actions")
                .and_then(Value::as_array)
                .map(|actions| action_labels(actions))
}

pub(crate) fn action_map(probe: &Value) -> Vec<Value> {
    probe
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|action| {
            json!({
                "action_id": action_id_for(action),
                "label": probe_action_label(action),
                "kind": "click",
                "enabled": action_enabled(action),
                "sensitivity": action.get("sensitivity").cloned().unwrap_or_else(|| json!({"kind": "none"})),
                "blocked_by": action
                    .get("blockedBy")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty()),
                "rect": action.get("rect").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub(crate) fn sensitive_action_count(actions: &[Value]) -> usize {
    actions
        .iter()
        .filter(|action| {
            action
                .pointer("/sensitivity/kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "none")
        })
        .count()
}

fn audit_findings(probe: &Value, actions: &[Value]) -> Vec<Value> {
    let mut findings = Vec::new();
    let body_text_length = probe
        .get("bodyTextLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let body_child_count = probe
        .get("bodyChildCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if body_text_length == 0 && body_child_count == 0 {
        push_audit_finding(
            &mut findings,
            "blank_page",
            "error",
            "body",
            "Live tab has no body text or body children.",
            json!({
                "body_text_length": body_text_length,
                "body_child_count": body_child_count,
            }),
        );
    }

    for action in probe
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let action_id = action_id_for(action);
        let label = probe_action_label(action);
        if action
            .get("offscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            push_audit_finding(
                &mut findings,
                "offscreen_action",
                "warning",
                &action_id,
                "Action exists but is outside the current viewport.",
                json!({
                    "action_id": action_id.clone(),
                    "label": label,
                    "rect": action.get("rect").cloned().unwrap_or(Value::Null),
                }),
            );
            continue;
        }
        if let Some(blocked_by) = action
            .get("blockedBy")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            push_audit_finding(
                &mut findings,
                "blocked_action",
                "warning",
                &action_id,
                "Action center is covered by another visible element.",
                json!({
                    "action_id": action_id.clone(),
                    "label": label,
                    "blocked_by": blocked_by,
                }),
            );
        }
    }

    let sensitive_count = sensitive_action_count(actions);
    if sensitive_count > 0 {
        push_audit_finding(
            &mut findings,
            "sensitive_fields_require_user",
            "info",
            "form",
            "Sensitive fields are present and require user input or confirmation.",
            json!({
                "sensitive_fields": sensitive_count,
            }),
        );
    }

    findings
}

fn push_audit_finding(
    findings: &mut Vec<Value>,
    kind: &str,
    severity: &str,
    selector: &str,
    message: &str,
    evidence: Value,
) {
    findings.push(json!({
        "finding_id": format!("live_{:02}", findings.len() + 1),
        "kind": kind,
        "severity": severity,
        "selector": selector,
        "message": message,
        "evidence": evidence,
    }));
}

fn action_labels(actions: &[Value]) -> Vec<String> {
    actions.iter().map(probe_action_label).collect()
}

pub(crate) fn action_id_for(action: &Value) -> String {
    let label = probe_action_label(action);
    if label.eq_ignore_ascii_case("submit") {
        "act_submit".into()
    } else if label.eq_ignore_ascii_case("export") {
        "act_export".into()
    } else {
        format!(
            "act_{}",
            action.get("index").and_then(Value::as_u64).unwrap_or(0)
        )
    }
}

fn probe_action_label(action: &Value) -> String {
    action
        .get("label")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Action")
        .trim()
        .to_string()
}

pub(crate) fn action_enabled(action: &Value) -> bool {
    !action
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && action
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && !action
            .get("offscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && action.get("blockedBy").is_none_or(Value::is_null)
}

fn click_action(state: &Rc<WorkerState>, webview: &WebView, action: &Value) {
    let rect = action.get("rect").unwrap_or(&Value::Null);
    let x = (value_f64(rect, "left") + value_f64(rect, "width") / 2.0) as f32;
    let y = (value_f64(rect, "top") + value_f64(rect, "height") / 2.0) as f32;
    let page_point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(x, y));
    webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(page_point)));
    webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
        MouseButtonAction::Down,
        MouseButton::Left,
        page_point,
    )));
    webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
        MouseButtonAction::Up,
        MouseButton::Left,
        page_point,
    )));
    state.window.request_redraw();
}

fn type_text_into_focused_field(state: &Rc<WorkerState>, webview: &WebView, text: &str) {
    for character in text.chars() {
        let Some(key) = servo_key_for_command_char(character) else {
            continue;
        };
        webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Down,
            key.clone(),
        )));
        webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
            KeyState::Up,
            key,
        )));
    }
    state.window.request_redraw();
}

fn servo_key_for_command_char(character: char) -> Option<ServoKey> {
    match character {
        '\n' => Some(ServoKey::Named(ServoNamedKey::Enter)),
        '\t' => Some(ServoKey::Named(ServoNamedKey::Tab)),
        character if character.is_control() => None,
        character => Some(ServoKey::Character(character.to_string())),
    }
}

const TYPE_FOCUSED_PROBE_JS: &str = r#"
(() => {
  const el = document.activeElement;
  if (!el || el === document.body || el === document.documentElement) {
    return JSON.stringify({ ok: false, reason: "no_focused_field" });
  }

  function sensitivityOf(el) {
    const labelText = Array.from(el.labels || []).map((label) => label.textContent || "").join(" ");
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || "",
      labelText
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver.?license)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment|routing|bank)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  function writableKind(el) {
    const tag = el.tagName ? el.tagName.toLowerCase() : "";
    const type = (el.getAttribute("type") || "text").toLowerCase();
    if (tag === "textarea") return "textarea";
    if (tag === "input") {
      const allowed = new Set(["text", "search", "url", "email", "tel", "number"]);
      return allowed.has(type) ? "input" : "";
    }
    if (el.isContentEditable) return "contenteditable";
    return "";
  }

  function valueLength(el, kind) {
    if (kind === "contenteditable") return String(el.textContent || "").length;
    return String(el.value || "").length;
  }

  const sensitivity = sensitivityOf(el);
  if (sensitivity !== "none") {
    return JSON.stringify({
      ok: false,
      reason: "focused_field_sensitive",
      sensitivity
    });
  }
  const kind = writableKind(el);
  if (!kind) {
    return JSON.stringify({
      ok: false,
      reason: "focused_element_not_text_writable",
      tag: el.tagName ? el.tagName.toLowerCase() : "",
      type: (el.getAttribute("type") || "").toLowerCase()
    });
  }

  return JSON.stringify({
    ok: true,
    tag: el.tagName ? el.tagName.toLowerCase() : "",
    type: (el.getAttribute("type") || "").toLowerCase(),
    contentEditable: Boolean(el.isContentEditable),
    idPresent: Boolean(el.id),
    namePresent: Boolean(el.getAttribute("name")),
    sensitivity,
    valueLength: valueLength(el, kind)
  });
})()
"#;

const INSPECT_EDITORS_JS: &str = r##"
(() => {
  function textOf(value) {
    return String(value || "").replace(/\s+/g, " ").trim().slice(0, 120);
  }

  function rectOf(el) {
    const rect = el.getBoundingClientRect();
    return {
      left: rect.left,
      top: rect.top,
      right: rect.right,
      bottom: rect.bottom,
      width: rect.width,
      height: rect.height
    };
  }

  function sensitivityOf(el) {
    const labelText = Array.from(el.labels || []).map((label) => label.textContent || "").join(" ");
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || "",
      labelText
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver.?license)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment|routing|bank)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  function editorKind(el) {
    const tag = el.tagName ? el.tagName.toLowerCase() : "";
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (tag === "textarea") return "textarea";
    if (tag === "input" && (!type || ["text", "search", "url", "email", "tel", "number"].includes(type))) {
      return "input";
    }
    if (el.isContentEditable) return "contenteditable";
    const role = (el.getAttribute("role") || "").toLowerCase();
    if (role === "textbox") return "role_textbox";
    const className = String(el.className || "");
    if (/\b(cm-content|CodeMirror|CodeMirror-code|ace_editor|ace_text-input)\b/.test(className)) {
      return "js_editor_shell";
    }
    return "";
  }

  function valueLength(el, kind) {
    if (kind === "input" || kind === "textarea") return String(el.value || "").length;
    return String(el.textContent || "").length;
  }

  const selectors = [
    "textarea",
    "input",
    "[contenteditable='true']",
    "[role='textbox']",
    ".cm-content",
    ".CodeMirror",
    ".CodeMirror-code",
    ".ace_editor",
    ".ace_text-input"
  ];
  const seen = new Set();
  const elements = [];
  for (const selector of selectors) {
    for (const el of Array.from(document.querySelectorAll(selector))) {
      if (!seen.has(el)) {
        seen.add(el);
        elements.push(el);
      }
    }
  }

  const active = document.activeElement;
  const editors = elements
    .map((el, index) => {
      const kind = editorKind(el);
      if (!kind) return null;
      const style = window.getComputedStyle(el);
      const rect = rectOf(el);
      const labelText = Array.from(el.labels || [])
        .map((label) => label.textContent || "")
        .join(" ");
      const hidden =
        el.hidden ||
        style.display === "none" ||
        style.visibility === "hidden" ||
        (rect.width <= 0 || rect.height <= 0);
      return {
        index,
        kind,
        tag: el.tagName ? el.tagName.toLowerCase() : "",
        type: (el.getAttribute("type") || "").toLowerCase(),
        id: el.id || "",
        name: el.getAttribute("name") || "",
        role: el.getAttribute("role") || "",
        className: textOf(el.className || ""),
        ariaLabel: textOf(el.getAttribute("aria-label") || ""),
        placeholder: textOf(el.getAttribute("placeholder") || ""),
        label: textOf(labelText),
        autocomplete: textOf(el.getAttribute("autocomplete") || ""),
        disabled: Boolean(el.disabled),
        readOnly: Boolean(el.readOnly),
        contentEditable: Boolean(el.isContentEditable),
        active: el === active,
        hidden,
        rect,
        valueLength: valueLength(el, kind),
        sensitivity: sensitivityOf(el)
      };
    })
    .filter(Boolean);

  return JSON.stringify({
    ok: true,
    url: location.href,
    title: document.title,
    activeTag: active && active.tagName ? active.tagName.toLowerCase() : "",
    activeId: active && active.id ? active.id : "",
    editorCount: editors.length,
    sensitiveFieldsSeen: editors.filter((editor) => editor.sensitivity !== "none").length,
    editors
  });
})()
"##;

const WEBGL_RUNTIME_PROBE_JS: &str = r##"
(() => {
  const runtime = window.__saccadeWebglRuntime || null;
  return JSON.stringify({
    ok: true,
    url: location.href,
    title: document.title || "",
    runtime
  });
})()
"##;

const WEBGL_PAGE_PROBE_JS: &str = include_str!("../../../scripts/webgl_page_probe.js");

fn type_focused_contenteditable_insert_script(text_json: &str) -> String {
    format!(
        r#"
(() => {{
  const text = {text_json};
  const el = document.activeElement;
  if (!el || el === document.body || el === document.documentElement) {{
    return JSON.stringify({{ ok: false, reason: "no_focused_field" }});
  }}

  function sensitivityOf(el) {{
    const labelText = Array.from(el.labels || []).map((label) => label.textContent || "").join(" ");
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || "",
      labelText
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver.?license)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment|routing|bank)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }}

  function probe(extra) {{
    const tag = el.tagName ? el.tagName.toLowerCase() : "";
    const type = (el.getAttribute("type") || "").toLowerCase();
    return Object.assign({{
      ok: true,
      tag,
      type,
      contentEditable: Boolean(el.isContentEditable),
      idPresent: Boolean(el.id),
      namePresent: Boolean(el.getAttribute("name")),
      sensitivity: "none",
      valueLength: String(el.textContent || "").length
    }}, extra || {{}});
  }}

  const sensitivity = sensitivityOf(el);
  if (sensitivity !== "none") {{
    return JSON.stringify({{
      ok: false,
      reason: "focused_field_sensitive",
      sensitivity
    }});
  }}
  if (!el.isContentEditable) {{
    return JSON.stringify({{
      ok: false,
      reason: "focused_element_not_contenteditable",
      tag: el.tagName ? el.tagName.toLowerCase() : "",
      type: (el.getAttribute("type") || "").toLowerCase()
    }});
  }}

  let inserted = false;
  let insertMethod = "none";
  el.focus();

  try {{
    if (
      typeof document.queryCommandSupported === "function" &&
      document.queryCommandSupported("insertText")
    ) {{
      inserted = document.execCommand("insertText", false, text);
      if (inserted) insertMethod = "execCommand.insertText";
    }}
  }} catch (_) {{}}

  if (!inserted) {{
    try {{
      const selection = window.getSelection && window.getSelection();
      if (selection) {{
        let range = selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
        if (
          !range ||
          (range.commonAncestorContainer !== el && !el.contains(range.commonAncestorContainer))
        ) {{
          range = document.createRange();
          range.selectNodeContents(el);
          range.collapse(false);
          selection.removeAllRanges();
          selection.addRange(range);
        }}
        range.deleteContents();
        const node = document.createTextNode(text);
        range.insertNode(node);
        range.setStartAfter(node);
        range.setEndAfter(node);
        selection.removeAllRanges();
        selection.addRange(range);
        inserted = true;
        insertMethod = "selection.range";
      }}
    }} catch (_) {{}}
  }}

  if (!inserted) {{
    el.textContent = String(el.textContent || "") + text;
    inserted = true;
    insertMethod = "textContent.append";
  }}

  try {{
    const event =
      typeof InputEvent === "function"
        ? new InputEvent("input", {{ bubbles: true, inputType: "insertText", data: text }})
        : new Event("input", {{ bubbles: true }});
    el.dispatchEvent(event);
  }} catch (_) {{
    try {{
      el.dispatchEvent(new Event("input", {{ bubbles: true }}));
    }} catch (__) {{}}
  }}

  return JSON.stringify(probe({{ inserted: Boolean(inserted), insertMethod }}));
}})()
"#
    )
}

pub(crate) fn fill_agent_fields_script(fields_json: &str) -> String {
    format!(
        r#"
(() => {{
  const requested = {fields_json};
  const filled = [];
  const rejected = [];

  function sensitivityOf(el) {{
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }}

  for (const [id, value] of Object.entries(requested)) {{
    const el = document.getElementById(id);
    if (!el) {{
      rejected.push({{ id, reason: "not_found" }});
      continue;
    }}
    const owner = el.getAttribute("data-owner") || "";
    const declaredSensitivity = el.getAttribute("data-sensitive") || "none";
    const sensitivity = sensitivityOf(el);
    if (owner !== "agent" || declaredSensitivity !== "none" || sensitivity !== "none") {{
      rejected.push({{ id, reason: "not_agent_owned_non_sensitive", owner, sensitivity }});
      continue;
    }}
    if (el.type === "checkbox") {{
      el.checked = Boolean(value);
    }} else {{
      el.value = String(value);
    }}
    el.dispatchEvent(new Event("input", {{ bubbles: true }}));
    el.dispatchEvent(new Event("change", {{ bubbles: true }}));
    filled.push(id);
  }}

  const body = document.body;
  const previousRevision = Number(body && body.dataset ? (body.dataset.sessionRevision || "0") : "0") || 0;
  if (filled.length && body && body.dataset) {{
    body.dataset.sessionRevision = String(previousRevision + 1);
  }}
  const sensitiveFieldsSeen = Array.from(document.querySelectorAll("input, select, textarea"))
    .filter((el) => sensitivityOf(el) !== "none" || (el.getAttribute("data-sensitive") || "none") !== "none")
    .length;
  return JSON.stringify({{
    filled,
    rejected,
    pageRevision: body && body.dataset ? Number(body.dataset.sessionRevision || "0") || 0 : 0,
    sensitiveFieldsSeen
  }});
}})()
"#
    )
}

pub(crate) fn inspect_fields_script(fields_json: &str) -> String {
    format!(
        r#"
(() => {{
  const requested = {fields_json};
  const fields = [];

  function sensitivityOf(el) {{
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }}

  function fieldValue(el) {{
    if (el.type === "checkbox") return Boolean(el.checked);
    return String(el.value || "");
  }}

  function hasValue(el) {{
    if (el.type === "checkbox") return Boolean(el.checked);
    return String(el.value || "").trim().length > 0;
  }}

  for (const id of requested) {{
    const el = document.getElementById(id);
    if (!el) {{
      fields.push({{ id, status: "not_found" }});
      continue;
    }}
    const owner = el.getAttribute("data-owner") || "";
    const declaredSensitivity = el.getAttribute("data-sensitive") || "none";
    const sensitivity = sensitivityOf(el);
    const completionState = sensitivity === "none" && declaredSensitivity === "none"
      ? (hasValue(el) ? "value_present" : "empty")
      : (hasValue(el) ? "completed_without_value" : "requires_user_input");
    const record = {{
      id,
      status: "ok",
      owner,
      declared_sensitivity: declaredSensitivity,
      sensitivity,
      completion_state: completionState
    }};
    if (sensitivity === "none" && declaredSensitivity === "none") {{
      record.value = fieldValue(el);
      record.value_returned = true;
    }} else {{
      record.value_redacted = true;
    }}
    fields.push(record);
  }}

  const sensitiveFieldsSeen = Array.from(document.querySelectorAll("input, select, textarea"))
    .filter((el) => sensitivityOf(el) !== "none" || (el.getAttribute("data-sensitive") || "none") !== "none")
    .length;
  return JSON.stringify({{ fields, sensitiveFieldsSeen }});
}})()
"#
    )
}

pub(crate) const FORMMAX_LIVE_FILL_JS: &str = r##"
(() => {
  const fixture = window.__FORMMAX_FIXTURE;
  const module = window.FormmaxFixture;
  if (!fixture || !module) throw new Error("FORMMAX fixture API is missing");

  const startedAt = Date.now();
  const events = [];
  const rows = fixture.rows || [];
  const pages = fixture.pages || [];
  const fieldSpecs = fixture.fieldSpecs || [];
  const sensitiveFields = fixture.sensitiveFields || [];
  const scroller = document.getElementById("table-scroll");
  const submit = document.getElementById("submit-page");
  if (!scroller || !submit) throw new Error("FORMMAX fixture controls are missing");

  function emit(kind, data = {}) {
    events.push(Object.assign({
      kind,
      ts_ms: Date.now() - startedAt,
      echo_values: false
    }, data));
  }

  function event(type) {
    return new Event(type, { bubbles: true });
  }

  function renderedRows() {
    return Array.from(document.querySelectorAll("#capacity-body tr"));
  }

  function controlFor(row, spec) {
    return document.getElementsByName(row.id + "_" + spec.key)[0] || null;
  }

  function setControlValue(control, spec, expected) {
    control.focus();
    emit("field_focused", {
      row_id: expected.id,
      field: spec.key,
      control: control.tagName.toLowerCase(),
      input_type: control.type || null
    });
    if (spec.kind === "checkbox") {
      control.checked = Boolean(expected[spec.key]);
    } else {
      control.value = String(expected[spec.key]);
    }
    control.dispatchEvent(event("input"));
    control.dispatchEvent(event("change"));
  }

  function controlMatches(control, spec, expected) {
    if (spec.kind === "checkbox") return control.checked === Boolean(expected[spec.key]);
    if (spec.kind === "number") return Number(control.value) === Number(expected[spec.key]);
    return control.value === String(expected[spec.key]);
  }

  function ensureAllRowsRendered(pageIndex) {
    const expected = pages[pageIndex].length;
    let guard = 0;
    emit("scroll_checkpoint", {
      page: pageIndex + 1,
      rendered_rows: renderedRows().length,
      target_rows: expected
    });
    while (renderedRows().length < expected && guard < 20) {
      scroller.scrollTop = scroller.scrollHeight;
      scroller.dispatchEvent(event("scroll"));
      emit("scroll_checkpoint", {
        page: pageIndex + 1,
        rendered_rows: renderedRows().length,
        target_rows: expected
      });
      guard += 1;
    }
    if (renderedRows().length < expected) {
      throw new Error(`page ${pageIndex + 1} rendered ${renderedRows().length} of ${expected} rows`);
    }
  }

  function fillPage(pageIndex) {
    emit("page_started", { page: pageIndex + 1, rows: pages[pageIndex].length });
    ensureAllRowsRendered(pageIndex);
    let filled = 0;
    for (const row of pages[pageIndex]) {
      for (const spec of fieldSpecs) {
        const control = controlFor(row, spec);
        emit("field_discovered", {
          page: pageIndex + 1,
          row_id: row.id,
          field: spec.key,
          sensitive: false,
          control_found: Boolean(control)
        });
        if (!control) throw new Error(`missing control ${row.id}_${spec.key}`);
        setControlValue(control, spec, row);
        filled += 1;
        emit("field_filled", {
          page: pageIndex + 1,
          row_id: row.id,
          field: spec.key,
          value_echoed: false
        });
        const ok = controlMatches(control, spec, row);
        emit("field_verified", {
          page: pageIndex + 1,
          row_id: row.id,
          field: spec.key,
          passed: ok
        });
        if (!ok) throw new Error(`verification failed for ${row.id}_${spec.key}`);
      }
    }
    return filled;
  }

  function blockSensitiveFields() {
    let blocked = 0;
    for (const field of sensitiveFields) {
      const control = document.querySelector(`[data-sensitive="${field.name}"]`);
      const hasValue = control
        ? (control.type === "checkbox" ? control.checked : control.value !== "")
        : false;
      emit("field_discovered", {
        field: field.name,
        label: field.label,
        sensitive: true,
        reason: field.reason,
        control_found: Boolean(control)
      });
      emit("confirmation_required", {
        field: field.name,
        reason: field.reason,
        status: "requires_user_input",
        value_echoed: false
      });
      emit("field_blocked_sensitive", {
        field: field.name,
        reason: field.reason,
        value_present: hasValue,
        value_echoed: false
      });
      if (hasValue) throw new Error(`sensitive field unexpectedly had value: ${field.name}`);
      blocked += 1;
    }
    return blocked;
  }

  emit("form_run_started", {
    engine: "saccade-browser-session-formmax-live-v0",
    rows: rows.length,
    pages: pages.length,
    policy: {
      block_sensitive: true,
      local_fixture_only: true,
      same_live_tab: true,
      echo_values: false
    }
  });

  let filled = fillPage(0);
  submit.focus();
  submit.click();
  emit("page_next_clicked", { from_page: 1, to_page: 2 });
  emit("validation_seen", { page: 1, errors: 0 });

  filled += fillPage(1);
  const blocked = blockSensitiveFields();
  submit.focus();
  submit.click();
  emit("page_next_clicked", { from_page: 2, to_page: "receipt", local_fixture_only: true });

  const receiptText = document.getElementById("receipt").textContent || "{}";
  const receipt = JSON.parse(receiptText);
  const validation = receipt.validation || module.validateReceipt(rows, receipt);
  const validationErrors = (validation.failures || []).length;
  const receiptPanel = document.getElementById("receipt-panel");
  if (receiptPanel) receiptPanel.scrollIntoView({ block: "start" });
  emit("receipt_seen", {
    row_count: receipt.row_count,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors
  });
  emit("form_transaction_finished", {
    rows: rows.length,
    pages: pages.length,
    filled,
    blocked_sensitive: blocked,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors
  });

  return JSON.stringify({
    engine: "saccade-browser-session-formmax-live-v0",
    runtime: "browser_session_worker_v0",
    rows: rows.length,
    pages: pages.length,
    filled,
    blocked_sensitive: blocked,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors,
    replay_events: events.length,
    events,
    receipt
  });
})()
"##;

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

fn respond_ok(state: &Rc<WorkerState>, id: Value, result: Value) {
    write_report(state, &result);
    write_response(
        state,
        json!({
            "id": id,
            "ok": true,
            "result": result,
        }),
    );
}

fn respond_error(state: &Rc<WorkerState>, id: Value, error: String) {
    write_response(
        state,
        json!({
            "id": id,
            "ok": false,
            "error": error,
        }),
    );
}

fn write_response(state: &Rc<WorkerState>, response: Value) {
    let mut stdout = state.stdout.borrow_mut();
    let _ = writeln!(stdout, "{response}");
    let _ = stdout.flush();
}

fn value_f64(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn artifact_paths(state: &Rc<WorkerState>) -> Value {
    json!({
        "run_dir": state.output_dir.display().to_string(),
        "report": state.report_path.display().to_string(),
        "replay": state.replay_path.display().to_string(),
        "screenshots": state.screenshots.borrow().clone(),
        "screenshot_skipped_sensitive": state.screenshot_skipped_sensitive.get(),
    })
}

fn write_report(state: &Rc<WorkerState>, latest: &Value) {
    let report = json!({
        "run_id": state.run_id.as_str(),
        "engine": "saccade-browser-session-worker-v0",
        "url": state.target_url.as_str(),
        "rendering_profile": state.rendering_settings.profile.name(),
        "renderer_engine": state.rendering_settings.profile.engine(),
        "servo_grid_enabled": state.rendering_settings.layout_grid_enabled,
        "legacy_grid_override": state.rendering_settings.legacy_grid_override,
        "experimental_prefs": state.rendering_settings.experimental_prefs(),
        "page_revision": state.page_revision.get(),
        "latest": latest,
        "artifacts": artifact_paths(state),
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
        let _ = fs::write(&state.report_path, bytes);
    }
}

fn maybe_save_screenshot(
    state: &Rc<WorkerState>,
    webview: &WebView,
    filename: &str,
    sensitive_count: usize,
) -> Option<String> {
    if sensitive_count > 0 {
        state
            .screenshot_skipped_sensitive
            .set(state.screenshot_skipped_sensitive.get().saturating_add(1));
        log_replay(
            state,
            json!({
                "kind": "screenshot_skipped_sensitive_fields",
                "run_id": state.run_id.as_str(),
                "filename": filename,
                "sensitive_fields": sensitive_count,
            }),
        );
        return None;
    }

    let context_size = state.rendering_context.size2d();
    let rect = DeviceIntRect::from_size(DeviceIntSize::new(
        context_size.width as i32,
        context_size.height as i32,
    ));
    let path = state.output_dir.join(filename);
    for attempt in 1..=5 {
        webview.paint();
        state.window.request_redraw();
        match state.rendering_context.read_to_image(rect) {
            Some(image) => {
                let non_white_ratio = image_non_white_ratio(&image);
                if non_white_ratio <= 0.0005 && attempt < 5 {
                    thread::sleep(Duration::from_millis(80));
                    continue;
                }
                if image.save(&path).is_ok() {
                    let path = path.display().to_string();
                    state.screenshots.borrow_mut().push(path.clone());
                    log_replay(
                        state,
                        json!({
                            "kind": "screenshot_saved",
                            "run_id": state.run_id.as_str(),
                            "path": path,
                            "non_white_ratio": non_white_ratio,
                            "attempt": attempt,
                        }),
                    );
                    return Some(path);
                }
            }
            None => {
                if attempt < 5 {
                    thread::sleep(Duration::from_millis(80));
                    continue;
                }
            }
        };
    }
    None
}

fn image_non_white_ratio(image: &image::RgbaImage) -> f64 {
    let mut sampled = 0u64;
    let mut non_white = 0u64;
    for pixel in image.pixels().step_by(16) {
        sampled += 1;
        let [r, g, b, a] = pixel.0;
        if a > 8 && (r < 245 || g < 245 || b < 245) {
            non_white += 1;
        }
    }
    if sampled == 0 {
        0.0
    } else {
        non_white as f64 / sampled as f64
    }
}

fn log_replay(state: &Rc<WorkerState>, event: Value) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&state.replay_path)
    {
        let _ = writeln!(file, "{event}");
    }
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX_EPOCH")?
        .as_millis())
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

pub(crate) const PROBE_JS: &str = r##"
(() => {
  const viewport = { width: window.innerWidth || 0, height: window.innerHeight || 0 };
  const body = document.body;
  const bodyTextLength = body ? ((body.innerText || body.textContent || "").trim().length) : 0;
  const pageRevision = Number(body && body.dataset ? (body.dataset.sessionRevision || "0") : "0") || 0;

  function rectOf(el) {
    const rect = el.getBoundingClientRect();
    return {
      left: rect.left,
      top: rect.top,
      right: rect.right,
      bottom: rect.bottom,
      width: rect.width,
      height: rect.height
    };
  }

  function centerOf(rect) {
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  function visibleRect(rect) {
    return rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.bottom > 0 &&
      rect.left < viewport.width && rect.top < viewport.height;
  }

  function offscreen(rect) {
    return rect.width > 0 && rect.height > 0 &&
      (rect.right <= 0 || rect.bottom <= 0 || rect.left >= viewport.width || rect.top >= viewport.height);
  }

  function label(el) {
    if (!el) return "";
    if (el.id) return "#" + el.id;
    if (el.className && typeof el.className === "string") return "." + el.className.trim().split(/\s+/).join(".");
    const text = (el.innerText || el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 40);
    return el.tagName.toLowerCase() + (text ? ":" + text : "");
  }

  function fieldToken(el) {
    return [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
  }

  function sensitivityOf(el) {
    const token = fieldToken(el);
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(passport|driver|license|national id|government)\b/.test(token)) return "government_or_tax_id";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  function safeLabel(el, sensitivity) {
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    const role = el.getAttribute("role") || "";
    const isCommandInput = tag === "input" && ["button", "submit", "reset"].includes(type);
    if (tag === "button" || tag === "a" || role === "button" || isCommandInput) {
      return (el.innerText || el.textContent || el.getAttribute("aria-label") || el.value || el.getAttribute("href") || el.tagName).trim();
    }
    const descriptor = (el.getAttribute("aria-label") || el.getAttribute("placeholder") || el.getAttribute("name") || el.id || el.tagName).trim();
    return sensitivity === "none" ? descriptor : `${descriptor || tag} (${sensitivity})`;
  }

  function completionState(el, sensitivity) {
    if (sensitivity === "none") return "not_sensitive";
    const hasEntry = el.type === "checkbox" || el.type === "radio" ? !!el.checked : !!String(el.value || "");
    return hasEntry ? "completed_without_value" : "requires_user_input";
  }

  function layoutProbeOf(el) {
    const style = getComputedStyle(el);
    return {
      name: el.getAttribute("data-saccade-probe") || "",
      tag: el.tagName.toLowerCase(),
      rect: rectOf(el),
      display: style.display || "",
      position: style.position || "",
      gridTemplateColumns: style.gridTemplateColumns || "",
      gridTemplateRows: style.gridTemplateRows || "",
      columnGap: style.columnGap || "",
      rowGap: style.rowGap || "",
      flexDirection: style.flexDirection || "",
      flexBasis: style.flexBasis || "",
      flexGrow: style.flexGrow || "",
      flexShrink: style.flexShrink || "",
      alignSelf: style.alignSelf || "",
      justifySelf: style.justifySelf || "",
      boxSizing: style.boxSizing || "",
      width: style.width || "",
      height: style.height || "",
      minWidth: style.minWidth || "",
      maxWidth: style.maxWidth || "",
      minHeight: style.minHeight || "",
      maxHeight: style.maxHeight || "",
      overflowX: style.overflowX || "",
      overflowY: style.overflowY || "",
      paddingLeft: style.paddingLeft || "",
      paddingRight: style.paddingRight || "",
      borderLeftWidth: style.borderLeftWidth || "",
      borderRightWidth: style.borderRightWidth || "",
      fontSize: style.fontSize || "",
      lineHeight: style.lineHeight || "",
      gridColumnStart: style.gridColumnStart || "",
      gridColumnEnd: style.gridColumnEnd || "",
      maxWidth760: window.matchMedia ? window.matchMedia("(max-width: 760px)").matches : null
    };
  }

  const elements = Array.from(document.querySelectorAll("body *"));
  const blockers = elements.map((el, index) => {
    const style = getComputedStyle(el);
    const rect = rectOf(el);
    return { el, index, style, rect };
  }).filter(item => {
    const pos = item.style.position;
    const pointer = item.style.pointerEvents;
    const visible = item.style.display !== "none" && item.style.visibility !== "hidden" && item.style.opacity !== "0";
    const area = item.rect.width * item.rect.height;
    return visible && pointer !== "none" && area > 1000 && visibleRect(item.rect) &&
      (pos === "fixed" || pos === "absolute");
  });

  const actions = Array.from(document.querySelectorAll("button, a, input, select, textarea, [role='button']")).map((el, index) => {
    const rect = rectOf(el);
    const center = centerOf(rect);
    const style = getComputedStyle(el);
    const sensitivity = sensitivityOf(el);
    const action = {
      index,
      label: safeLabel(el, sensitivity),
      tag: el.tagName.toLowerCase(),
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      rect,
      offscreen: offscreen(rect),
      visible: visibleRect(rect) && style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0",
      blockedBy: null,
      sensitivity: {
        kind: sensitivity,
        completion_state: completionState(el, sensitivity)
      }
    };
    for (const blocker of blockers) {
      if (blocker.el === el || el.contains(blocker.el) || blocker.el.contains(el)) continue;
      const b = blocker.rect;
      if (center.x >= b.left && center.x <= b.right && center.y >= b.top && center.y <= b.bottom) {
        action.blockedBy = label(blocker.el);
        break;
      }
    }
    return action;
  });

  return JSON.stringify({
    engine: "saccade-browser-session-worker-v0",
    title: document.title || "",
    url: location.href,
    viewport,
    bodyTextLength,
    bodyChildCount: body ? body.children.length : 0,
    pageRevision,
    layoutProbes: Array.from(document.querySelectorAll("[data-saccade-probe]")).map(layoutProbeOf),
    actions
  });
})()
"##;
