use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use servo::{
    CSSPixel, DeviceIntRect, DeviceIntSize, InputEvent, InputEventId, InputEventResult, JSValue,
    Key, KeyState, KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent,
    MouseMoveEvent, RenderingContext, Servo, ServoBuilder, WebView, WebViewBuilder,
    WebViewDelegate, WebViewPoint, WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::{RenderingProfile, RenderingProfileSettings};

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 900;
const FORMMAX_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormmaxRunReport {
    pub engine: String,
    #[serde(default)]
    pub rendering_profile: String,
    #[serde(default)]
    pub servo_grid_enabled: bool,
    pub rows: usize,
    pub pages: usize,
    pub filled: usize,
    pub blocked_sensitive: usize,
    pub receipt_verified: bool,
    pub validation_errors: usize,
    pub replay_events: usize,
    pub events: Vec<Value>,
    pub receipt: Value,
    #[serde(default)]
    pub screenshots: Vec<String>,
    #[serde(default)]
    pub native_input: Value,
}

#[derive(Debug, Clone)]
pub struct FormmaxRunConfig {
    pub url: Url,
    pub artifact_dir: Option<PathBuf>,
    pub rendering_profile: Option<RenderingProfile>,
}

pub fn run_formmax_fixture(url: Url) -> Result<FormmaxRunReport> {
    run_formmax_fixture_with_config(FormmaxRunConfig {
        url,
        artifact_dir: None,
        rendering_profile: None,
    })
}

pub fn run_formmax_fixture_with_config(config: FormmaxRunConfig) -> Result<FormmaxRunReport> {
    let rendering_settings = RenderingProfile::resolve(config.rendering_profile)?;
    if rendering_settings.profile == RenderingProfile::ChromeReference {
        bail!("chrome-reference is not supported by the Servo FORMMAX runner");
    }
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = FormmaxApp::new(&event_loop, config, rendering_settings, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(report)) => Ok(report),
        Some(Err(message)) => bail!(message),
        None => bail!("FORMMAX runner exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    BeforeScreenshot,
    NativePlanRequested,
    NativeVerifyReady,
    NativeVerifyRequested,
    DriveRequested,
    AfterScreenshot,
    Done,
}

#[derive(Debug, Clone)]
struct SentInput {
    id: InputEventId,
    label: &'static str,
}

#[derive(Debug, Clone)]
struct HandledInput {
    label: &'static str,
    dispatch_failed: bool,
}

struct FormmaxState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webview: RefCell<Option<WebView>>,
    config: FormmaxRunConfig,
    started_at: Instant,
    phase: Cell<Phase>,
    phase_started_at: RefCell<Instant>,
    rendering_settings: RenderingProfileSettings,
    pending_native: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_drive: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_report: RefCell<Option<FormmaxRunReport>>,
    native_input: RefCell<Option<Value>>,
    sent_inputs: RefCell<Vec<SentInput>>,
    handled_inputs: RefCell<Vec<HandledInput>>,
    screenshots: RefCell<Vec<String>>,
    result: Rc<RefCell<Option<std::result::Result<FormmaxRunReport, String>>>>,
}

impl WebViewDelegate for FormmaxState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }

    fn notify_input_event_handled(
        &self,
        _webview: WebView,
        event_id: InputEventId,
        result: InputEventResult,
    ) {
        let label = self
            .sent_inputs
            .borrow()
            .iter()
            .find(|input| input.id == event_id)
            .map(|input| input.label)
            .unwrap_or("unknown");
        self.handled_inputs.borrow_mut().push(HandledInput {
            label,
            dispatch_failed: result.contains(InputEventResult::DispatchFailed),
        });
    }
}

enum FormmaxApp {
    Initial {
        waker: Waker,
        config: FormmaxRunConfig,
        rendering_settings: RenderingProfileSettings,
        result: Rc<RefCell<Option<std::result::Result<FormmaxRunReport, String>>>>,
    },
    Running {
        state: Rc<FormmaxState>,
    },
    Finished,
}

