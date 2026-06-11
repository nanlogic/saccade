use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::{Point2D, Scale};
use serde::Deserialize;
use servo::{
    CSSPixel, DeviceIntRect, DeviceIntSize, InputEvent, JSValue, LoadStatus, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, RenderingContext, Servo, ServoBuilder,
    WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(75);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(5);
const MID_GAME_AT: Duration = Duration::from_secs(2);
const RESULT_AT: Duration = Duration::from_secs(19);

#[derive(Debug, Default)]
pub struct RealSiteRecon {
    pub screenshots: Vec<PathBuf>,
    pub initial_probe_json: Option<String>,
    pub after_options_probe_json: Option<String>,
    pub arm_observation_json: Option<String>,
    pub final_probe_json: Option<String>,
    pub errors: Vec<String>,
}

pub fn real_site_recon(url: Url, output_dir: PathBuf) -> Result<RealSiteRecon> {
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = ReconApp::new(&event_loop, url, output_dir, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(result)) => Ok(result),
        Some(Err(message)) => bail!(message),
        None => bail!("recon exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitLoad,
    LoadedScreenshot,
    InitialProbe,
    ClickEpic,
    ClickTiny,
    AfterOptionsScreenshot,
    AfterOptionsProbe,
    ClickStart,
    ArmObservation,
    WaitMidGame,
    MidGameScreenshot,
    WaitResults,
    ResultsScreenshot,
    FinalProbe,
    Done,
}

#[derive(Debug, Default, Deserialize)]
struct Probe {
    controls: Option<Controls>,
}

#[derive(Debug, Default, Deserialize)]
struct Controls {
    epic: Option<Vec<Rect>>,
    tiny: Option<Vec<Rect>>,
    start: Option<Vec<Rect>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

struct ReconState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    url: Url,
    output_dir: PathBuf,
    phase: Cell<Phase>,
    phase_started_at: Cell<Instant>,
    run_started_at: Cell<Option<Instant>>,
    pending_js: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_screenshot: Rc<RefCell<Option<std::result::Result<PathBuf, String>>>>,
    recon: RefCell<RealSiteRecon>,
    result: Rc<RefCell<Option<std::result::Result<RealSiteRecon, String>>>>,
}

impl WebViewDelegate for ReconState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum ReconApp {
    Initial {
        waker: Waker,
        url: Url,
        output_dir: PathBuf,
        result: Rc<RefCell<Option<std::result::Result<RealSiteRecon, String>>>>,
        started_at: Instant,
    },
    Running {
        state: Rc<ReconState>,
        started_at: Instant,
    },
    Finished,
}

impl ReconApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        url: Url,
        output_dir: PathBuf,
        result: Rc<RefCell<Option<std::result::Result<RealSiteRecon, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            url,
            output_dir,
            result,
            started_at: Instant::now(),
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let (state, started_at) = match self {
            Self::Running { state, started_at } => (state.clone(), *started_at),
            _ => return,
        };

        if started_at.elapsed() > TOTAL_TIMEOUT {
            state
                .recon
                .borrow_mut()
                .errors
                .push(format!("recon timed out after {:?}", TOTAL_TIMEOUT));
            finish_ok(&state, event_loop);
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

        match state.phase.get() {
            Phase::WaitLoad => {
                if webview.load_status() == LoadStatus::Complete {
                    advance(&state, Phase::LoadedScreenshot);
                    request_screenshot(&state, &webview, "01_loaded.png");
                } else if state.phase_started_at.get().elapsed() > LOAD_TIMEOUT {
                    state.recon.borrow_mut().errors.push(format!(
                        "real page did not complete load within {:?}",
                        LOAD_TIMEOUT
                    ));
                    match save_readback(&state, &webview, "01_loaded_timeout_readback.png") {
                        Ok(path) => state.recon.borrow_mut().screenshots.push(path),
                        Err(error) => state.recon.borrow_mut().errors.push(error),
                    }
                    finish_ok(&state, event_loop);
                    *self = Self::Finished;
                    return;
                }
            }
            Phase::LoadedScreenshot => {
                if finish_screenshot_if_ready(&state) {
                    advance(&state, Phase::InitialProbe);
                    request_js(&state, &webview, RECON_PROBE_JS);
                } else {
                    maybe_fallback_screenshot(&state, &webview, "01_loaded_readback.png");
                }
            }
            Phase::InitialProbe => {
                if let Some(json) = finish_js_if_ready(&state) {
                    state.recon.borrow_mut().initial_probe_json = Some(json);
                    advance(&state, Phase::ClickEpic);
                }
            }
            Phase::ClickEpic => {
                if click_control(&state, &webview, ControlName::Epic) {
                    advance(&state, Phase::ClickTiny);
                } else {
                    advance(&state, Phase::AfterOptionsScreenshot);
                    request_screenshot(&state, &webview, "02_after_options.png");
                }
            }
            Phase::ClickTiny => {
                let _ = click_control(&state, &webview, ControlName::Tiny);
                advance(&state, Phase::AfterOptionsScreenshot);
                request_screenshot(&state, &webview, "02_after_options.png");
            }
            Phase::AfterOptionsScreenshot => {
                if finish_screenshot_if_ready(&state) {
                    advance(&state, Phase::AfterOptionsProbe);
                    request_js(&state, &webview, RECON_PROBE_JS);
                } else {
                    maybe_fallback_screenshot(&state, &webview, "02_after_options_readback.png");
                }
            }
            Phase::AfterOptionsProbe => {
                if let Some(json) = finish_js_if_ready(&state) {
                    state.recon.borrow_mut().after_options_probe_json = Some(json);
                    advance(&state, Phase::ClickStart);
                }
            }
            Phase::ClickStart => {
                if click_control(&state, &webview, ControlName::Start) {
                    state.run_started_at.set(Some(Instant::now()));
                    advance(&state, Phase::ArmObservation);
                    request_js(&state, &webview, ARM_OBSERVATION_JS);
                } else {
                    state
                        .recon
                        .borrow_mut()
                        .errors
                        .push("could not click Start control; stopping M1 recon early".into());
                    finish_ok(&state, event_loop);
                    *self = Self::Finished;
                    return;
                }
            }
            Phase::ArmObservation => {
                if let Some(json) = finish_js_if_ready(&state) {
                    state.recon.borrow_mut().arm_observation_json = Some(json);
                    advance(&state, Phase::WaitMidGame);
                }
            }
            Phase::WaitMidGame => {
                if state
                    .run_started_at
                    .get()
                    .is_some_and(|started| started.elapsed() >= MID_GAME_AT)
                {
                    advance(&state, Phase::MidGameScreenshot);
                    request_screenshot(&state, &webview, "03_mid_game.png");
                }
            }
            Phase::MidGameScreenshot => {
                if finish_screenshot_if_ready(&state) {
                    advance(&state, Phase::WaitResults);
                } else {
                    maybe_fallback_screenshot(&state, &webview, "03_mid_game_readback.png");
                }
            }
            Phase::WaitResults => {
                if state
                    .run_started_at
                    .get()
                    .is_some_and(|started| started.elapsed() >= RESULT_AT)
                {
                    advance(&state, Phase::ResultsScreenshot);
                    request_screenshot(&state, &webview, "04_results.png");
                }
            }
            Phase::ResultsScreenshot => {
                if finish_screenshot_if_ready(&state) {
                    advance(&state, Phase::FinalProbe);
                    request_js(&state, &webview, FINAL_PROBE_JS);
                } else {
                    maybe_fallback_screenshot(&state, &webview, "04_results_readback.png");
                }
            }
            Phase::FinalProbe => {
                if let Some(json) = finish_js_if_ready(&state) {
                    state.recon.borrow_mut().final_probe_json = Some(json);
                    advance(&state, Phase::Done);
                    finish_ok(&state, event_loop);
                    *self = Self::Finished;
                    return;
                }
            }
            Phase::Done => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for ReconApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        let Self::Initial {
            waker,
            url,
            output_dir,
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
                .with_title("Saccade M1 Recon")
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

        let state = Rc::new(ReconState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            url: url.clone(),
            output_dir: output_dir.clone(),
            phase: Cell::new(Phase::WaitLoad),
            phase_started_at: Cell::new(Instant::now()),
            run_started_at: Cell::new(None),
            pending_js: Rc::new(RefCell::new(None)),
            pending_screenshot: Rc::new(RefCell::new(None)),
            recon: RefCell::new(RealSiteRecon::default()),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.url.clone())
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
                state.window.request_redraw();
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

fn advance(state: &Rc<ReconState>, phase: Phase) {
    state.phase.set(phase);
    state.phase_started_at.set(Instant::now());
}

fn finish_ok(state: &Rc<ReconState>, event_loop: &ActiveEventLoop) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(std::mem::take(&mut *state.recon.borrow_mut())));
    }
    event_loop.exit();
}

fn request_js(state: &Rc<ReconState>, webview: &WebView, script: &'static str) {
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

fn finish_js_if_ready(state: &Rc<ReconState>) -> Option<String> {
    let result = state.pending_js.borrow_mut().take()?;
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            state.recon.borrow_mut().errors.push(error);
            Some(String::new())
        }
    }
}

fn request_screenshot(state: &Rc<ReconState>, webview: &WebView, filename: &'static str) {
    *state.pending_screenshot.borrow_mut() = None;
    let path = state.output_dir.join(filename);
    let pending = state.pending_screenshot.clone();
    webview.take_screenshot(None, move |result| {
        let outcome = match result {
            Ok(image) => image
                .save(&path)
                .map(|_| path.clone())
                .map_err(|error| format!("failed to save {}: {error}", path.display())),
            Err(error) => Err(format!(
                "screenshot failed for {}: {error:?}",
                path.display()
            )),
        };
        if pending.borrow().is_none() {
            *pending.borrow_mut() = Some(outcome);
        }
    });
}

fn finish_screenshot_if_ready(state: &Rc<ReconState>) -> bool {
    let Some(result) = state.pending_screenshot.borrow_mut().take() else {
        return false;
    };
    match result {
        Ok(path) => state.recon.borrow_mut().screenshots.push(path),
        Err(error) => state.recon.borrow_mut().errors.push(error),
    }
    true
}

fn maybe_fallback_screenshot(state: &Rc<ReconState>, webview: &WebView, filename: &'static str) {
    if state.phase_started_at.get().elapsed() >= SCREENSHOT_TIMEOUT {
        state.recon.borrow_mut().errors.push(format!(
            "take_screenshot timed out for {filename}; used readback fallback"
        ));
        *state.pending_screenshot.borrow_mut() = Some(save_readback(state, webview, filename));
    }
}

fn save_readback(
    state: &Rc<ReconState>,
    webview: &WebView,
    filename: &'static str,
) -> std::result::Result<PathBuf, String> {
    webview.paint();
    let rect = DeviceIntRect::from_size(DeviceIntSize::new(
        WINDOW_WIDTH as i32,
        WINDOW_HEIGHT as i32,
    ));
    let path = state.output_dir.join(filename);
    state
        .rendering_context
        .read_to_image(rect)
        .ok_or_else(|| format!("readback returned no image for {}", path.display()))
        .and_then(|image| {
            image
                .save(&path)
                .map_err(|error| format!("failed to save {}: {error}", path.display()))
        })?;
    Ok(path)
}

#[derive(Clone, Copy)]
enum ControlName {
    Epic,
    Tiny,
    Start,
}

fn click_control(state: &Rc<ReconState>, webview: &WebView, name: ControlName) -> bool {
    let probe_json = {
        let recon = state.recon.borrow();
        recon
            .after_options_probe_json
            .as_ref()
            .or(recon.initial_probe_json.as_ref())
            .cloned()
    };

    let Some(probe_json) = probe_json else {
        state
            .recon
            .borrow_mut()
            .errors
            .push("missing probe JSON before control click".into());
        return false;
    };

    let probe = match serde_json::from_str::<Probe>(&probe_json) {
        Ok(probe) => probe,
        Err(error) => {
            state
                .recon
                .borrow_mut()
                .errors
                .push(format!("failed to parse probe JSON before click: {error}"));
            return false;
        }
    };

    let rect = match control_rect(probe.controls.as_ref(), name) {
        Some(rect) => rect,
        None => {
            state
                .recon
                .borrow_mut()
                .errors
                .push(format!("missing {} control rect", control_label(name)));
            return false;
        }
    };

    let point = WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(
        rect.x + rect.w / 2.0,
        rect.y + rect.h / 2.0,
    ));
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
    true
}

