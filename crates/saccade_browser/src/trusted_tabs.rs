use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde_json::Value;
use servo::{
    JSValue, LoadStatus, RenderingContext, Servo, ServoBuilder, WebView, WebViewBuilder,
    WebViewDelegate, WindowRenderingContext,
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
const TABS_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub struct TrustedTabsProfile {
    pub webviews: usize,
    pub cookie_shared: bool,
    pub storage_shared: bool,
    pub input_isolated: bool,
    pub read_policy_enforced: bool,
    pub human_login: bool,
    pub agent_session: bool,
    pub password_exposed: bool,
}

pub fn selftest_trusted_tabs(base_url: Url) -> Result<TrustedTabsProfile> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = TabsApp::new(&event_loop, base_url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(profile)) => Ok(profile),
        Some(Err(message)) => bail!(message),
        None => bail!("trusted tabs selftest exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitHumanDashboard,
    ProbeHuman,
    WaitAgentDashboard,
    ProbeAgent,
    Done,
}

struct TabRuntime {
    info: TabInfo,
    webview: WebView,
}

struct Runtime {
    phase: Phase,
    human_probe_requested: bool,
    agent_probe_requested: bool,
    human_probe: Option<Value>,
}

struct TabsState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    base_url: Url,
    started_at: Instant,
    tabs: RefCell<Vec<TabRuntime>>,
    runtime: RefCell<Runtime>,
    pending_human_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_agent_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    result: Rc<RefCell<Option<std::result::Result<TrustedTabsProfile, String>>>>,
}

impl WebViewDelegate for TabsState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum TabsApp {
    Initial {
        waker: Waker,
        base_url: Url,
        result: Rc<RefCell<Option<std::result::Result<TrustedTabsProfile, String>>>>,
    },
    Running {
        state: Rc<TabsState>,
    },
    Finished,
}