impl FormmaxApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        config: FormmaxRunConfig,
        rendering_settings: RenderingProfileSettings,
        result: Rc<RefCell<Option<std::result::Result<FormmaxRunReport, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            config,
            rendering_settings,
            result,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        if state.started_at.elapsed() > FORMMAX_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!(
                    "FORMMAX runner timed out after {FORMMAX_TIMEOUT:?} in phase {:?}",
                    state.phase.get()
                ),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webview.borrow().clone() else {
            return;
        };

        match state.phase.get() {
            Phase::Load if webview.load_status() == LoadStatus::Complete => {
                set_phase(&state, Phase::BeforeScreenshot);
            }
            Phase::BeforeScreenshot
                if state.phase_started_at.borrow().elapsed() >= Duration::from_millis(220) =>
            {
                save_artifact_screenshot(&state, &webview, "before.png");
                request_native_plan(&state, &webview);
                set_phase(&state, Phase::NativePlanRequested);
            }
            Phase::NativePlanRequested => {
                let Some(raw) = finish_probe(&state.pending_native) else {
                    return;
                };
                let Ok(plan) = serde_json::from_str::<Value>(&raw) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("failed to parse FORMMAX native input plan: {raw:?}"),
                    );
                    *self = Self::Finished;
                    return;
                };
                let Some((x, y)) = probe_input_center(&plan) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("FORMMAX native input plan missing rect: {plan}"),
                    );
                    *self = Self::Finished;
                    return;
                };
                let Some(text) = plan.get("text").and_then(Value::as_str) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("FORMMAX native input plan missing text: {plan}"),
                    );
                    *self = Self::Finished;
                    return;
                };
                click_page_point(&state, &webview, x, y);
                type_text(&state, &webview, text);
                set_phase(&state, Phase::NativeVerifyReady);
            }
            Phase::NativeVerifyReady
                if state.phase_started_at.borrow().elapsed() >= Duration::from_millis(220) =>
            {
                request_native_verify(&state, &webview);
                set_phase(&state, Phase::NativeVerifyRequested);
            }
            Phase::NativeVerifyRequested => {
                let Some(raw) = finish_probe(&state.pending_native) else {
                    return;
                };
                let Ok(mut native_input) = serde_json::from_str::<Value>(&raw) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("failed to parse FORMMAX native input verification: {raw:?}"),
                    );
                    *self = Self::Finished;
                    return;
                };
                append_native_input_outcomes(&state, &mut native_input);
                if native_input.get("value_matches").and_then(Value::as_bool) != Some(true)
                    || native_input
                        .get("dispatch_failed_keyboard_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(1)
                        != 0
                {
                    finish_err(
                        &state,
                        event_loop,
                        format!("FORMMAX native input verification failed: {native_input}"),
                    );
                    *self = Self::Finished;
                    return;
                }
                *state.native_input.borrow_mut() = Some(native_input);
                request_drive(&state, &webview);
                set_phase(&state, Phase::DriveRequested);
            }
            Phase::DriveRequested => {
                let Some(raw) = finish_drive(&state.pending_drive) else {
                    return;
                };
                match serde_json::from_str::<FormmaxRunReport>(&raw) {
                    Ok(report) => {
                        *state.pending_report.borrow_mut() =
                            Some(report_with_native_input(&state, report));
                        set_phase(&state, Phase::AfterScreenshot);
                    }
                    Err(error) => {
                        finish_err(
                            &state,
                            event_loop,
                            format!("failed to parse FORMMAX report JSON: {error}; raw={raw:?}"),
                        );
                        *self = Self::Finished;
                    }
                }
            }
            Phase::AfterScreenshot
                if state.phase_started_at.borrow().elapsed() >= Duration::from_millis(220) =>
            {
                save_artifact_screenshot(&state, &webview, "after.png");
                match state.pending_report.borrow_mut().take() {
                    Some(mut report) => {
                        report.screenshots = state.screenshots.borrow().clone();
                        finish_ok(&state, event_loop, report);
                        set_phase(&state, Phase::Done);
                        *self = Self::Finished;
                    }
                    None => {
                        finish_err(
                            &state,
                            event_loop,
                            "FORMMAX runner reached screenshot phase without a report",
                        );
                        *self = Self::Finished;
                    }
                }
            }
            Phase::Done => {}
            _ => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for FormmaxApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            config,
            rendering_settings,
            result,
        } = self
        else {
            return;
        };

        let display_handle = match event_loop.display_handle() {
            Ok(handle) => handle,
            Err(error) => {
                *result.borrow_mut() = Some(Err(format!("failed to get display handle: {error}")));
                event_loop.exit();
                return;
            }
        };

        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Saccade FORMMAX Runner")
                .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
        ) {
            Ok(window) => window,
            Err(error) => {
                *result.borrow_mut() = Some(Err(format!("failed to create window: {error}")));
                event_loop.exit();
                return;
            }
        };

        let window_handle = match window.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                *result.borrow_mut() = Some(Err(format!("failed to get window handle: {error}")));
                event_loop.exit();
                return;
            }
        };

        let rendering_context = match WindowRenderingContext::new(
            display_handle,
            window_handle,
            PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        ) {
            Ok(context) => Rc::new(context),
            Err(error) => {
                *result.borrow_mut() = Some(Err(format!(
                    "failed to create rendering context: {error:?}"
                )));
                event_loop.exit();
                return;
            }
        };

        if let Err(error) = rendering_context.make_current() {
            *result.borrow_mut() =
                Some(Err(format!("failed to make GL context current: {error:?}")));
            event_loop.exit();
            return;
        }

        let servo = ServoBuilder::default()
            .preferences(rendering_settings.servo_preferences())
            .event_loop_waker(Box::new(waker.clone()))
            .build();
        servo.setup_logging();

        let state = Rc::new(FormmaxState {
            window,
            servo,
            rendering_context,
            webview: RefCell::new(None),
            config: config.clone(),
            started_at: Instant::now(),
            phase: Cell::new(Phase::Load),
            phase_started_at: RefCell::new(Instant::now()),
            rendering_settings: rendering_settings.clone(),
            pending_native: Rc::new(RefCell::new(None)),
            pending_drive: Rc::new(RefCell::new(None)),
            pending_report: RefCell::new(None),
            native_input: RefCell::new(None),
            sent_inputs: RefCell::new(Vec::new()),
            handled_inputs: RefCell::new(Vec::new()),
            screenshots: RefCell::new(Vec::new()),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.config.url.clone())
            .hidpi_scale_factor(Scale::new(1.0))
            .delegate(state.clone())
            .build();
        *state.webview.borrow_mut() = Some(webview);

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
                    finish_err(
                        state,
                        event_loop,
                        "window closed before FORMMAX runner finished",
                    );
                    *self = Self::Finished;
                }
                WindowEvent::RedrawRequested => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.paint();
                        state.rendering_context.present();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    state.rendering_context.resize(new_size);
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.resize(new_size);
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

