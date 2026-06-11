use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use saccade_core::{
    AccuracySummary, BenchmarkResult, ClickOutcome, ClickReceipt, CssPoint, CssRect, DetectorUsage,
    DifficultyConfig, DomRectObs, FrameObservation, Histogram, InputBackendKind, InputSpace,
    LatencyPair, LatencySummary, MotorAction, PixelRegion, RunCounters, ScoreState,
    VerificationResult, ViewportInfo,
};
use saccade_detect::{DetectConfig, DetectionPipeline};
use saccade_motor::MotorController;
use saccade_replay::{ReplayEvent, ReplayLogger};
use serde_json::json;
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
const BACKGROUND_DELAY: Duration = Duration::from_millis(160);
const FRAME_INTERVAL: Duration = Duration::from_millis(20);
const SCORE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct ArenaRunConfig {
    pub url: Url,
    pub run_id: String,
    pub spawn_speed: String,
    pub target_size: String,
    pub duration_s: u32,
    pub seed: u64,
    pub instrumentation: String,
    pub input_space: InputSpace,
    pub calibration_max_err_css_px: f32,
    pub replay_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ArenaRunReport {
    pub result: BenchmarkResult,
}

pub fn run_arena(config: ArenaRunConfig) -> Result<ArenaRunReport> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = ArenaApp::new(&event_loop, config, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(report)) => Ok(report),
        Some(Err(message)) => bail!(message),
        None => bail!("arena run exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    Background,
    Run,
    ProbeScore,
}

struct ArenaRuntime {
    phase: Phase,
    page_started_at: Instant,
    phase_started_at: Instant,
    last_frame_at: Instant,
    last_score_at: Instant,
    frame_id: u64,
    pipeline: DetectionPipeline,
    motor: MotorController,
}

impl ArenaRuntime {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            phase: Phase::Load,
            page_started_at: now,
            phase_started_at: now,
            last_frame_at: now - FRAME_INTERVAL,
            last_score_at: now - SCORE_INTERVAL,
            frame_id: 0,
            pipeline: DetectionPipeline::default(),
            motor: MotorController::default(),
        }
    }

    fn advance(&mut self, phase: Phase) {
        self.phase = phase;
        self.phase_started_at = Instant::now();
    }
}

#[derive(Default)]
struct RunHistograms {
    detect_to_dispatch: Histogram,
    first_visible_to_dispatch: Histogram,
    capture: Histogram,
    detect: Histogram,
}

struct ArenaMetrics {
    clicks_sent: u32,
    click_id: u64,
    targets_seen: u32,
    stale_clicks: u32,
    pending_clicks: VecDeque<ClickReceipt>,
    verified_hits: u32,
    detectors_used: DetectorUsage,
    histograms: RunHistograms,
    last_score: Option<ArenaScore>,
}

