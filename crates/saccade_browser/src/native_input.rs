use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use serde_json::Value;
use servo::{
    CSSPixel, EmbedderControl, InputEvent, InputEventId, InputEventResult, JSValue, Key, KeyState,
    KeyboardEvent, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    RenderingContext, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate, WebViewPoint,
    WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 500;
const EXPECTED_TEXT: &str = "saccade42";
const EXPECTED_SELECT_VALUE: &str = "gamma";
const EXPECTED_SELECT_INDEX: usize = 2;
const INPUT_TIMEOUT: Duration = Duration::from_secs(20);
const FOCUS_SETTLE: Duration = Duration::from_millis(140);
const VALUE_SETTLE: Duration = Duration::from_millis(180);
const RETRY_SETTLE: Duration = Duration::from_millis(80);

#[derive(Debug, Clone)]
pub struct NativeInputProfile {
    pub focused: bool,
    pub value: String,
    pub expected_value: String,
    pub keydown_events: usize,
    pub keypress_events: usize,
    pub beforeinput_events: usize,
    pub input_events: usize,
    pub keyup_events: usize,
    pub handled_keyboard_events: usize,
    pub consumed_keyboard_events: usize,
    pub dispatch_failed_keyboard_events: usize,
    pub select_focused: bool,
    pub select_value: String,
    pub expected_select_value: String,
    pub select_input_events: usize,
    pub select_change_events: usize,
    pub select_controls_shown: usize,
    pub select_options_seen: usize,
    pub select_requested_index: usize,
}

pub fn selftest_native_input(url: Url) -> Result<NativeInputProfile> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = NativeInputApp::new(&event_loop, url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(profile)) => Ok(profile),
        Some(Err(message)) => bail!(message),
        None => bail!("native input selftest exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitPage,
    WaitReadyProbe,
    WaitFocus,
    WaitValue,
    WaitSelectReadyProbe,
    WaitSelectValue,
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
    consumed: bool,
    dispatch_failed: bool,
}

struct NativeInputState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    url: Url,
    webview: RefCell<Option<WebView>>,
    started_at: Instant,
    phase: Cell<Phase>,
    phase_started_at: RefCell<Instant>,
    probe_requested: Cell<bool>,
    sent_inputs: RefCell<Vec<SentInput>>,
    handled_inputs: RefCell<Vec<HandledInput>>,
    text_profile: RefCell<Option<NativeInputProfile>>,
    select_controls_shown: Cell<usize>,
    select_options_seen: Cell<usize>,
    select_requested_index: Cell<usize>,
    pending_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    result: Rc<RefCell<Option<std::result::Result<NativeInputProfile, String>>>>,
}

impl WebViewDelegate for NativeInputState {
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
            consumed: result.contains(InputEventResult::Consumed),
            dispatch_failed: result.contains(InputEventResult::DispatchFailed),
        });
    }

    fn show_embedder_control(&self, _webview: WebView, embedder_control: EmbedderControl) {
        if let EmbedderControl::SelectElement(mut select) = embedder_control {
            self.select_controls_shown
                .set(self.select_controls_shown.get() + 1);
            self.select_options_seen.set(select.options().len());
            self.select_requested_index.set(EXPECTED_SELECT_INDEX);
            select.select(vec![EXPECTED_SELECT_INDEX]);
            select.submit();
        }
    }
}

enum NativeInputApp {
    Initial {
        waker: Waker,
        url: Url,
        result: Rc<RefCell<Option<std::result::Result<NativeInputProfile, String>>>>,
    },
    Running {
        state: Rc<NativeInputState>,
    },
    Finished,
}