fn request_drive(state: &Rc<FormmaxState>, webview: &WebView) {
    *state.pending_drive.borrow_mut() = None;
    let pending = state.pending_drive.clone();
    webview.evaluate_javascript(DRIVE_JS, move |result| {
        *pending.borrow_mut() = Some(match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        });
    });
}

fn request_native_plan(state: &Rc<FormmaxState>, webview: &WebView) {
    *state.pending_native.borrow_mut() = None;
    let pending = state.pending_native.clone();
    webview.evaluate_javascript(NATIVE_PLAN_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn request_native_verify(state: &Rc<FormmaxState>, webview: &WebView) {
    *state.pending_native.borrow_mut() = None;
    let pending = state.pending_native.clone();
    webview.evaluate_javascript(NATIVE_VERIFY_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn js_result_to_string(
    result: std::result::Result<JSValue, servo::JavaScriptEvaluationError>,
) -> std::result::Result<String, String> {
    match result {
        Ok(JSValue::String(value)) => Ok(value),
        Ok(value) => Ok(format!("{value:?}")),
        Err(error) => Err(format!("{error:?}")),
    }
}

fn finish_drive(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
) -> Option<String> {
    pending
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn finish_probe(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
) -> Option<String> {
    pending
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn click_page_point(state: &Rc<FormmaxState>, webview: &WebView, x: f32, y: f32) {
    let page_point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(x, y));
    record_sent(
        state,
        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(page_point))),
        "mousemove",
    );
    record_sent(
        state,
        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
            MouseButtonAction::Down,
            MouseButton::Left,
            page_point,
        ))),
        "mousedown",
    );
    record_sent(
        state,
        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
            MouseButtonAction::Up,
            MouseButton::Left,
            page_point,
        ))),
        "mouseup",
    );
    state.window.request_redraw();
}

