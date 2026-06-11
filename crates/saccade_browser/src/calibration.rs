use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use saccade_core::InputSpace;
use serde::{Deserialize, Serialize};
use servo::{
    CSSPixel, DeviceIntRect, DeviceIntSize, InputEvent, JSValue, LoadStatus, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, RenderingContext, Servo, ServoBuilder,
    WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const TIMEOUT: Duration = Duration::from_secs(20);
const FIRST_CLICK_DELAY: Duration = Duration::from_millis(300);
const CLICK_INTERVAL: Duration = Duration::from_millis(40);
const CLICK_SETTLE: Duration = Duration::from_millis(150);
const MAX_OK_ERR_CSS_PX: f32 = 0.5;

const POINTS: [CalibrationPoint; 5] = [
    CalibrationPoint { x: 100.0, y: 100.0 },
    CalibrationPoint {
        x: 1180.0,
        y: 100.0,
    },
    CalibrationPoint { x: 100.0, y: 700.0 },
    CalibrationPoint {
        x: 1180.0,
        y: 700.0,
    },
    CalibrationPoint { x: 640.0, y: 400.0 },
];

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationReport {
    pub input_space: InputSpace,
    pub max_err_css_px: f32,
    pub device_pixel_ratio: f32,
    pub attempts: Vec<CalibrationAttempt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationAttempt {
    pub input_space: InputSpace,
    pub max_err_css_px: f32,
    pub clicks: Vec<CalibrationClick>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationClick {
    pub intended_x: f32,
    pub intended_y: f32,
    pub received_x: f32,
    pub received_y: f32,
    pub err_css_px: f32,
}

pub fn calibrate_input(calibration_url: Url) -> Result<CalibrationReport> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = CalibrationApp::new(&event_loop, calibration_url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(report)) => Ok(report),
        Some(Err(message)) => bail!(message),
        None => bail!("calibration exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    AboutBlank,
    Calibration,
    RenderReadback,
    QueryDpr,
    ResetCss,
    ClickCss,
    ProbeCss,
    ResetDevice,
    ClickDevice,
    ProbeDevice,
}

#[derive(Debug, Clone, Copy)]
struct CalibrationPoint {
    x: f32,
    y: f32,
}

#[derive(Debug, Deserialize)]
struct Probe {
    dpr: f32,
    cal: Vec<ReceivedClick>,
}

#[derive(Debug, Deserialize)]
struct ReceivedClick {
    #[serde(rename = "clientX")]
    client_x: f32,
    #[serde(rename = "clientY")]
    client_y: f32,
}

struct CalibrationState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    target_url: Url,
    phase: Cell<Phase>,
    phase_started_at: Cell<Instant>,
    click_index: Cell<usize>,
    pending_js: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    dpr: Cell<Option<f32>>,
    attempts: RefCell<Vec<CalibrationAttempt>>,
    result: Rc<RefCell<Option<std::result::Result<CalibrationReport, String>>>>,
}

impl WebViewDelegate for CalibrationState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum CalibrationApp {
    Initial {
        waker: Waker,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<CalibrationReport, String>>>>,
        started_at: Instant,
    },
    Running {
        state: Rc<CalibrationState>,
        started_at: Instant,
    },
    Finished,
}

impl CalibrationApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<CalibrationReport, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            target_url,
            result,
            started_at: Instant::now(),
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let (state, started_at) = match self {
            Self::Running { state, started_at } => (state.clone(), *started_at),
            _ => return,
        };

        if started_at.elapsed() > TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("calibration timed out after {TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

        match state.phase.get() {
            Phase::AboutBlank if webview.load_status() == LoadStatus::Complete => {
                advance(&state, Phase::Calibration);
                webview.load(state.target_url.clone());
            }
            Phase::Calibration if webview.load_status() == LoadStatus::Complete => {
                advance(&state, Phase::RenderReadback);
                webview.paint();
                state.window.request_redraw();
            }
            Phase::RenderReadback => {
                webview.paint();
                let rect = DeviceIntRect::from_size(DeviceIntSize::new(
                    WINDOW_WIDTH as i32,
                    WINDOW_HEIGHT as i32,
                ));
                if state.rendering_context.read_to_image(rect).is_some() {
                    advance(&state, Phase::QueryDpr);
                    request_js(&state, &webview, CAL_PROBE_JS);
                } else {
                    state.window.request_redraw();
                }
            }
            Phase::QueryDpr => {
                if let Some(raw) = finish_js_if_ready(&state) {
                    match parse_probe(&raw) {
                        Ok(probe) => {
                            state.dpr.set(Some(probe.dpr));
                            advance(&state, Phase::ResetCss);
                            request_js(&state, &webview, RESET_JS);
                        }
                        Err(error) => {
                            finish_err(&state, event_loop, error);
                            *self = Self::Finished;
                            return;
                        }
                    }
                }
            }
            Phase::ResetCss => {
                if finish_js_if_ready(&state).is_some() {
                    state.click_index.set(0);
                    advance(&state, Phase::ClickCss);
                }
            }
            Phase::ClickCss => {
                click_or_probe(
                    &state,
                    &webview,
                    InputSpace::CssLogical,
                    Phase::ProbeCss,
                    CAL_PROBE_JS,
                );
            }
            Phase::ProbeCss => {
                if let Some(raw) = finish_js_if_ready(&state) {
                    match evaluate_attempt(InputSpace::CssLogical, &raw) {
                        Ok(attempt) if attempt.max_err_css_px <= MAX_OK_ERR_CSS_PX => {
                            let report = CalibrationReport {
                                input_space: InputSpace::CssLogical,
                                max_err_css_px: attempt.max_err_css_px,
                                device_pixel_ratio: state.dpr.get().unwrap_or(1.0),
                                attempts: vec![attempt],
                            };
                            finish_ok(&state, event_loop, report);
                            *self = Self::Finished;
                            return;
                        }
                        Ok(attempt) => {
                            state.attempts.borrow_mut().push(attempt);
                            advance(&state, Phase::ResetDevice);
                            request_js(&state, &webview, RESET_JS);
                        }
                        Err(error) => {
                            finish_err(&state, event_loop, error);
                            *self = Self::Finished;
                            return;
                        }
                    }
                }
            }
            Phase::ResetDevice => {
                if finish_js_if_ready(&state).is_some() {
                    state.click_index.set(0);
                    advance(&state, Phase::ClickDevice);
                }
            }
            Phase::ClickDevice => {
                click_or_probe(
                    &state,
                    &webview,
                    InputSpace::DevicePhysical,
                    Phase::ProbeDevice,
                    CAL_PROBE_JS,
                );
            }
            Phase::ProbeDevice => {
                if let Some(raw) = finish_js_if_ready(&state) {
                    match evaluate_attempt(InputSpace::DevicePhysical, &raw) {
                        Ok(attempt) if attempt.max_err_css_px <= MAX_OK_ERR_CSS_PX => {
                            let mut attempts = std::mem::take(&mut *state.attempts.borrow_mut());
                            let max_err = attempt.max_err_css_px;
                            attempts.push(attempt);
                            let report = CalibrationReport {
                                input_space: InputSpace::DevicePhysical,
                                max_err_css_px: max_err,
                                device_pixel_ratio: state.dpr.get().unwrap_or(1.0),
                                attempts,
                            };
                            finish_ok(&state, event_loop, report);
                            *self = Self::Finished;
                            return;
                        }
                        Ok(attempt) => {
                            state.attempts.borrow_mut().push(attempt);
                            finish_err(
                                &state,
                                event_loop,
                                failure_summary(&state.attempts.borrow()),
                            );
                            *self = Self::Finished;
                            return;
                        }
                        Err(error) => {
                            finish_err(&state, event_loop, error);
                            *self = Self::Finished;
                            return;
                        }
                    }
                }
            }
            _ => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for CalibrationApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            target_url,
            result,
            started_at,
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
                .with_title("Saccade M4 Calibration")
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

        let state = Rc::new(CalibrationState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            target_url: target_url.clone(),
            phase: Cell::new(Phase::AboutBlank),
            phase_started_at: Cell::new(Instant::now()),
            click_index: Cell::new(0),
            pending_js: Rc::new(RefCell::new(None)),
            dpr: Cell::new(None),
            attempts: RefCell::new(Vec::new()),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(Url::parse("about:blank").expect("static URL is valid"))
            .hidpi_scale_factor(Scale::new(1.0))
            .delegate(state.clone())
            .build();
        state.webviews.borrow_mut().push(webview);

        *self = Self::Running {
            state,
            started_at: *started_at,
        };
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: WakerEvent) {
        if let Self::Running { state, .. } = self {
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
        let state = match self {
            Self::Running { state, .. } => state.clone(),
            _ => {
                self.after_spin(event_loop);
                return;
            }
        };

        state.servo.spin_event_loop();
        match event {
            WindowEvent::CloseRequested => {
                finish_err(
                    &state,
                    event_loop,
                    "window closed before calibration finished".into(),
                );
                *self = Self::Finished;
                return;
            }
            WindowEvent::RedrawRequested => {
                if let Some(webview) = state.webviews.borrow().last() {
                    webview.paint();
                    state.rendering_context.present();
                }
            }
            WindowEvent::Resized(new_size) => {
                state.rendering_context.resize(new_size);
                if let Some(webview) = state.webviews.borrow().last() {
                    webview.resize(new_size);
                }
            }
            _ => {}
        }
        self.after_spin(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.after_spin(event_loop);
    }
}

fn advance(state: &Rc<CalibrationState>, phase: Phase) {
    state.phase.set(phase);
    state.phase_started_at.set(Instant::now());
}

fn finish_ok(
    state: &Rc<CalibrationState>,
    event_loop: &ActiveEventLoop,
    report: CalibrationReport,
) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(report));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<CalibrationState>, event_loop: &ActiveEventLoop, message: String) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Err(message));
    }
    event_loop.exit();
}