impl NativeInputApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        url: Url,
        result: Rc<RefCell<Option<std::result::Result<NativeInputProfile, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            url,
            result,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        if state.started_at.elapsed() > INPUT_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("native input selftest timed out after {INPUT_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webview.borrow().as_ref().cloned() else {
            return;
        };

        match state.phase.get() {
            Phase::WaitPage => {
                if webview.load_status() == LoadStatus::Complete
                    && webview.page_title().as_deref() == Some("Native Input")
                {
                    request_text_probe(&state, &webview);
                    set_phase(&state, Phase::WaitReadyProbe);
                }
            }
            Phase::WaitReadyProbe => {
                let Some(text) = finish_probe(&state.pending_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&text) else {
                    finish_err(&state, event_loop, format!("invalid ready probe: {text}"));
                    *self = Self::Finished;
                    return;
                };

                let Some((x, y)) = probe_input_center(&probe) else {
                    finish_err(&state, event_loop, format!("missing input rect: {probe}"));
                    *self = Self::Finished;
                    return;
                };

                click_probe_input(&state, &webview, x, y);
                set_phase(&state, Phase::WaitFocus);
            }
            Phase::WaitFocus => {
                if state.phase_started_at.borrow().elapsed() < FOCUS_SETTLE {
                    return;
                }

                if !state.probe_requested.get() {
                    request_text_probe(&state, &webview);
                    return;
                }

                let Some(text) = finish_probe(&state.pending_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&text) else {
                    finish_err(&state, event_loop, format!("invalid focus probe: {text}"));
                    *self = Self::Finished;
                    return;
                };

                if probe.get("focused").and_then(Value::as_bool) == Some(true) {
                    type_text(&state, &webview, EXPECTED_TEXT);
                    set_phase(&state, Phase::WaitValue);
                    return;
                }

                if state.phase_started_at.borrow().elapsed() > Duration::from_secs(3) {
                    finish_err(
                        &state,
                        event_loop,
                        format!("input did not receive focus: {probe}"),
                    );
                    *self = Self::Finished;
                    return;
                }

                state.probe_requested.set(false);
                *state.phase_started_at.borrow_mut() = Instant::now() - FOCUS_SETTLE + RETRY_SETTLE;
            }
            Phase::WaitValue => {
                if state.phase_started_at.borrow().elapsed() < VALUE_SETTLE {
                    return;
                }

                if !state.probe_requested.get() {
                    request_text_probe(&state, &webview);
                    return;
                }

                let Some(text) = finish_probe(&state.pending_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&text) else {
                    finish_err(&state, event_loop, format!("invalid value probe: {text}"));
                    *self = Self::Finished;
                    return;
                };

                let profile = profile_from_probe(&state, &probe);
                if profile.value == EXPECTED_TEXT
                    && profile.focused
                    && profile.keydown_events >= EXPECTED_TEXT.len()
                    && profile.input_events >= EXPECTED_TEXT.len()
                    && profile.keyup_events >= EXPECTED_TEXT.len()
                    && profile.dispatch_failed_keyboard_events == 0
                {
                    *state.text_profile.borrow_mut() = Some(profile);
                    request_select_probe(&state, &webview);
                    set_phase(&state, Phase::WaitSelectReadyProbe);
                    return;
                }

                if state.phase_started_at.borrow().elapsed() > Duration::from_secs(3) {
                    finish_err(
                        &state,
                        event_loop,
                        format!("native keyboard input did not settle: {profile:?}"),
                    );
                    *self = Self::Finished;
                    return;
                }

                state.probe_requested.set(false);
                *state.phase_started_at.borrow_mut() = Instant::now() - VALUE_SETTLE + RETRY_SETTLE;
            }
            Phase::WaitSelectReadyProbe => {
                let Some(text) = finish_probe(&state.pending_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&text) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("invalid select ready probe: {text}"),
                    );
                    *self = Self::Finished;
                    return;
                };

                let Some((x, y)) = probe_input_center(&probe) else {
                    finish_err(&state, event_loop, format!("missing select rect: {probe}"));
                    *self = Self::Finished;
                    return;
                };

                click_probe_input(&state, &webview, x, y);
                set_phase(&state, Phase::WaitSelectValue);
            }
            Phase::WaitSelectValue => {
                if state.phase_started_at.borrow().elapsed() < VALUE_SETTLE {
                    return;
                }

                if !state.probe_requested.get() {
                    request_select_probe(&state, &webview);
                    return;
                }

                let Some(text) = finish_probe(&state.pending_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&text) else {
                    finish_err(
                        &state,
                        event_loop,
                        format!("invalid select value probe: {text}"),
                    );
                    *self = Self::Finished;
                    return;
                };

                let profile = profile_with_select_probe(&state, &probe);
                if profile.select_value == EXPECTED_SELECT_VALUE
                    && profile.select_focused
                    && profile.select_controls_shown >= 1
                    && profile.select_options_seen >= 3
                    && profile.select_input_events >= 1
                    && profile.select_change_events >= 1
                {
                    finish_ok(&state, event_loop, profile);
                    state.phase.set(Phase::Done);
                    *self = Self::Finished;
                    return;
                }

                if state.phase_started_at.borrow().elapsed() > Duration::from_secs(3) {
                    finish_err(
                        &state,
                        event_loop,
                        format!("native select input did not settle: {profile:?}"),
                    );
                    *self = Self::Finished;
                    return;
                }

                state.probe_requested.set(false);
                *state.phase_started_at.borrow_mut() = Instant::now() - VALUE_SETTLE + RETRY_SETTLE;
            }
            Phase::Done => {}
        }
    }
}

impl ApplicationHandler<WakerEvent> for NativeInputApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial { waker, url, result } = self else {
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
                .with_title("Saccade Native Input")
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
            .event_loop_waker(Box::new(waker.clone()))
            .build();
        servo.setup_logging();