fn type_text(state: &Rc<FormmaxState>, webview: &WebView, text: &str) {
    for character in text.chars() {
        let key = Key::Character(character.to_string());
        record_sent(
            state,
            webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Down,
                key.clone(),
            ))),
            "keydown",
        );
        record_sent(
            state,
            webview.notify_input_event(InputEvent::Keyboard(KeyboardEvent::from_state_and_key(
                KeyState::Up,
                key,
            ))),
            "keyup",
        );
    }
    state.window.request_redraw();
}

fn record_sent(state: &Rc<FormmaxState>, id: InputEventId, label: &'static str) {
    state.sent_inputs.borrow_mut().push(SentInput { id, label });
}

fn probe_input_center(probe: &Value) -> Option<(f32, f32)> {
    let rect = probe.get("rect")?;
    let left = rect.get("left")?.as_f64()? as f32;
    let top = rect.get("top")?.as_f64()? as f32;
    let width = rect.get("width")?.as_f64()? as f32;
    let height = rect.get("height")?.as_f64()? as f32;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some((left + width / 2.0, top + height / 2.0))
}

fn append_native_input_outcomes(state: &Rc<FormmaxState>, native_input: &mut Value) {
    let keyboard_outcomes: Vec<HandledInput> = state
        .handled_inputs
        .borrow()
        .iter()
        .filter(|event| matches!(event.label, "keydown" | "keyup"))
        .cloned()
        .collect();
    native_input["fields_typed"] = json!(1);
    native_input["handled_keyboard_events"] = json!(keyboard_outcomes.len());
    native_input["dispatch_failed_keyboard_events"] = json!(
        keyboard_outcomes
            .iter()
            .filter(|event| event.dispatch_failed)
            .count()
    );
}

fn report_with_native_input(
    state: &Rc<FormmaxState>,
    mut report: FormmaxRunReport,
) -> FormmaxRunReport {
    let native_input = state
        .native_input
        .borrow()
        .clone()
        .unwrap_or_else(|| json!({ "enabled": false }));
    if native_input.get("enabled").and_then(Value::as_bool) == Some(true) {
        report.events.push(json!({
            "kind": "native_input_verified",
            "ts_ms": 0,
            "echo_values": false,
            "value_echoed": false,
            "row_id": native_input.get("row_id").cloned().unwrap_or(Value::Null),
            "field": native_input.get("field").cloned().unwrap_or(Value::Null),
            "fields_typed": native_input.get("fields_typed").cloned().unwrap_or(Value::Null),
            "value_matches": native_input.get("value_matches").cloned().unwrap_or(Value::Null),
            "keydown_events": native_input.get("keydown_events").cloned().unwrap_or(Value::Null),
            "input_events": native_input.get("input_events").cloned().unwrap_or(Value::Null),
            "keyup_events": native_input.get("keyup_events").cloned().unwrap_or(Value::Null),
            "dispatch_failed_keyboard_events": native_input
                .get("dispatch_failed_keyboard_events")
                .cloned()
                .unwrap_or(Value::Null)
        }));
        report.replay_events = report.events.len();
    }
    report.rendering_profile = state.rendering_settings.profile.name().to_string();
    report.servo_grid_enabled = state.rendering_settings.layout_grid_enabled;
    report.native_input = native_input;
    report
}

