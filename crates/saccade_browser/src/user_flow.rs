use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde::Serialize;
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
const USER_FLOW_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize)]
pub struct UserFlowProfile {
    pub human_login: bool,
    pub handoff_done: bool,
    pub agent_session: bool,
    pub agent_input_to_human_tab_blocked: bool,
    pub read_policy_enforced: bool,
    pub agent_round1_filled: usize,
    pub user_can_see_agent_values: bool,
    pub round1_sensitive_completed_without_value: usize,
    pub round1_sensitive_requires_user_input: usize,
    pub user_page_change_seen: bool,
    pub user_normal_value_checked: bool,
    pub user_sensitive_status_checked_without_value: bool,
    pub agent_completed_remaining: usize,
    pub agent_preserved_user_values: bool,
    pub same_agent_tab_continued: bool,
    pub final_sensitive_completed_without_value: usize,
    pub agent_sensitive_values_exposed: bool,
}

pub fn selftest_user_flow(base_url: Url) -> Result<UserFlowProfile> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = UserFlowApp::new(&event_loop, base_url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(profile)) => Ok(profile),
        Some(Err(message)) => bail!(message),
        None => bail!("user flow selftest exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    WaitHumanLoginPage,
    SubmitHumanLogin,
    ProbeHuman,
    WaitAgentPage,
    AgentRound1,
    UserStep,
    AgentRound2,
    Done,
}

struct TabRuntime {
    info: TabInfo,
    webview: WebView,
}

struct Runtime {
    phase: Phase,
    human_probe: Option<Value>,
    round1_probe: Option<Value>,
    user_probe: Option<Value>,
}

struct UserFlowState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    base_url: Url,
    started_at: Instant,
    tabs: RefCell<Vec<TabRuntime>>,
    runtime: RefCell<Runtime>,
    pending_human_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    pending_agent_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    result: Rc<RefCell<Option<std::result::Result<UserFlowProfile, String>>>>,
}

impl WebViewDelegate for UserFlowState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum UserFlowApp {
    Initial {
        waker: Waker,
        base_url: Url,
        result: Rc<RefCell<Option<std::result::Result<UserFlowProfile, String>>>>,
    },
    Running {
        state: Rc<UserFlowState>,
    },
    Finished,
}