impl Default for ArenaMetrics {
    fn default() -> Self {
        Self {
            clicks_sent: 0,
            click_id: 0,
            targets_seen: 0,
            stale_clicks: 0,
            pending_clicks: VecDeque::new(),
            verified_hits: 0,
            detectors_used: DetectorUsage {
                pixel_detector: 0,
                dom_rect: 0,
                canvas_observe: 0,
                fused: 0,
            },
            histograms: RunHistograms::default(),
            last_score: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ArenaScore {
    score: ScoreState,
    spawned: u32,
    expired: u32,
}

struct ArenaState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    config: ArenaRunConfig,
    run_started_at: Instant,
    runtime: RefCell<ArenaRuntime>,
    pending_score: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_rects: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    rects_in_flight: Rc<RefCell<bool>>,
    last_rects: RefCell<Vec<DomRectObs>>,
    metrics: RefCell<ArenaMetrics>,
    logger: RefCell<Option<ReplayLogger>>,
    result: Rc<RefCell<Option<std::result::Result<ArenaRunReport, String>>>>,
}

impl WebViewDelegate for ArenaState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum ArenaApp {
    Initial {
        waker: Waker,
        config: ArenaRunConfig,
        result: Rc<RefCell<Option<std::result::Result<ArenaRunReport, String>>>>,
    },
    Running {
        state: Rc<ArenaState>,
    },
    Finished,
}

impl ArenaApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        config: ArenaRunConfig,
        result: Rc<RefCell<Option<std::result::Result<ArenaRunReport, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            config,
            result,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        let timeout = Duration::from_secs(state.config.duration_s as u64 + 8);
        if state.runtime.borrow().page_started_at.elapsed() > timeout {
            finish_err(
                &state,
                event_loop,
                format!("arena run timed out after {timeout:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

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
                maybe_run_frame(&state, &webview);
                if state.runtime.borrow().last_score_at.elapsed() >= SCORE_INTERVAL {
                    request_score(&state, &webview);
                    state.runtime.borrow_mut().advance(Phase::ProbeScore);
                }
            }
            Phase::ProbeScore => {
                if let Some(raw_score) = finish_score_if_ready(&state) {
                    let score = parse_arena_score(&raw_score, elapsed_ns(state.run_started_at));
                    log_score(&state, &score);
                    reconcile_score(&state, &score);
                    let finished = score.score.finished;
                    state.metrics.borrow_mut().last_score = Some(score);
                    if finished {
                        match finish_run(&state) {
                            Ok(report) => {
                                finish_ok(&state, event_loop, report);
                                *self = Self::Finished;
                            }
                            Err(error) => {
                                finish_err(&state, event_loop, error.to_string());
                                *self = Self::Finished;
                            }
                        }
                    } else {
                        state.runtime.borrow_mut().last_score_at = Instant::now();
                        state.runtime.borrow_mut().advance(Phase::Run);
                    }
                }
            }
            _ => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for ArenaApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            config,
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
                .with_title("Saccade M6 Arena")
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

        let logger = config.replay_path.clone().map(ReplayLogger::spawn);
        let state = Rc::new(ArenaState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            config: config.clone(),
            run_started_at: Instant::now(),
            runtime: RefCell::new(ArenaRuntime::new()),
            pending_score: Rc::new(RefCell::new(None)),
            pending_rects: Rc::new(RefCell::new(None)),
            rects_in_flight: Rc::new(RefCell::new(false)),
            last_rects: RefCell::new(Vec::new()),
            metrics: RefCell::new(ArenaMetrics::default()),
            logger: RefCell::new(logger),
            result: result.clone(),
        });

        log_run_started(&state);

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.config.url.clone())
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
                    "window closed before arena finished".into(),
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

fn maybe_run_frame(state: &Rc<ArenaState>, webview: &WebView) {
    if state.runtime.borrow().last_frame_at.elapsed() < FRAME_INTERVAL {
        return;
    }
    request_rects_if_idle(state, webview);
    let Some(obs) = capture(state, webview) else {
        return;
    };

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

    {
        let mut metrics = state.metrics.borrow_mut();
        metrics
            .histograms
            .capture
            .record_ns(obs.t_readback_ns.saturating_sub(obs.t_paint_ns));
        metrics
            .histograms
            .detect
            .record_ns((report.detector_ms * 1_000_000.0) as u64);
    }

    for event in state.runtime.borrow().pipeline.events().to_vec() {
        if let saccade_core::TrackerEvent::Appeared { target } = &event {
            if safe_area.contains(target.center_css) {
                state.metrics.borrow_mut().targets_seen += 1;
            }
        }
        log_event(state, ReplayEvent::TrackerEvent { event });
    }
    log_event(
        state,
        ReplayEvent::FrameReport {
            report: report.clone(),
        },
    );

    let now_ns = elapsed_ns(state.run_started_at);
    let action = state.runtime.borrow_mut().motor.on_frame(&report, now_ns);
    if let MotorAction::Click {
        target_id,
        point_css,
        frame_id,
    } = action
    {
        let Some(target) = report.targets.iter().find(|target| target.id == target_id) else {
            state.metrics.borrow_mut().stale_clicks += 1;
            state.runtime.borrow_mut().last_frame_at = Instant::now();
            return;
        };
        let t_decided_ns = elapsed_ns(state.run_started_at);
        let receipt = click(
            state,
            webview,
            state.metrics.borrow().click_id + 1,
            target_id,
            point_css,
            frame_id,
            target.first_seen_ns,
            t_decided_ns,
        );
        {
            let mut metrics = state.metrics.borrow_mut();
            metrics.click_id += 1;
            metrics.clicks_sent += 1;
            metrics.detectors_used.record(target.source);
            metrics
                .histograms
                .detect_to_dispatch
                .record_ns(receipt.t_down_sent_ns.saturating_sub(receipt.t_decided_ns));
            metrics.histograms.first_visible_to_dispatch.record_ns(
                receipt
                    .t_down_sent_ns
                    .saturating_sub(receipt.t_target_first_seen_ns),
            );
            metrics.pending_clicks.push_back(receipt.clone());
        }
        state.runtime.borrow_mut().pipeline.mark_clicked(target_id);
        log_event(state, ReplayEvent::ClickDispatched { receipt });
    }
    state.runtime.borrow_mut().last_frame_at = Instant::now();
}

fn capture(state: &Rc<ArenaState>, webview: &WebView) -> Option<FrameObservation> {
    finish_rects_if_ready(state);
    let run_started_at = state.run_started_at;
    webview.paint();
    let frame_id = {
        let mut runtime = state.runtime.borrow_mut();
        runtime.frame_id += 1;
        runtime.frame_id
    };
    let t_paint_ns = elapsed_ns(run_started_at);
    let rect = DeviceIntRect::from_size(DeviceIntSize::new(
        WINDOW_WIDTH as i32,
        WINDOW_HEIGHT as i32,
    ));
    let image = state.rendering_context.read_to_image(rect)?;
    let t_readback_ns = elapsed_ns(run_started_at);
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

fn click(
    state: &Rc<ArenaState>,
    webview: &WebView,
    click_id: u64,
    target_id: saccade_core::TargetId,
    point: CssPoint,
    frame_id: u64,
    t_target_first_seen_ns: u64,
    t_decided_ns: u64,
) -> ClickReceipt {
    let scale = match state.config.input_space {
        InputSpace::CssLogical => 1.0,
        InputSpace::DevicePhysical => 1.0,
    };
    let page_point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(
        point.x * scale,
        point.y * scale,
    ));
    webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(page_point)));
    let t_move_sent_ns = elapsed_ns(state.run_started_at);
    webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
        MouseButtonAction::Down,
        MouseButton::Left,
        page_point,
    )));
    let t_down_sent_ns = elapsed_ns(state.run_started_at);
    webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
        MouseButtonAction::Up,
        MouseButton::Left,
        page_point,
    )));
    let t_up_sent_ns = elapsed_ns(state.run_started_at);
    state.window.request_redraw();
    ClickReceipt {
        click_id,
        target_id,
        point_css: point,
        frame_id,
        t_target_first_seen_ns,
        t_decided_ns,
        t_move_sent_ns,
        t_down_sent_ns,
        t_up_sent_ns,
        backend: InputBackendKind::ServoInternal,
    }
}