fn control_rect(controls: Option<&Controls>, name: ControlName) -> Option<Rect> {
    let controls = controls?;
    let candidates = match name {
        ControlName::Epic => controls.epic.as_ref()?,
        ControlName::Tiny => controls.tiny.as_ref()?,
        ControlName::Start => controls.start.as_ref()?,
    };
    candidates.first().copied()
}

fn control_label(name: ControlName) -> &'static str {
    match name {
        ControlName::Epic => "Epic",
        ControlName::Tiny => "Tiny",
        ControlName::Start => "Start",
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

const RECON_PROBE_JS: &str = r#"
(() => {
  const vis = el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
    return r.width>0 && r.height>0 && s.visibility!=='hidden' && s.display!=='none'; };
  const rect = el => { const r = el.getBoundingClientRect();
    return {x:r.x, y:r.y, w:r.width, h:r.height}; };
  const byText = t => { const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const out=[]; let n; while(n=w.nextNode()){ if(n.textContent.trim()===t){
      let el=n.parentElement; for(let i=0;i<4&&el;i++){ if(vis(el)&&(el.onclick||el.tagName==='BUTTON'||
        el.tagName==='A'||el.tagName==='LABEL'||getComputedStyle(el).cursor==='pointer')) break;
        el=el.parentElement; } if(el&&vis(el)) out.push(rect(el)); }} return out; };
  const canvases = [...document.querySelectorAll('canvas')].filter(vis).map(c =>
    ({rect:rect(c), w:c.width, h:c.height}));
  const iframes = [...document.querySelectorAll('iframe')].map(f =>
    ({rect:rect(f), src:(f.src||'').slice(0,120)}));
  const textLines = () => document.body.innerText.split(/\n+/)
    .map(t => t.trim()).filter(Boolean);
  let best=null; for (const el of document.querySelectorAll('div,main,section,canvas')) {
    if(!vis(el)) continue; const r=el.getBoundingClientRect();
    const a=r.width*r.height; if(a>1e4 && (!best || a>best.a)) best={a, rect:rect(el),
      hint:(el.id?'#'+el.id:'')+(el.className?'.'+String(el.className).split(' ')[0]:'')}; }
  const checked = [...document.querySelectorAll('input:checked, option:checked')].map(e =>
    ({tag:e.tagName, type:e.type||'', value:e.value||'', text:e.textContent.trim()}));
  return JSON.stringify({
    title: document.title,
    url: location.href,
    dpr: devicePixelRatio,
    pointerEvents: ('onpointerdown' in window),
    controls: { epic: byText('Epic'), tiny: byText('Tiny'),
                start: byText('Start!').concat(byText('Start')) },
    checked,
    scoreText: textLines().filter(t=>/clicked|misclicked|Time Remaining|Time is up/i.test(t)).slice(0,20),
    bodyTextSample: document.body.innerText.slice(0, 1200),
    canvases, iframes, container: best,
  });
})()
"#;