fn finish_ok(state: &Rc<FormmaxState>, event_loop: &ActiveEventLoop, report: FormmaxRunReport) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(report));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<FormmaxState>, event_loop: &ActiveEventLoop, message: impl Into<String>) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Err(message.into()));
    }
    event_loop.exit();
}

fn set_phase(state: &Rc<FormmaxState>, phase: Phase) {
    state.phase.set(phase);
    *state.phase_started_at.borrow_mut() = Instant::now();
}

fn save_artifact_screenshot(state: &Rc<FormmaxState>, webview: &WebView, filename: &str) {
    let Some(artifact_dir) = state.config.artifact_dir.as_ref() else {
        return;
    };
    webview.paint();
    let rect = DeviceIntRect::from_size(DeviceIntSize::new(
        WINDOW_WIDTH as i32,
        WINDOW_HEIGHT as i32,
    ));
    let path = artifact_dir.join(filename);
    match state.rendering_context.read_to_image(rect) {
        Some(image) => {
            if let Err(error) = image.save(&path) {
                eprintln!("failed to save {}: {error}", path.display());
            } else {
                state
                    .screenshots
                    .borrow_mut()
                    .push(path.display().to_string());
            }
        }
        None => eprintln!("failed to capture {}", path.display()),
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

const NATIVE_PLAN_JS: &str = r##"
(() => {
  const fixture = window.__FORMMAX_FIXTURE;
  if (!fixture) throw new Error("FORMMAX fixture API is missing");

  const row = (fixture.pages && fixture.pages[0] && fixture.pages[0][0]) || null;
  if (!row) throw new Error("FORMMAX native input row is missing");

  const field = "site_name";
  const control = document.getElementsByName(row.id + "_" + field)[0] || null;
  if (!control) throw new Error("FORMMAX native input control is missing");

  const state = {
    row_id: row.id,
    field,
    expected: String(row[field]),
    events: []
  };

  function record(event) {
    state.events.push({
      type: event.type,
      key: event.key || null,
      inputType: event.inputType || null,
      valueLength: control.value.length
    });
  }

  [
    "mousedown",
    "mouseup",
    "click",
    "focus",
    "keydown",
    "keypress",
    "beforeinput",
    "input",
    "keyup",
    "change"
  ].forEach((type) => control.addEventListener(type, record));

  window.__FORMMAX_NATIVE_INPUT = state;

  const rect = control.getBoundingClientRect();
  return JSON.stringify({
    ready: true,
    row_id: row.id,
    field,
    text: state.expected,
    rect: {
      left: rect.left,
      top: rect.top,
      width: rect.width,
      height: rect.height
    }
  });
})()
"##;

const NATIVE_VERIFY_JS: &str = r##"
(() => {
  const native = window.__FORMMAX_NATIVE_INPUT;
  if (!native) throw new Error("FORMMAX native input state is missing");

  const control = document.getElementsByName(native.row_id + "_" + native.field)[0] || null;
  if (!control) throw new Error("FORMMAX native input control disappeared");

  const counts = {};
  for (const event of native.events) {
    counts[event.type] = (counts[event.type] || 0) + 1;
  }

  return JSON.stringify({
    enabled: true,
    row_id: native.row_id,
    field: native.field,
    focused: document.activeElement === control,
    value_matches: control.value === native.expected,
    value_length: control.value.length,
    keydown_events: counts.keydown || 0,
    keypress_events: counts.keypress || 0,
    beforeinput_events: counts.beforeinput || 0,
    input_events: counts.input || 0,
    keyup_events: counts.keyup || 0
  });
})()
"##;

const DRIVE_JS: &str = r##"
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
    engine: "servo-formmax-fixture-runner-v0",
    rows: rows.length,
    pages: pages.length,
    policy: {
      block_sensitive: true,
      submit: "allow_local_fixture_only",
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
    engine: "servo-formmax-fixture-runner-v0",
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