fn request_js(state: &Rc<CalibrationState>, webview: &WebView, script: &'static str) {
    *state.pending_js.borrow_mut() = None;
    let pending = state.pending_js.clone();
    webview.evaluate_javascript(script, move |result| {
        let value = match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        };
        *pending.borrow_mut() = Some(value);
    });
}

fn finish_js_if_ready(state: &Rc<CalibrationState>) -> Option<String> {
    let result = state.pending_js.borrow_mut().take()?;
    Some(result.unwrap_or_else(|error| error))
}

fn click_or_probe(
    state: &Rc<CalibrationState>,
    webview: &WebView,
    input_space: InputSpace,
    probe_phase: Phase,
    probe_js: &'static str,
) {
    let index = state.click_index.get();
    if index < POINTS.len() {
        let required_delay = if index == 0 {
            FIRST_CLICK_DELAY
        } else {
            CLICK_INTERVAL
        };
        if state.phase_started_at.get().elapsed() >= required_delay {
            click_point(state, webview, input_space, POINTS[index]);
            state.servo.spin_event_loop();
            state.click_index.set(index + 1);
            state.phase_started_at.set(Instant::now());
        }
        return;
    }

    if state.phase_started_at.get().elapsed() >= CLICK_SETTLE {
        advance(state, probe_phase);
        request_js(state, webview, probe_js);
    }
}