const ARM_OBSERVATION_JS: &str = r#"
(() => {
  const vis = el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
    return r.width>0 && r.height>0 && s.visibility!=='hidden' && s.display!=='none'; };
  const rect = el => { const r = el.getBoundingClientRect();
    return {x:r.x, y:r.y, w:r.width, h:r.height}; };
  const classText = el => String(el.className || '').slice(0, 80);
  const isTarget = el => /\btarget\b/.test(classText(el));
  const textLines = () => document.body.innerText.split(/\n+/)
    .map(t => t.trim()).filter(Boolean);
  const scoreText = () => textLines()
    .filter(t=>/clicked|misclicked|Time Remaining|Time is up/i.test(t)).slice(0,20);
  const targetElements = () => [...document.querySelectorAll('.target,[class*="target"]')]
    .filter(vis).map(e => ({tag:e.tagName, id:e.id||'', cls:classText(e), rect:rect(e)})).slice(0,80);
  const state = {armedAt: performance.now(), mutations: [], samples: [], droppedMutations: 0};
  const recordMutation = entry => {
    if (state.mutations.length < 300) state.mutations.push(entry);
    else state.droppedMutations += 1;
  };
  const sample = () => state.samples.push({
    t: performance.now(),
    scoreText: scoreText(),
    canvases: [...document.querySelectorAll('canvas')].filter(vis).map(c => ({rect:rect(c), w:c.width, h:c.height})),
    targets: targetElements()
  });
  const observer = new MutationObserver(muts => {
    for (const m of muts) {
      for (const n of m.addedNodes) if (n.nodeType === 1 && isTarget(n) && vis(n)) recordMutation({t:performance.now(), kind:'added', tag:n.tagName, id:n.id||'', cls:classText(n), rect:rect(n)});
      for (const n of m.removedNodes) if (n.nodeType === 1 && isTarget(n)) recordMutation({t:performance.now(), kind:'removed', tag:n.tagName, id:n.id||'', cls:classText(n)});
    }
  });
  observer.observe(document.body, {childList:true, subtree:true});
  state.interval = setInterval(sample, 100);
  window.__saccadeM1 = state;
  window.__saccadeM1Observer = observer;
  sample();
  return JSON.stringify({armed:true, t:state.armedAt});
})()
"#;