impl TabsApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        base_url: Url,
        result: Rc<RefCell<Option<std::result::Result<TrustedTabsProfile, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            base_url,
            result,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self {
            Self::Running { state } => state.clone(),
            _ => return,
        };

        if state.started_at.elapsed() > TABS_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("trusted tabs selftest timed out after {TABS_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let phase = state.runtime.borrow().phase;
        match phase {
            Phase::WaitHumanDashboard => {
                let Some(human) = tab(&state, TabId(1)) else {
                    return;
                };
                if human.webview.load_status() == LoadStatus::Complete
                    && human.webview.page_title().as_deref() == Some("Dashboard")
                {
                    request_human_probe(&state, &human.webview);
                    let mut runtime = state.runtime.borrow_mut();
                    runtime.phase = Phase::ProbeHuman;
                    runtime.human_probe_requested = true;
                }
            }
            Phase::ProbeHuman => {
                let Some(probe) = finish_probe(&state.pending_human_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&probe) else {
                    finish_err(&state, event_loop, "failed to parse human probe".into());
                    *self = Self::Finished;
                    return;
                };
                if !probe_text(&probe).contains("LOGGED_IN") {
                    finish_err(
                        &state,
                        event_loop,
                        "human tab did not reach logged-in dashboard".into(),
                    );
                    *self = Self::Finished;
                    return;
                }
                state.runtime.borrow_mut().human_probe = Some(probe);
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                match state.base_url.join("dashboard.html") {
                    Ok(url) => agent.webview.load(url),
                    Err(error) => {
                        finish_err(
                            &state,
                            event_loop,
                            format!("failed to build dashboard URL: {error}"),
                        );
                        *self = Self::Finished;
                        return;
                    }
                }
                state.runtime.borrow_mut().phase = Phase::WaitAgentDashboard;
            }
            Phase::WaitAgentDashboard => {
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                if agent.webview.load_status() == LoadStatus::Complete
                    && agent.webview.page_title().as_deref() == Some("Dashboard")
                {
                    request_agent_probe(&state, &agent.webview);
                    let mut runtime = state.runtime.borrow_mut();
                    runtime.phase = Phase::ProbeAgent;
                    runtime.agent_probe_requested = true;
                }
            }
            Phase::ProbeAgent => {
                let Some(agent_probe) = finish_probe(&state.pending_agent_probe) else {
                    return;
                };
                let Ok(agent_probe) = parse_probe(&agent_probe) else {
                    finish_err(&state, event_loop, "failed to parse agent probe".into());
                    *self = Self::Finished;
                    return;
                };
                let profile = build_profile(&state, &agent_probe);
                if !profile.input_isolated || !profile.read_policy_enforced {
                    finish_err(
                        &state,
                        event_loop,
                        format!("policy selftest failed: {profile:?}"),
                    );
                    *self = Self::Finished;
                    return;
                }
                finish_ok(&state, event_loop, profile);
                state.runtime.borrow_mut().phase = Phase::Done;
                *self = Self::Finished;
            }
            Phase::Done => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for TabsApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            base_url,
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
                .with_title("Saccade Trusted Tabs")
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

        let state = Rc::new(TabsState {
            window,
            servo,
            rendering_context,
            base_url: base_url.clone(),
            started_at: Instant::now(),
            tabs: RefCell::new(Vec::new()),
            runtime: RefCell::new(Runtime {
                phase: Phase::WaitHumanDashboard,
                human_probe_requested: false,
                agent_probe_requested: false,
                human_probe: None,
            }),
            pending_human_probe: Rc::new(RefCell::new(None)),
            pending_agent_probe: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let human_url = match state.base_url.join("login.html?auto=1") {
            Ok(url) => url,
            Err(error) => {
                *state.result.borrow_mut() =
                    Some(Err(format!("failed to build login URL: {error}")));
                event_loop.exit();
                return;
            }
        };
        let agent_url = Url::parse("about:blank").expect("static URL is valid");

        let human_webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(human_url.clone())
            .hidpi_scale_factor(Scale::new(1.0))
            .delegate(state.clone())
            .build();
        let agent_webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(agent_url.clone())
            .hidpi_scale_factor(Scale::new(1.0))
            .delegate(state.clone())
            .build();

        state.tabs.borrow_mut().push(TabRuntime {
            info: tab_info(TabId(1), TabOwner::Human, ReadGrant::None, human_url),
            webview: human_webview,
        });
        state.tabs.borrow_mut().push(TabRuntime {
            info: tab_info(TabId(2), TabOwner::Agent, ReadGrant::FullTruth, agent_url),
            webview: agent_webview,
        });

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
                        "window closed before trusted tabs selftest finished".into(),
                    );
                    *self = Self::Finished;
                }
                WindowEvent::RedrawRequested => {
                    if let Some(active) = state.tabs.borrow().last() {
                        active.webview.paint();
                        state.rendering_context.present();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    state.rendering_context.resize(new_size);
                    for tab in state.tabs.borrow().iter() {
                        tab.webview.resize(new_size);
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

fn tab(state: &Rc<TabsState>, tab_id: TabId) -> Option<TabRuntime> {
    state
        .tabs
        .borrow()
        .iter()
        .find(|tab| tab.info.tab_id == tab_id)
        .map(|tab| TabRuntime {
            info: tab.info.clone(),
            webview: tab.webview.clone(),
        })
}

fn tab_info(tab_id: TabId, owner: TabOwner, read_grant: ReadGrant, url: Url) -> TabInfo {
    let badge = match owner {
        TabOwner::Human => "HUMAN",
        TabOwner::Agent => "AGENT",
    };
    let color_name = match owner {
        TabOwner::Human => "blue",
        TabOwner::Agent => "green",
    };
    TabInfo {
        tab_id,
        owner,
        url: url.to_string(),
        title: None,
        read_grant,
        page_revision: 0,
        visual_marker: TabVisualMarker {
            border: true,
            badge: badge.into(),
            color_name: color_name.into(),
        },
    }
}

fn request_human_probe(state: &Rc<TabsState>, webview: &WebView) {
    *state.pending_human_probe.borrow_mut() = None;
    let pending = state.pending_human_probe.clone();
    webview.evaluate_javascript(PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn request_agent_probe(state: &Rc<TabsState>, webview: &WebView) {
    *state.pending_agent_probe.borrow_mut() = None;
    let pending = state.pending_agent_probe.clone();
    webview.evaluate_javascript(PROBE_JS, move |result| {
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

fn probe_text(probe: &Value) -> String {
    probe
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn probe_cookie(probe: &Value) -> String {
    probe
        .get("cookie")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn probe_storage(probe: &Value) -> String {
    probe
        .get("storage")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn probe_password(probe: &Value) -> Option<String> {
    probe
        .get("password")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn build_profile(state: &Rc<TabsState>, agent_probe: &Value) -> TrustedTabsProfile {
    let human_probe = state
        .runtime
        .borrow()
        .human_probe
        .clone()
        .unwrap_or(Value::Null);
    let tabs = state.tabs.borrow();
    let human = tabs.iter().find(|tab| tab.info.tab_id == TabId(1));
    let agent = tabs.iter().find(|tab| tab.info.tab_id == TabId(2));
    let input_isolated = human.is_some_and(|tab| !tab.info.agent_input_allowed())
        && agent.is_some_and(|tab| tab.info.agent_input_allowed());
    let read_policy_enforced = human.is_some_and(|tab| !tab.info.agent_truth_allowed())
        && agent.is_some_and(|tab| tab.info.agent_truth_allowed());
    let human_password = probe_password(&human_probe).unwrap_or_default();
    let agent_password = probe_password(agent_probe).unwrap_or_default();

    TrustedTabsProfile {
        webviews: tabs.len(),
        cookie_shared: probe_cookie(agent_probe).contains("saccade_session=demo"),
        storage_shared: probe_storage(agent_probe) == "shared",
        input_isolated,
        read_policy_enforced,
        human_login: probe_text(&human_probe).contains("LOGGED_IN"),
        agent_session: probe_text(agent_probe).contains("LOGGED_IN"),
        password_exposed: !human_password.is_empty() || !agent_password.is_empty(),
    }
}

fn finish_ok(state: &Rc<TabsState>, event_loop: &ActiveEventLoop, profile: TrustedTabsProfile) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(profile));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<TabsState>, event_loop: &ActiveEventLoop, message: String) {
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

const PROBE_JS: &str = r#"
JSON.stringify({
  title: document.title,
  url: location.href,
  text: document.body ? document.body.innerText : "",
  cookie: document.cookie,
  storage: localStorage.getItem("saccade_storage") || "",
  password: (document.querySelector('input[type="password"]') || {}).value || ""
})
"#;