fn request_score(state: &Rc<ArenaState>, webview: &WebView) {
    *state.pending_score.borrow_mut() = None;
    let pending = state.pending_score.clone();
    webview.evaluate_javascript(SCORE_JS, move |result| {
        let value = match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        };
        *pending.borrow_mut() = Some(value);
    });
}

fn request_rects_if_idle(state: &Rc<ArenaState>, webview: &WebView) {
    if *state.rects_in_flight.borrow() {
        return;
    }
    *state.rects_in_flight.borrow_mut() = true;
    let pending = state.pending_rects.clone();
    let in_flight = state.rects_in_flight.clone();
    webview.evaluate_javascript(RECTS_JS, move |result| {
        let value = match result {
            Ok(JSValue::String(value)) => Ok(value),
            Ok(value) => Ok(format!("{value:?}")),
            Err(error) => Err(format!("{error:?}")),
        };
        *pending.borrow_mut() = Some(value);
        *in_flight.borrow_mut() = false;
    });
}

fn start_page(webview: &WebView) {
    webview.evaluate_javascript(START_JS, |_| {});
}

fn finish_score_if_ready(state: &Rc<ArenaState>) -> Option<String> {
    state
        .pending_score
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn finish_rects_if_ready(state: &Rc<ArenaState>) {
    let Some(result) = state.pending_rects.borrow_mut().take() else {
        return;
    };
    let t_obs_ns = elapsed_ns(state.run_started_at);
    let rects = result
        .map(|text| parse_rects(&text, t_obs_ns))
        .unwrap_or_default();
    *state.last_rects.borrow_mut() = rects;
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

fn parse_arena_score(text: &str, t_obs_ns: u64) -> ArenaScore {
    let values = text
        .split_whitespace()
        .filter_map(|part| part.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>();
    let hits = parse_u32(&values, "hits");
    let misses = parse_u32(&values, "misses");
    ArenaScore {
        score: ScoreState {
            hits,
            misses,
            time_remaining_s: values
                .get("time_remaining_s")
                .and_then(|value| value.parse::<f32>().ok()),
            finished: values.get("finished").is_some_and(|value| value == "true"),
            t_obs_ns,
        },
        spawned: parse_u32(&values, "spawned"),
        expired: parse_u32(&values, "expired"),
    }
}

fn parse_u32(values: &HashMap<String, String>, key: &str) -> u32 {
    values
        .get(key)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

fn reconcile_score(state: &Rc<ArenaState>, arena_score: &ArenaScore) {
    let mut verified = Vec::new();
    {
        let mut metrics = state.metrics.borrow_mut();
        while metrics.verified_hits < arena_score.score.hits {
            let Some(receipt) = metrics.pending_clicks.pop_front() else {
                break;
            };
            metrics.verified_hits += 1;
            verified.push(VerificationResult {
                click_id: receipt.click_id,
                target_id: receipt.target_id,
                outcome: ClickOutcome::Hit,
                t_verified_ns: arena_score.score.t_obs_ns,
                reason: "arena hit counter advanced".into(),
            });
        }
    }
    for result in verified {
        log_event(state, ReplayEvent::ClickVerified { result });
    }
}

fn log_run_started(state: &Rc<ArenaState>) {
    let wall_clock_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    log_event(
        state,
        ReplayEvent::RunStarted {
            run_id: state.config.run_id.clone(),
            wall_clock_unix_ms,
            config: json!({
                "site": "arena",
                "url": state.config.url,
                "spawn_speed": state.config.spawn_speed,
                "target_size": state.config.target_size,
                "duration_s": state.config.duration_s,
                "seed": state.config.seed,
                "instrumentation": state.config.instrumentation,
            }),
            input_space: state.config.input_space,
        },
    );
}

fn log_score(state: &Rc<ArenaState>, arena_score: &ArenaScore) {
    log_event(
        state,
        ReplayEvent::ScorePoll {
            score: arena_score.score.clone(),
        },
    );
}

fn log_event(state: &Rc<ArenaState>, event: ReplayEvent) {
    if let Some(logger) = state.logger.borrow_mut().as_mut() {
        logger.try_log(event);
    }
}

fn finish_run(state: &Rc<ArenaState>) -> Result<ArenaRunReport> {
    let final_score = state
        .metrics
        .borrow()
        .last_score
        .clone()
        .context("arena finished without a score")?;
    reconcile_score(state, &final_score);

    let (counters, latency_ms, detectors_used) = {
        let metrics = state.metrics.borrow();
        let false_positive_clicks = metrics.clicks_sent.saturating_sub(final_score.score.hits);
        let unknown_verifications = metrics.pending_clicks.len() as u32;
        let counters = RunCounters {
            hits: final_score.score.hits,
            misses: final_score.score.misses,
            targets_seen: metrics.targets_seen,
            clicks_sent: metrics.clicks_sent,
            unknown_verifications,
            false_positive_clicks,
            stale_clicks: metrics.stale_clicks,
            expired_unclicked: final_score.expired,
        };
        let latency_ms = LatencySummary {
            detect_to_dispatch: LatencyPair::from(&metrics.histograms.detect_to_dispatch),
            first_visible_to_dispatch: LatencyPair::from(
                &metrics.histograms.first_visible_to_dispatch,
            ),
            capture: LatencyPair::from(&metrics.histograms.capture),
            detect: LatencyPair::from(&metrics.histograms.detect),
        };
        (counters, latency_ms, metrics.detectors_used.clone())
    };

    let pass = counters.misses == 0
        && counters.hits == counters.targets_seen
        && counters.hits == final_score.spawned
        && counters.false_positive_clicks == 0
        && counters.stale_clicks == 0
        && counters.unknown_verifications == 0
        && counters.expired_unclicked == 0
        && latency_ms.detect_to_dispatch.p95 <= 5.0
        && latency_ms.first_visible_to_dispatch.p95 <= FRAME_INTERVAL.as_secs_f32() * 1000.0 + 5.0;

    let result = BenchmarkResult {
        run_id: state.config.run_id.clone(),
        site: "arena".into(),
        url: state.config.url.to_string(),
        difficulty: DifficultyConfig {
            spawn_speed: state.config.spawn_speed.clone(),
            target_size: state.config.target_size.clone(),
        },
        duration_s: state.config.duration_s,
        verdict: if pass { "PASS" } else { "FAIL" }.into(),
        result: counters,
        latency_ms,
        accuracy: AccuracySummary {
            median_click_error_css_px: 0.0,
            max_click_error_css_px: 0.0,
        },
        detectors_used,
        instrumentation: state.config.instrumentation.clone(),
        input_space: state.config.input_space,
        llm_frame_calls: 0,
        calibration_max_err_css_px: state.config.calibration_max_err_css_px,
        replay_file: state
            .config
            .replay_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    };

    log_event(
        state,
        ReplayEvent::RunFinished {
            result: result.clone(),
        },
    );
    if let Some(logger) = state.logger.borrow_mut().take() {
        let drops = logger.finish()?;
        if drops.frame_reports > 0 || drops.other_events > 0 {
            eprintln!(
                "replay drops frame_reports={} other_events={}",
                drops.frame_reports, drops.other_events
            );
        }
    }

    Ok(ArenaRunReport { result })
}

fn finish_ok(state: &Rc<ArenaState>, event_loop: &ActiveEventLoop, report: ArenaRunReport) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(report));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<ArenaState>, event_loop: &ActiveEventLoop, message: String) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Err(message));
    }
    event_loop.exit();
}

fn elapsed_ns(start: Instant) -> u64 {
    start.elapsed().as_nanos() as u64
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

const SCORE_JS: &str = r#"
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
    const rawLabel = node.getAttribute('aria-label') || node.dataset.targetId || node.tagName || 'target';
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