        let state = Rc::new(NativeInputState {
            window,
            servo,
            rendering_context,
            url: url.clone(),
            webview: RefCell::new(None),
            started_at: Instant::now(),
            phase: Cell::new(Phase::WaitPage),
            phase_started_at: RefCell::new(Instant::now()),
            probe_requested: Cell::new(false),
            sent_inputs: RefCell::new(Vec::new()),
            handled_inputs: RefCell::new(Vec::new()),
            text_profile: RefCell::new(None),
            select_controls_shown: Cell::new(0),
            select_options_seen: Cell::new(0),
            select_requested_index: Cell::new(usize::MAX),
            pending_probe: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.url.clone())
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
                    finish_err(&state, event_loop, "window closed before selftest finished");
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

fn click_probe_input(state: &Rc<NativeInputState>, webview: &WebView, x: f32, y: f32) {
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

fn type_text(state: &Rc<NativeInputState>, webview: &WebView, text: &str) {
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

fn record_sent(state: &Rc<NativeInputState>, id: InputEventId, label: &'static str) {
    state.sent_inputs.borrow_mut().push(SentInput { id, label });
}

fn request_text_probe(state: &Rc<NativeInputState>, webview: &WebView) {
    *state.pending_probe.borrow_mut() = None;
    state.probe_requested.set(true);
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(TEXT_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn request_select_probe(state: &Rc<NativeInputState>, webview: &WebView) {
    *state.pending_probe.borrow_mut() = None;
    state.probe_requested.set(true);
    let pending = state.pending_probe.clone();
    webview.evaluate_javascript(SELECT_PROBE_JS, move |result| {
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

fn finish_probe(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
) -> Option<String> {
    pending
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn parse_probe(text: &str) -> std::result::Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

fn profile_from_probe(state: &Rc<NativeInputState>, probe: &Value) -> NativeInputProfile {
    let keyboard_outcomes: Vec<HandledInput> = state
        .handled_inputs
        .borrow()
        .iter()
        .filter(|event| matches!(event.label, "keydown" | "keyup"))
        .cloned()
        .collect();
    NativeInputProfile {
        focused: probe
            .get("focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        value: probe
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        expected_value: EXPECTED_TEXT.to_string(),
        keydown_events: count_probe_event(probe, "keydown"),
        keypress_events: count_probe_event(probe, "keypress"),
        beforeinput_events: count_probe_event(probe, "beforeinput"),
        input_events: count_probe_event(probe, "input"),
        keyup_events: count_probe_event(probe, "keyup"),
        handled_keyboard_events: keyboard_outcomes.len(),
        consumed_keyboard_events: keyboard_outcomes
            .iter()
            .filter(|event| event.consumed)
            .count(),
        dispatch_failed_keyboard_events: keyboard_outcomes
            .iter()
            .filter(|event| event.dispatch_failed)
            .count(),
        select_focused: false,
        select_value: String::new(),
        expected_select_value: EXPECTED_SELECT_VALUE.to_string(),
        select_input_events: 0,
        select_change_events: 0,
        select_controls_shown: state.select_controls_shown.get(),
        select_options_seen: state.select_options_seen.get(),
        select_requested_index: state.select_requested_index.get(),
    }
}

fn profile_with_select_probe(state: &Rc<NativeInputState>, probe: &Value) -> NativeInputProfile {
    let mut profile = state
        .text_profile
        .borrow()
        .clone()
        .unwrap_or_else(|| profile_from_probe(state, probe));
    profile.select_focused = probe
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    profile.select_value = probe
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    profile.expected_select_value = EXPECTED_SELECT_VALUE.to_string();
    profile.select_input_events = count_probe_event(probe, "input");
    profile.select_change_events = count_probe_event(probe, "change");
    profile.select_controls_shown = state.select_controls_shown.get();
    profile.select_options_seen = state.select_options_seen.get();
    profile.select_requested_index = state.select_requested_index.get();
    profile
}

fn count_probe_event(probe: &Value, name: &str) -> usize {
    probe
        .get("counts")
        .and_then(|counts| counts.get(name))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize
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

fn finish_ok(
    state: &Rc<NativeInputState>,
    event_loop: &ActiveEventLoop,
    profile: NativeInputProfile,
) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(profile));
    }
    event_loop.exit();
}

fn finish_err(
    state: &Rc<NativeInputState>,
    event_loop: &ActiveEventLoop,
    message: impl Into<String>,
) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Err(message.into()));
    }
    event_loop.exit();
}

fn set_phase(state: &Rc<NativeInputState>, phase: Phase) {
    state.phase.set(phase);
    *state.phase_started_at.borrow_mut() = Instant::now();
    state.probe_requested.set(false);
}

const TEXT_PROBE_JS: &str = r#"
(() => {
  if (!window.__NATIVE_INPUT_PROBE) {
    return JSON.stringify({ ready: false });
  }
  return window.__NATIVE_INPUT_PROBE();
})()
"#;

const SELECT_PROBE_JS: &str = r#"
(() => {
  if (!window.__NATIVE_SELECT_PROBE) {
    return JSON.stringify({ ready: false });
  }
  return window.__NATIVE_SELECT_PROBE();
})()
"#;

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