fn click_point(
    state: &Rc<CalibrationState>,
    webview: &WebView,
    input_space: InputSpace,
    point: CalibrationPoint,
) {
    let dpr = state.dpr.get().unwrap_or(1.0);
    let scale = match input_space {
        InputSpace::CssLogical => 1.0,
        InputSpace::DevicePhysical => dpr,
    };
    let page_point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(
        point.x * scale,
        point.y * scale,
    ));
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

fn parse_probe(raw: &str) -> std::result::Result<Probe, String> {
    serde_json::from_str(raw)
        .map_err(|error| format!("failed to parse calibration probe: {error}; raw={raw:?}"))
}

fn evaluate_attempt(
    input_space: InputSpace,
    raw: &str,
) -> std::result::Result<CalibrationAttempt, String> {
    let probe = parse_probe(raw)?;
    if probe.cal.len() != POINTS.len() {
        return Err(format!(
            "{input_space:?} produced {} clicks, expected {}: {raw}",
            probe.cal.len(),
            POINTS.len()
        ));
    }

    let clicks = POINTS
        .iter()
        .zip(probe.cal.iter())
        .map(|(intended, received)| {
            let dx = received.client_x - intended.x;
            let dy = received.client_y - intended.y;
            CalibrationClick {
                intended_x: intended.x,
                intended_y: intended.y,
                received_x: received.client_x,
                received_y: received.client_y,
                err_css_px: (dx * dx + dy * dy).sqrt(),
            }
        })
        .collect::<Vec<_>>();
    let max_err_css_px = clicks
        .iter()
        .map(|click| click.err_css_px)
        .fold(0.0, f32::max);

    Ok(CalibrationAttempt {
        input_space,
        max_err_css_px,
        clicks,
    })
}

fn failure_summary(attempts: &[CalibrationAttempt]) -> String {
    let mut summary = String::from("calibration failed; attempts:");
    for attempt in attempts {
        summary.push_str(&format!(
            " {:?} max_err_css_px={:.3}",
            attempt.input_space, attempt.max_err_css_px
        ));
        for click in &attempt.clicks {
            summary.push_str(&format!(
                " [intended=({:.1},{:.1}) received=({:.1},{:.1}) err={:.3}]",
                click.intended_x,
                click.intended_y,
                click.received_x,
                click.received_y,
                click.err_css_px
            ));
        }
    }
    summary
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

const RESET_JS: &str = r#"
(() => {
  window.__resetCal();
  return JSON.stringify({ok:true});
})()
"#;

const CAL_PROBE_JS: &str = r#"
(() => JSON.stringify({
  dpr: window.devicePixelRatio,
  cal: window.__cal || []
}))()
"#;
