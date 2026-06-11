use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use saccade_core::{
    CssPoint, CssRect, DomRectObs, FrameObservation, InputSpace, MotorAction, PixelRegion,
    ViewportInfo,
};
use saccade_detect::{DetectConfig, DetectionPipeline};
use saccade_motor::MotorController;
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
const SAFE_TOP_CSS: f32 = 100.0;
const TOTAL_TIMEOUT: Duration = Duration::from_secs(45);
const PAGE_TIMEOUT: Duration = Duration::from_secs(6);
const BACKGROUND_DELAY: Duration = Duration::from_millis(160);
const FRAME_INTERVAL: Duration = Duration::from_millis(28);
const TRUTH_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone)]
pub struct SelftestPagesReport {
    pub outcomes: Vec<SelftestPageOutcome>,
}

#[derive(Debug, Clone)]
pub struct SelftestPageOutcome {
    pub name: String,
    pub passed: bool,
    pub truth: String,
    pub clicks_sent: u32,
    pub detail: String,
}

pub fn selftest_pages(base_url: Url, input_space: InputSpace) -> Result<SelftestPagesReport> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = SelftestApp::new(&event_loop, base_url, input_space, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(report)) => Ok(report),
        Some(Err(message)) => bail!(message),
        None => bail!("selftest pages exited without a result"),
    }
}

#[derive(Debug, Clone, Copy)]
struct PageSpec {
    name: &'static str,
    path: &'static str,
    required_hits: u32,
    overlay_safe: bool,
}

const PAGES: [PageSpec; 7] = [
    PageSpec {
        name: "dom_targets",
        path: "dom_targets.html",
        required_hits: 3,
        overlay_safe: false,
    },
    PageSpec {
        name: "svg_targets",
        path: "svg_targets.html",
        required_hits: 3,
        overlay_safe: false,
    },
    PageSpec {
        name: "canvas_arc_targets",
        path: "canvas_arc_targets.html",
        required_hits: 3,
        overlay_safe: false,
    },
    PageSpec {
        name: "canvas_sprite_targets",
        path: "canvas_sprite_targets.html",
        required_hits: 3,
        overlay_safe: false,
    },
    PageSpec {
        name: "overlay_interference",
        path: "overlay_interference.html",
        required_hits: 0,
        overlay_safe: true,
    },
    PageSpec {
        name: "high_dpi_grid",
        path: "high_dpi_grid.html",
        required_hits: 9,
        overlay_safe: false,
    },
    PageSpec {
        name: "webgl_targets",
        path: "webgl_targets.html",
        required_hits: 3,
        overlay_safe: false,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    Background,
    Run,
    ProbeTruth,
}

struct PageRuntime {
    index: usize,
    phase: Phase,
    page_started_at: Instant,
    phase_started_at: Instant,
    last_frame_at: Instant,
    last_truth_at: Instant,
    frame_id: u64,
    clicks_sent: u32,
    last_truth: String,
    click_points: Vec<CssPoint>,
    pipeline: DetectionPipeline,
    motor: MotorController,
}

impl PageRuntime {
    fn new(index: usize) -> Self {
        let now = Instant::now();
        Self {
            index,
            phase: Phase::Load,
            page_started_at: now,
            phase_started_at: now,
            last_frame_at: now - FRAME_INTERVAL,
            last_truth_at: now - TRUTH_INTERVAL,
            frame_id: 0,
            clicks_sent: 0,
            last_truth: String::new(),
            click_points: Vec::new(),
            pipeline: DetectionPipeline::default(),
            motor: MotorController::default(),
        }
    }

    fn advance(&mut self, phase: Phase) {
        self.phase = phase;
        self.phase_started_at = Instant::now();
    }
}

struct SelftestState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    base_url: Url,
    input_space: InputSpace,
    run_started_at: Instant,
    runtime: RefCell<PageRuntime>,
    pending_truth: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_rects: Rc<RefCell<Option<(usize, std::result::Result<String, String>)>>>,
    rects_in_flight: Rc<RefCell<Option<usize>>>,
    last_rects: RefCell<Vec<DomRectObs>>,
    outcomes: RefCell<Vec<SelftestPageOutcome>>,
    result: Rc<RefCell<Option<std::result::Result<SelftestPagesReport, String>>>>,
}