const FINAL_PROBE_JS: &str = r#"
(() => {
  if (window.__saccadeM1 && window.__saccadeM1.interval) clearInterval(window.__saccadeM1.interval);
  if (window.__saccadeM1Observer) window.__saccadeM1Observer.disconnect();
  const vis = el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
    return r.width>0 && r.height>0 && s.visibility!=='hidden' && s.display!=='none'; };
  const rect = el => { const r = el.getBoundingClientRect();
    return {x:r.x, y:r.y, w:r.width, h:r.height}; };
  const text = document.body.innerText.split(/\n+/).map(t => t.trim()).filter(Boolean);
  return JSON.stringify({
    title: document.title,
    url: location.href,
    dpr: devicePixelRatio,
    scoreText: text.filter(t=>/clicked|misclicked|Time Remaining|Time is up/i.test(t)).slice(0,40),
    resultText: text.filter(t=>/Time is up|You clicked|You misclicked/i.test(t)).slice(0,40),
    bodyTextSample: document.body.innerText.slice(0, 1600),
    canvases: [...document.querySelectorAll('canvas')].filter(vis).map(c => ({rect:rect(c), w:c.width, h:c.height})),
    iframes: [...document.querySelectorAll('iframe')].map(f => ({rect:rect(f), src:(f.src||'').slice(0,120)})),
    observation: window.__saccadeM1 || null
  });
})()
"#;