impl UserFlowApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        base_url: Url,
        result: Rc<RefCell<Option<std::result::Result<UserFlowProfile, String>>>>,
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

        if state.started_at.elapsed() > USER_FLOW_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("user flow selftest timed out after {USER_FLOW_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let phase = state.runtime.borrow().phase;
        match phase {
            Phase::WaitHumanLoginPage => {
                let Some(human) = tab(&state, TabId(1)) else {
                    return;
                };
                if human.webview.load_status() == LoadStatus::Complete
                    && human.webview.page_title().as_deref() == Some("Login")
                {
                    eval_no_result(&human.webview, HUMAN_LOGIN_JS);
                    state.runtime.borrow_mut().phase = Phase::SubmitHumanLogin;
                }
            }
            Phase::SubmitHumanLogin => {
                let Some(human) = tab(&state, TabId(1)) else {
                    return;
                };
                if human.webview.load_status() == LoadStatus::Complete
                    && human.webview.page_title().as_deref() == Some("Dashboard")
                {
                    request_probe(&state.pending_human_probe, &human.webview, DONE_PROBE_JS);
                    state.runtime.borrow_mut().phase = Phase::ProbeHuman;
                }
            }
            Phase::ProbeHuman => {
                let Some(raw) = finish_probe(&state.pending_human_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&raw) else {
                    finish_err(&state, event_loop, "failed to parse human probe".into());
                    *self = Self::Finished;
                    return;
                };
                if !probe_text(&probe).contains("LOGGED_IN") || !probe_handoff_done(&probe) {
                    finish_err(
                        &state,
                        event_loop,
                        format!("human login or handoff failed: {probe:?}"),
                    );
                    *self = Self::Finished;
                    return;
                }
                state.runtime.borrow_mut().human_probe = Some(probe);
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                match state.base_url.join("user_flow.html") {
                    Ok(url) => agent.webview.load(url),
                    Err(error) => {
                        finish_err(
                            &state,
                            event_loop,
                            format!("failed to build user flow URL: {error}"),
                        );
                        *self = Self::Finished;
                        return;
                    }
                }
                state.runtime.borrow_mut().phase = Phase::WaitAgentPage;
            }
            Phase::WaitAgentPage => {
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                if agent.webview.load_status() == LoadStatus::Complete
                    && agent.webview.page_title().as_deref() == Some("User Flow Fixture")
                {
                    request_probe(&state.pending_agent_probe, &agent.webview, AGENT_ROUND1_JS);
                    state.runtime.borrow_mut().phase = Phase::AgentRound1;
                }
            }
            Phase::AgentRound1 => {
                let Some(raw) = finish_probe(&state.pending_agent_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&raw) else {
                    finish_err(
                        &state,
                        event_loop,
                        "failed to parse agent round1 probe".into(),
                    );
                    *self = Self::Finished;
                    return;
                };
                state.runtime.borrow_mut().round1_probe = Some(probe);
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                request_probe(&state.pending_agent_probe, &agent.webview, USER_STEP_JS);
                state.runtime.borrow_mut().phase = Phase::UserStep;
            }
            Phase::UserStep => {
                let Some(raw) = finish_probe(&state.pending_agent_probe) else {
                    return;
                };
                let Ok(probe) = parse_probe(&raw) else {
                    finish_err(&state, event_loop, "failed to parse user step probe".into());
                    *self = Self::Finished;
                    return;
                };
                state.runtime.borrow_mut().user_probe = Some(probe);
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                request_probe(&state.pending_agent_probe, &agent.webview, AGENT_ROUND2_JS);
                state.runtime.borrow_mut().phase = Phase::AgentRound2;
            }
            Phase::AgentRound2 => {
                let Some(raw) = finish_probe(&state.pending_agent_probe) else {
                    return;
                };
                let Ok(final_probe) = parse_probe(&raw) else {
                    finish_err(
                        &state,
                        event_loop,
                        "failed to parse agent round2 probe".into(),
                    );
                    *self = Self::Finished;
                    return;
                };
                let profile = build_profile(&state, &final_probe);
                if !profile.human_login
                    || !profile.handoff_done
                    || !profile.agent_session
                    || !profile.agent_input_to_human_tab_blocked
                    || !profile.read_policy_enforced
                    || profile.agent_round1_filled < 4
                    || !profile.user_can_see_agent_values
                    || profile.round1_sensitive_requires_user_input < 2
                    || !profile.user_page_change_seen
                    || !profile.user_normal_value_checked
                    || !profile.user_sensitive_status_checked_without_value
                    || profile.agent_completed_remaining < 2
                    || !profile.agent_preserved_user_values
                    || !profile.same_agent_tab_continued
                    || profile.final_sensitive_completed_without_value < 3
                    || profile.agent_sensitive_values_exposed
                {
                    finish_err(
                        &state,
                        event_loop,
                        format!("user flow selftest failed: {profile:?}"),
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

impl ApplicationHandler<WakerEvent> for UserFlowApp {
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
                .with_title("Saccade User Flow Selftest")
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

        let state = Rc::new(UserFlowState {
            window,
            servo,
            rendering_context,
            base_url: base_url.clone(),
            started_at: Instant::now(),
            tabs: RefCell::new(Vec::new()),
            runtime: RefCell::new(Runtime {
                phase: Phase::WaitHumanLoginPage,
                human_probe: None,
                round1_probe: None,
                user_probe: None,
            }),
            pending_human_probe: Rc::new(RefCell::new(None)),
            pending_agent_probe: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let human_url = match state.base_url.join("login.html") {
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
                        "window closed before user flow selftest finished".into(),
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

fn tab(state: &Rc<UserFlowState>, tab_id: TabId) -> Option<TabRuntime> {
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

fn request_probe(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
    webview: &WebView,
    script: &'static str,
) {
    *pending.borrow_mut() = None;
    let pending = pending.clone();
    webview.evaluate_javascript(script, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn eval_no_result(webview: &WebView, script: &'static str) {
    webview.evaluate_javascript(script, |_| {});
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

fn probe_handoff_done(probe: &Value) -> bool {
    probe
        .get("handoffDone")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn build_profile(state: &Rc<UserFlowState>, final_probe: &Value) -> UserFlowProfile {
    let runtime = state.runtime.borrow();
    let human_probe = runtime.human_probe.clone().unwrap_or(Value::Null);
    let round1_probe = runtime.round1_probe.clone().unwrap_or(Value::Null);
    let user_probe = runtime.user_probe.clone().unwrap_or(Value::Null);
    let tabs = state.tabs.borrow();
    let human = tabs.iter().find(|tab| tab.info.tab_id == TabId(1));
    let agent = tabs.iter().find(|tab| tab.info.tab_id == TabId(2));

    UserFlowProfile {
        human_login: probe_text(&human_probe).contains("LOGGED_IN"),
        handoff_done: probe_handoff_done(&human_probe),
        agent_session: probe_text(final_probe).contains("LOGGED_IN"),
        agent_input_to_human_tab_blocked: human.is_some_and(|tab| !tab.info.agent_input_allowed()),
        read_policy_enforced: human.is_some_and(|tab| !tab.info.agent_truth_allowed())
            && agent.is_some_and(|tab| tab.info.agent_truth_allowed()),
        agent_round1_filled: number(&round1_probe, "agentRound1Filled"),
        user_can_see_agent_values: boolean(&round1_probe, "userCanSeeAgentValues"),
        round1_sensitive_completed_without_value: number(
            &round1_probe,
            "sensitiveCompletedWithoutValue",
        ),
        round1_sensitive_requires_user_input: number(&round1_probe, "sensitiveRequiresUserInput"),
        user_page_change_seen: boolean(&user_probe, "userPageChangeSeen")
            && boolean(final_probe, "userPageChangeSeen"),
        user_normal_value_checked: boolean(final_probe, "userNormalValueChecked"),
        user_sensitive_status_checked_without_value: boolean(
            final_probe,
            "userSensitiveStatusCheckedWithoutValue",
        ),
        agent_completed_remaining: number(final_probe, "agentCompletedRemaining"),
        agent_preserved_user_values: boolean(final_probe, "agentPreservedUserValues"),
        same_agent_tab_continued: number(final_probe, "revision") >= 3
            && boolean(final_probe, "sameAgentTabContinued"),
        final_sensitive_completed_without_value: number(
            final_probe,
            "sensitiveCompletedWithoutValue",
        ),
        agent_sensitive_values_exposed: boolean(final_probe, "agentSensitiveValuesExposed"),
    }
}

fn number(probe: &Value, key: &str) -> usize {
    probe
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0)
}

fn boolean(probe: &Value, key: &str) -> bool {
    probe.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn finish_ok(state: &Rc<UserFlowState>, event_loop: &ActiveEventLoop, profile: UserFlowProfile) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(profile));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<UserFlowState>, event_loop: &ActiveEventLoop, message: String) {
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

const HUMAN_LOGIN_JS: &str = r#"
(() => {
  const username = document.getElementById("username");
  const password = document.getElementById("password");
  const otp = document.getElementById("otp");
  if (username) username.value = "wayne";
  if (password) password.value = "human-only-password";
  if (otp) otp.value = "123456";
  const form = document.getElementById("login-form");
  if (form && form.requestSubmit) {
    form.requestSubmit();
  } else if (form) {
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  }
})()
"#;

const DONE_PROBE_JS: &str = r#"
(() => {
  const done = document.getElementById("handoff-done");
  if (done) done.click();
  return JSON.stringify({
    title: document.title,
    text: document.body ? document.body.innerText : "",
    handoffDone: localStorage.getItem("saccade_handoff_done") === "true"
  });
})()
"#;

macro_rules! agent_helpers_js {
    () => {
        r#"
function flowFields() {
  return Array.from(document.querySelectorAll("[data-saccade-field]"));
}
function fieldValue(el) {
  return el.type === "checkbox" ? String(el.checked) : (el.value || "");
}
function maskedFields() {
  return flowFields().map((el) => {
    const owner = (el.getAttribute("data-owner") || "").toLowerCase();
    const sensitivity = (el.getAttribute("data-sensitive") || "none").toLowerCase();
    const raw = fieldValue(el);
    const sensitive = owner === "human" || sensitivity !== "none";
    if (sensitive) {
      return {
        id: el.id || "",
        owner,
        sensitivity,
        value: null,
        masked: true,
        value_state: raw && raw !== "false" ? "completed_without_value" : "requires_user_input",
        format_valid: validateSensitive(el.id || "", raw)
      };
    }
    return {
      id: el.id || "",
      owner,
      sensitivity,
      value: raw,
      masked: false,
      value_state: "agent_visible",
      format_valid: true
    };
  });
}
function validateSensitive(id, raw) {
  if (!raw) return false;
  if (id === "ssn") return /^\d{3}-\d{2}-\d{4}$/.test(raw);
  if (id === "tax-id-empty") return /^\d{2}-\d{7}$/.test(raw);
  if (id === "signature") return raw.trim().length >= 3;
  return true;
}
function sensitiveCompleted(fields) {
  return fields.filter((f) => f.masked && f.value_state === "completed_without_value").length;
}
function sensitiveRequiresUser(fields) {
  return fields.filter((f) => f.masked && f.value_state === "requires_user_input").length;
}
function sensitiveValuesExposed(fields) {
  return fields.some((f) => f.masked && f.value !== null);
}
function bumpRevision() {
  const next = Number(localStorage.getItem("saccade_user_flow_revision") || "0") + 1;
  localStorage.setItem("saccade_user_flow_revision", String(next));
  return next;
}
"#
    };
}

const AGENT_ROUND1_JS: &str = concat!(
    r#"
(() => {
"#,
    agent_helpers_js!(),
    r#"
  const values = {
    "task-1": "agent-task-1",
    "task-2": "agent-task-2",
    "task-3": "agent-task-3",
    "task-4": "agent-task-4"
  };
  for (const [id, value] of Object.entries(values)) {
    const field = document.getElementById(id);
    if (field) field.value = value;
  }
  const fields = maskedFields();
  const revision = bumpRevision();
  return JSON.stringify({
    text: document.body ? document.body.innerText : "",
    revision,
    agentRound1Filled: Object.keys(values).filter((id) => document.getElementById(id)?.value === values[id]).length,
    userCanSeeAgentValues: Object.keys(values).every((id) => document.getElementById(id)?.value === values[id]),
    sensitiveCompletedWithoutValue: sensitiveCompleted(fields),
    sensitiveRequiresUserInput: sensitiveRequiresUser(fields),
    agentSensitiveValuesExposed: sensitiveValuesExposed(fields),
    fields
  });
})()
"#
);

const USER_STEP_JS: &str = r#"
(() => {
  const next = document.getElementById("next-page");
  if (next) next.click();
  const quantity = document.getElementById("user-quantity");
  const tax = document.getElementById("tax-id-empty");
  const signature = document.getElementById("signature");
  if (quantity) quantity.value = "17";
  if (tax) tax.value = "12-3456789";
  if (signature) signature.value = "Wayne Ma";
  localStorage.setItem("saccade_user_reviewed_agent_values", "true");
  const nextRevision = Number(localStorage.getItem("saccade_user_flow_revision") || "0") + 1;
  localStorage.setItem("saccade_user_flow_revision", String(nextRevision));
  return JSON.stringify({
    text: document.body ? document.body.innerText : "",
    revision: nextRevision,
    userPageChangeSeen: localStorage.getItem("saccade_user_flow_page") === "2",
    userReviewedAgentValues: localStorage.getItem("saccade_user_reviewed_agent_values") === "true"
  });
})()
"#;

const AGENT_ROUND2_JS: &str = concat!(
    r#"
(() => {
"#,
    agent_helpers_js!(),
    r#"
  const beforeQuantity = document.getElementById("user-quantity")?.value || "";
  const beforeTax = document.getElementById("tax-id-empty")?.value || "";
  const beforeSignature = document.getElementById("signature")?.value || "";
  const code = document.getElementById("agent-page2-code");
  const owner = document.getElementById("agent-page2-owner");
  if (code) code.value = "agent-page2-ready";
  if (owner) owner.value = "agent-owned";
  const afterQuantity = document.getElementById("user-quantity")?.value || "";
  const afterTax = document.getElementById("tax-id-empty")?.value || "";
  const afterSignature = document.getElementById("signature")?.value || "";
  const fields = maskedFields();
  const revision = bumpRevision();
  const userNormalValueChecked = afterQuantity === "17";
  const taxField = fields.find((field) => field.id === "tax-id-empty");
  const signatureField = fields.find((field) => field.id === "signature");
  return JSON.stringify({
    text: document.body ? document.body.innerText : "",
    revision,
    sameAgentTabContinued: true,
    userPageChangeSeen: localStorage.getItem("saccade_user_flow_page") === "2",
    userNormalValueChecked,
    userSensitiveStatusCheckedWithoutValue:
      Boolean(taxField && taxField.masked && taxField.value === null && taxField.value_state === "completed_without_value" && taxField.format_valid) &&
      Boolean(signatureField && signatureField.masked && signatureField.value === null && signatureField.value_state === "completed_without_value" && signatureField.format_valid),
    agentCompletedRemaining: Number(code?.value === "agent-page2-ready") + Number(owner?.value === "agent-owned"),
    agentPreservedUserValues: beforeQuantity === afterQuantity && beforeTax === afterTax && beforeSignature === afterSignature,
    sensitiveCompletedWithoutValue: sensitiveCompleted(fields),
    sensitiveRequiresUserInput: sensitiveRequiresUser(fields),
    agentSensitiveValuesExposed: sensitiveValuesExposed(fields),
    fields
  });
})()
"#
);