impl WebViewDelegate for SelftestState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum SelftestApp {
    Initial {
        waker: Waker,
        base_url: Url,
        input_space: InputSpace,
        result: Rc<RefCell<Option<std::result::Result<SelftestPagesReport, String>>>>,
    },
    Running {
        state: Rc<SelftestState>,
    },
    Finished,
}

impl SelftestApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        base_url: Url,
        input_space: InputSpace,
        result: Rc<RefCell<Option<std::result::Result<SelftestPagesReport, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            base_url,
            input_space,
            result,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        if state.run_started_at.elapsed() > TOTAL_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("selftest timed out after {TOTAL_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

        let index = state.runtime.borrow().index;
        if index >= PAGES.len() {
            let report = SelftestPagesReport {
                outcomes: std::mem::take(&mut *state.outcomes.borrow_mut()),
            };
            finish_ok(&state, event_loop, report);
            *self = Self::Finished;
            return;
        }
        let spec = PAGES[index];

        if state.runtime.borrow().page_started_at.elapsed() > PAGE_TIMEOUT {
            let truth = state.runtime.borrow().last_truth.clone();
            record_outcome(
                &state,
                spec,
                false,
                if truth.is_empty() {
                    "timeout".into()
                } else {
                    truth
                },
                "page timed out".into(),
            );
            load_next_page(&state, &webview);
            return;
        }

        let phase = state.runtime.borrow().phase;
        match phase {
            Phase::Load if webview.load_status() == LoadStatus::Complete => {
                state.runtime.borrow_mut().advance(Phase::Background);
            }
            Phase::Background
                if state.runtime.borrow().phase_started_at.elapsed() >= BACKGROUND_DELAY =>
            {
                if let Some(obs) = capture(&state, &webview) {
                    let cfg = DetectConfig::default();
                    let _ = state.runtime.borrow_mut().pipeline.on_frame(&obs, &cfg);
                    start_page(&webview);
                    state.runtime.borrow_mut().advance(Phase::Run);
                }
            }
            Phase::Run => {
                if spec.overlay_safe {
                    if state.runtime.borrow().phase_started_at.elapsed()
                        >= Duration::from_millis(600)
                    {
                        request_truth(&state, &webview);
                        state.runtime.borrow_mut().advance(Phase::ProbeTruth);
                    }
                } else {
                    maybe_run_frame(&state, &webview);
                    if state.runtime.borrow().last_truth_at.elapsed() >= TRUTH_INTERVAL {
                        request_truth(&state, &webview);
                        state.runtime.borrow_mut().advance(Phase::ProbeTruth);
                    }
                }
            }
            Phase::ProbeTruth => {
                if let Some(truth) = finish_truth_if_ready(&state) {
                    state.runtime.borrow_mut().last_truth = truth.clone();
                    let parsed = parse_truth(&truth);
                    if page_passed(spec, &parsed) {
                        record_outcome(
                            &state,
                            spec,
                            true,
                            truth,
                            format!("clicks_sent={}", state.runtime.borrow().clicks_sent),
                        );
                        load_next_page(&state, &webview);
                    } else if state.runtime.borrow().page_started_at.elapsed() > PAGE_TIMEOUT {
                        record_outcome(
                            &state,
                            spec,
                            false,
                            truth,
                            format!("did not satisfy pass criteria for {}", spec.name),
                        );
                        load_next_page(&state, &webview);
                    } else {
                        state.runtime.borrow_mut().last_truth_at = Instant::now();
                        state.runtime.borrow_mut().advance(Phase::Run);
                    }
                }
            }
            _ => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for SelftestApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            base_url,
            input_space,
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
                .with_title("Saccade M5 Selftest Pages")
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

        let state = Rc::new(SelftestState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            base_url: base_url.clone(),
            input_space: *input_space,
            run_started_at: Instant::now(),
            runtime: RefCell::new(PageRuntime::new(0)),
            pending_truth: Rc::new(RefCell::new(None)),
            pending_rects: Rc::new(RefCell::new(None)),
            rects_in_flight: Rc::new(RefCell::new(None)),
            last_rects: RefCell::new(Vec::new()),
            outcomes: RefCell::new(Vec::new()),
            result: result.clone(),
        });

        let first_url = state
            .base_url
            .join(PAGES[0].path)
            .expect("page path is valid");
        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(first_url)
            .hidpi_scale_factor(Scale::new(1.0))
            .delegate(state.clone())
            .build();
        state.webviews.borrow_mut().push(webview);

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
        let state = match self {
            Self::Running { state } => state.clone(),
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
                    "window closed before selftest finished".into(),
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

fn maybe_run_frame(state: &Rc<SelftestState>, webview: &WebView) {
    if state.runtime.borrow().last_frame_at.elapsed() < FRAME_INTERVAL {
        return;
    }
    request_rects_if_idle(state, webview);
    let Some(obs) = capture(state, webview) else {
        return;
    };
    let now_ns = obs.t_readback_ns + 1_000_000;
    let cfg = DetectConfig::default();
    let mut report = state.runtime.borrow_mut().pipeline.on_frame(&obs, &cfg);
    let safe_area = CssRect {
        x: 0.0,
        y: SAFE_TOP_CSS,
        w: WINDOW_WIDTH as f32,
        h: WINDOW_HEIGHT as f32 - SAFE_TOP_CSS,
    };
    report
        .targets
        .retain(|target| safe_area.contains(target.center_css));
    report.game_area_css = safe_area;
    let action = state.runtime.borrow_mut().motor.on_frame(&report, now_ns);
    if let MotorAction::Click {
        target_id,
        point_css,
        ..
    } = action
    {
        click(state, webview, point_css);
        state.runtime.borrow_mut().pipeline.mark_clicked(target_id);
        state.runtime.borrow_mut().clicks_sent += 1;
        state.runtime.borrow_mut().click_points.push(point_css);
    }
    state.runtime.borrow_mut().last_frame_at = Instant::now();
}

fn capture(state: &Rc<SelftestState>, webview: &WebView) -> Option<FrameObservation> {
    finish_rects_if_ready(state);
    let run_started_at = state.run_started_at;
    webview.paint();
    let frame_id = {
        let mut runtime = state.runtime.borrow_mut();
        runtime.frame_id += 1;
        runtime.frame_id
    };
    let t_paint_ns = run_started_at.elapsed().as_nanos() as u64;
    let rect = DeviceIntRect::from_size(DeviceIntSize::new(
        WINDOW_WIDTH as i32,
        WINDOW_HEIGHT as i32,
    ));
    let image = state.rendering_context.read_to_image(rect)?;
    let t_readback_ns = run_started_at.elapsed().as_nanos() as u64;
    let w = image.width();
    let h = image.height();
    let rects = state.last_rects.borrow().clone();
    Some(FrameObservation {
        frame_id,
        t_paint_ns,
        t_readback_ns,
        viewport: ViewportInfo {
            width_css: WINDOW_WIDTH as f32,
            height_css: WINDOW_HEIGHT as f32,
            device_scale_factor: 1.0,
            page_zoom: 1.0,
        },
        game_area_css: CssRect {
            x: 0.0,
            y: 0.0,
            w: WINDOW_WIDTH as f32,
            h: WINDOW_HEIGHT as f32,
        },
        pixels: PixelRegion {
            w,
            h,
            rgba: Arc::new(image.into_raw()),
        },
        dom_rects: if rects.is_empty() { None } else { Some(rects) },
    })
}

fn click(state: &Rc<SelftestState>, webview: &WebView, point: CssPoint) {
    let scale = match state.input_space {
        InputSpace::CssLogical => 1.0,
        InputSpace::DevicePhysical => 1.0,
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

fn request_truth(state: &Rc<SelftestState>, webview: &WebView) {
    *state.pending_truth.borrow_mut() = None;
    let pending = state.pending_truth.clone();
    webview.evaluate_javascript(TRUTH_JS, move |result| {
        let value = match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        };
        *pending.borrow_mut() = Some(value);
    });
}

fn request_rects_if_idle(state: &Rc<SelftestState>, webview: &WebView) {
    if state.rects_in_flight.borrow().is_some() {
        return;
    }
    let page_index = state.runtime.borrow().index;
    *state.rects_in_flight.borrow_mut() = Some(page_index);
    let pending = state.pending_rects.clone();
    let in_flight = state.rects_in_flight.clone();
    webview.evaluate_javascript(RECTS_JS, move |result| {
        let value = match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        };
        *pending.borrow_mut() = Some((page_index, value));
        let mut in_flight = in_flight.borrow_mut();
        if *in_flight == Some(page_index) {
            *in_flight = None;
        }
    });
}

fn start_page(webview: &WebView) {
    webview.evaluate_javascript(START_JS, |_| {});
}

fn finish_rects_if_ready(state: &Rc<SelftestState>) {
    let Some((page_index, result)) = state.pending_rects.borrow_mut().take() else {
        return;
    };
    if page_index != state.runtime.borrow().index {
        return;
    }
    let t_obs_ns = state.run_started_at.elapsed().as_nanos() as u64;
    let rects = result
        .map(|text| parse_rects(&text, t_obs_ns))
        .unwrap_or_default();
    *state.last_rects.borrow_mut() = rects;
}

fn finish_truth_if_ready(state: &Rc<SelftestState>) -> Option<String> {
    state
        .pending_truth
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn parse_rects(text: &str, t_obs_ns: u64) -> Vec<DomRectObs> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let label = parts.next()?.to_string();
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            let w = parts.next()?.parse::<f32>().ok()?;
            let h = parts.next()?.parse::<f32>().ok()?;
            (x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
                .then_some(DomRectObs {
                    label,
                    rect_css: CssRect { x, y, w, h },
                    t_obs_ns,
                })
        })
        .collect()
}

fn parse_truth(text: &str) -> HashMap<String, String> {
    text.split_whitespace()
        .filter_map(|part| part.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn page_passed(spec: PageSpec, truth: &HashMap<String, String>) -> bool {
    let hits = truth
        .get("hits")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let misses = truth
        .get("misses")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let overlay_hits = truth
        .get("overlay_hits")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let finished = truth.get("finished").is_some_and(|value| value == "true");

    if spec.overlay_safe {
        misses == 0 && overlay_hits == 0 && finished
    } else {
        hits >= spec.required_hits && misses == 0 && finished
    }
}

fn record_outcome(
    state: &Rc<SelftestState>,
    spec: PageSpec,
    passed: bool,
    truth: String,
    detail: String,
) {
    let clicks_sent = state.runtime.borrow().clicks_sent;
    let points = state
        .runtime
        .borrow()
        .click_points
        .iter()
        .map(|point| format!("({:.1},{:.1})", point.x, point.y))
        .collect::<Vec<_>>()
        .join(",");
    let detail = if points.is_empty() {
        detail
    } else {
        format!("{detail} points={points}")
    };
    state.outcomes.borrow_mut().push(SelftestPageOutcome {
        name: spec.name.into(),
        passed,
        truth,
        clicks_sent,
        detail,
    });
}

fn load_next_page(state: &Rc<SelftestState>, webview: &WebView) {
    let next = state.runtime.borrow().index + 1;
    *state.runtime.borrow_mut() = PageRuntime::new(next);
    *state.pending_rects.borrow_mut() = None;
    *state.rects_in_flight.borrow_mut() = None;
    state.last_rects.borrow_mut().clear();
    if next < PAGES.len() {
        let url = state
            .base_url
            .join(PAGES[next].path)
            .expect("page path is valid");
        webview.load(url);
    }
}

fn finish_ok(state: &Rc<SelftestState>, event_loop: &ActiveEventLoop, report: SelftestPagesReport) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(report));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<SelftestState>, event_loop: &ActiveEventLoop, message: String) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Err(message));
    }
    event_loop.exit();
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

const TRUTH_JS: &str = r#"
(() => {
  const truth = document.getElementById('truth');
  const base = truth ? truth.textContent : 'missing_truth=true';
  return `${base} target_nodes=${document.querySelectorAll('.target').length}`;
})()
"#;

const RECTS_JS: &str = r#"
(() => {
  return Array.from(document.querySelectorAll('.target')).map((node, index) => {
    const rect = node.getBoundingClientRect();
    const rawLabel = node.getAttribute('aria-label') || node.tagName || 'target';
    const label = `${rawLabel}:${index}`.replace(/[|\n\r]/g, '_');
    return `${label}|${rect.left}|${rect.top}|${rect.width}|${rect.height}`;
  }).join('\n');
})()
"#;

const START_JS: &str = r#"
(() => {
  if (window.__saccadeStart) window.__saccadeStart();
  return 'ok';
})()
"#;
