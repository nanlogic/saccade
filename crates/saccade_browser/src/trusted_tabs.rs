use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde_json::{Value, json};
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
    pub otp_exposed: bool,
    pub agent_input_to_human_tab_blocked: bool,
    pub done_clicked: bool,
    pub human_can_see_agent_values: bool,
    pub agent_can_see_agent_values: bool,
    pub agent_ssn_exposed: bool,
    pub agent_government_id_exposed: bool,
    pub agent_credit_card_exposed: bool,
    pub agent_user_password_exposed: bool,
    pub masked_sensitive_fields: usize,
    pub sensitive_completed_without_value: usize,
    pub sensitive_requires_user_input: usize,
    pub agent_knows_sensitive_field_status: bool,
}

pub fn selftest_trusted_tabs(base_url: Url) -> Result<TrustedTabsProfile> {
    run_tabs_selftest(base_url, TabsMode::TrustedTabs)
}

pub fn selftest_login_handoff(base_url: Url) -> Result<TrustedTabsProfile> {
    run_tabs_selftest(base_url, TabsMode::LoginHandoff)
}

pub fn selftest_safety(base_url: Url) -> Result<TrustedTabsProfile> {
    run_tabs_selftest(base_url, TabsMode::Safety)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabsMode {
    TrustedTabs,
    LoginHandoff,
    Safety,
}

fn run_tabs_selftest(base_url: Url, mode: TabsMode) -> Result<TrustedTabsProfile> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = TabsApp::new(&event_loop, base_url, mode, result.clone());

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
    WaitHumanLoginPage,
    SubmitHumanLogin,
    WaitHumanDashboard,
    ProbeHuman,
    WaitAgentPage,
    ProbeAgent,
    Done,
}

struct TabRuntime {
    info: TabInfo,
    webview: WebView,
}

struct Runtime {
    mode: TabsMode,
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
        mode: TabsMode,
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
        mode: TabsMode,
        result: Rc<RefCell<Option<std::result::Result<TrustedTabsProfile, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            base_url,
            mode,
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
            Phase::WaitHumanLoginPage => {
                let Some(human) = tab(&state, TabId(1)) else {
                    return;
                };
                if human.webview.load_status() == LoadStatus::Complete
                    && human.webview.page_title().as_deref() == Some("Login")
                {
                    submit_human_login(&human.webview);
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
                    request_handoff_done_probe(&state, &human.webview);
                    let mut runtime = state.runtime.borrow_mut();
                    runtime.phase = Phase::ProbeHuman;
                    runtime.human_probe_requested = true;
                }
            }
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
                if matches!(
                    state.runtime.borrow().mode,
                    TabsMode::LoginHandoff | TabsMode::Safety
                ) && !probe_handoff_done(&probe)
                {
                    finish_err(
                        &state,
                        event_loop,
                        "human tab did not click Done for handoff".into(),
                    );
                    *self = Self::Finished;
                    return;
                }
                state.runtime.borrow_mut().human_probe = Some(probe);
                let Some(agent) = tab(&state, TabId(2)) else {
                    return;
                };
                let agent_path = match state.runtime.borrow().mode {
                    TabsMode::Safety => "safety.html",
                    TabsMode::TrustedTabs | TabsMode::LoginHandoff => "dashboard.html",
                };
                match state.base_url.join(agent_path) {
                    Ok(url) => agent.webview.load(url),
                    Err(error) => {
                        finish_err(
                            &state,
                            event_loop,
                            format!("failed to build agent URL: {error}"),
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
                let expected_title = match state.runtime.borrow().mode {
                    TabsMode::Safety => "Safety Fixture",
                    TabsMode::TrustedTabs | TabsMode::LoginHandoff => "Dashboard",
                };
                if agent.webview.load_status() == LoadStatus::Complete
                    && agent.webview.page_title().as_deref() == Some(expected_title)
                {
                    if state.runtime.borrow().mode == TabsMode::Safety {
                        request_safety_probe(&state, &agent.webview);
                    } else {
                        request_agent_probe(&state, &agent.webview);
                    }
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
            mode,
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
                mode: *mode,
                phase: match *mode {
                    TabsMode::TrustedTabs => Phase::WaitHumanDashboard,
                    TabsMode::LoginHandoff | TabsMode::Safety => Phase::WaitHumanLoginPage,
                },
                human_probe_requested: false,
                agent_probe_requested: false,
                human_probe: None,
            }),
            pending_human_probe: Rc::new(RefCell::new(None)),
            pending_agent_probe: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let human_path = match *mode {
            TabsMode::TrustedTabs => "login.html?auto=1",
            TabsMode::LoginHandoff | TabsMode::Safety => "login.html",
        };
        let human_url = match state.base_url.join(human_path) {
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

fn request_safety_probe(state: &Rc<TabsState>, webview: &WebView) {
    *state.pending_agent_probe.borrow_mut() = None;
    let pending = state.pending_agent_probe.clone();
    webview.evaluate_javascript(SAFETY_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn request_handoff_done_probe(state: &Rc<TabsState>, webview: &WebView) {
    *state.pending_human_probe.borrow_mut() = None;
    let pending = state.pending_human_probe.clone();
    webview.evaluate_javascript(DONE_PROBE_JS, move |result| {
        *pending.borrow_mut() = Some(js_result_to_string(result));
    });
}

fn submit_human_login(webview: &WebView) {
    webview.evaluate_javascript(HUMAN_LOGIN_JS, |_| {});
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

fn probe_otp(probe: &Value) -> Option<String> {
    probe
        .get("otp")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn probe_handoff_done(probe: &Value) -> bool {
    probe
        .get("handoffDone")
        .and_then(Value::as_bool)
        .unwrap_or(false)
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
    let human_otp = probe_otp(&human_probe).unwrap_or_default();
    let agent_otp = probe_otp(agent_probe).unwrap_or_default();
    let safety = safety_visibility(agent_probe);

    TrustedTabsProfile {
        webviews: tabs.len(),
        cookie_shared: probe_cookie(agent_probe).contains("saccade_session=demo"),
        storage_shared: probe_storage(agent_probe) == "shared",
        input_isolated,
        read_policy_enforced,
        human_login: probe_text(&human_probe).contains("LOGGED_IN"),
        agent_session: probe_text(agent_probe).contains("LOGGED_IN"),
        password_exposed: !human_password.is_empty() || !agent_password.is_empty(),
        otp_exposed: !human_otp.is_empty() || !agent_otp.is_empty(),
        agent_input_to_human_tab_blocked: human.is_some_and(|tab| !tab.info.agent_input_allowed()),
        done_clicked: probe_handoff_done(&human_probe),
        human_can_see_agent_values: safety.human_can_see_agent_values,
        agent_can_see_agent_values: safety.agent_can_see_agent_values,
        agent_ssn_exposed: safety.agent_ssn_exposed,
        agent_government_id_exposed: safety.agent_government_id_exposed,
        agent_credit_card_exposed: safety.agent_credit_card_exposed,
        agent_user_password_exposed: safety.agent_user_password_exposed,
        masked_sensitive_fields: safety.masked_sensitive_fields,
        sensitive_completed_without_value: safety.sensitive_completed_without_value,
        sensitive_requires_user_input: safety.sensitive_requires_user_input,
        agent_knows_sensitive_field_status: safety.agent_knows_sensitive_field_status,
    }
}

#[derive(Default)]
struct SafetyVisibility {
    human_can_see_agent_values: bool,
    agent_can_see_agent_values: bool,
    agent_ssn_exposed: bool,
    agent_government_id_exposed: bool,
    agent_credit_card_exposed: bool,
    agent_user_password_exposed: bool,
    masked_sensitive_fields: usize,
    sensitive_completed_without_value: usize,
    sensitive_requires_user_input: usize,
    agent_knows_sensitive_field_status: bool,
}

fn safety_visibility(probe: &Value) -> SafetyVisibility {
    let fields = probe
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let human_truth = fields.clone();
    let agent_truth = fields.iter().map(mask_field_for_agent).collect::<Vec<_>>();

    SafetyVisibility {
        human_can_see_agent_values: truth_has_value(
            &human_truth,
            "agent-note",
            "agent-filled-note",
        ),
        agent_can_see_agent_values: truth_has_value(
            &agent_truth,
            "agent-note",
            "agent-filled-note",
        ),
        agent_ssn_exposed: truth_has_unmasked_sensitive(&agent_truth, "ssn"),
        agent_government_id_exposed: truth_has_unmasked_sensitive(&agent_truth, "government-id"),
        agent_credit_card_exposed: truth_has_unmasked_sensitive(&agent_truth, "credit-card"),
        agent_user_password_exposed: truth_has_unmasked_sensitive(&agent_truth, "user-password"),
        masked_sensitive_fields: agent_truth
            .iter()
            .filter(|field| {
                field
                    .get("masked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .count(),
        sensitive_completed_without_value: count_fields_with_status(
            &agent_truth,
            "completed_without_value",
        ),
        sensitive_requires_user_input: count_fields_with_status(
            &agent_truth,
            "requires_user_input",
        ),
        agent_knows_sensitive_field_status: truth_has_status(
            &agent_truth,
            "ssn",
            "completed_without_value",
        ) && truth_has_status(
            &agent_truth,
            "tax-id-empty",
            "requires_user_input",
        ),
    }
}

fn mask_field_for_agent(field: &Value) -> Value {
    let owner = field.get("owner").and_then(Value::as_str).unwrap_or("");
    let sensitive = field
        .get("sensitivity")
        .and_then(Value::as_str)
        .unwrap_or("");
    let should_mask = owner == "human" || sensitive != "none";
    if should_mask {
        let value_present = field
            .get("value")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty() && value != "false");
        let status = if value_present {
            "completed_without_value"
        } else {
            "requires_user_input"
        };
        json!({
            "id": field.get("id").cloned().unwrap_or(Value::Null),
            "label": field.get("label").cloned().unwrap_or(Value::Null),
            "owner": owner,
            "sensitivity": sensitive,
            "value": null,
            "masked": true,
            "value_state": status,
            "user_action_required": !value_present,
        })
    } else {
        let mut visible = field.clone();
        visible["masked"] = Value::Bool(false);
        visible["value_state"] = Value::String("agent_visible".into());
        visible["user_action_required"] = Value::Bool(false);
        visible
    }
}

fn truth_has_value(fields: &[Value], id: &str, expected: &str) -> bool {
    fields.iter().any(|field| {
        field.get("id").and_then(Value::as_str) == Some(id)
            && field.get("value").and_then(Value::as_str) == Some(expected)
    })
}

fn truth_has_unmasked_sensitive(fields: &[Value], id: &str) -> bool {
    fields.iter().any(|field| {
        field.get("id").and_then(Value::as_str) == Some(id)
            && field
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
    })
}

fn truth_has_status(fields: &[Value], id: &str, status: &str) -> bool {
    fields.iter().any(|field| {
        field.get("id").and_then(Value::as_str) == Some(id)
            && field.get("value_state").and_then(Value::as_str) == Some(status)
            && field.get("value").is_some_and(Value::is_null)
            && field
                .get("masked")
                .and_then(Value::as_bool)
                .unwrap_or(false)
    })
}

fn count_fields_with_status(fields: &[Value], status: &str) -> usize {
    fields
        .iter()
        .filter(|field| field.get("value_state").and_then(Value::as_str) == Some(status))
        .count()
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
  password: (document.querySelector('input[type="password"]') || {}).value || "",
  otp: (document.getElementById("otp") || {}).value || "",
  handoffDone: localStorage.getItem("saccade_handoff_done") === "true"
})
"#;

const DONE_PROBE_JS: &str = r#"
(() => {
  const done = document.getElementById("handoff-done");
  if (done) {
    done.click();
  }
  return JSON.stringify({
    title: document.title,
    url: location.href,
    text: document.body ? document.body.innerText : "",
    cookie: document.cookie,
    storage: localStorage.getItem("saccade_storage") || "",
    password: "",
    otp: "",
    handoffDone: localStorage.getItem("saccade_handoff_done") === "true"
  });
})()
"#;

const HUMAN_LOGIN_JS: &str = r#"
(() => {
  const username = document.getElementById("username");
  const password = document.getElementById("password");
  const otp = document.getElementById("otp");
  if (username) {
    username.value = "wayne";
  }
  if (password) {
    password.value = "human-only-password";
  }
  if (otp) {
    otp.value = "123456";
  }
  const form = document.getElementById("login-form");
  if (form) {
    if (form.requestSubmit) {
      form.requestSubmit();
    } else {
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    }
  }
})()
"#;

const SAFETY_PROBE_JS: &str = r#"
(() => {
  const note = document.getElementById("agent-note");
  if (note) {
    note.value = "agent-filled-note";
    localStorage.setItem("saccade_agent_note", note.value);
  }
  const confirmation = document.getElementById("agent-confirm");
  if (confirmation) {
    confirmation.checked = true;
  }

  function labelFor(id) {
    const label = document.querySelector(`label[for="${id}"]`);
    return label ? label.textContent.trim().replace(/\s+/g, " ") : id;
  }

  const fields = Array.from(document.querySelectorAll("[data-saccade-field]")).map((el) => ({
    id: el.id || "",
    label: labelFor(el.id || ""),
    owner: el.getAttribute("data-owner") || "",
    sensitivity: el.getAttribute("data-sensitive") || "none",
    value: el.type === "checkbox" ? String(el.checked) : (el.value || "")
  }));

  return JSON.stringify({
    title: document.title,
    url: location.href,
    text: document.body ? document.body.innerText : "",
    cookie: document.cookie,
    storage: localStorage.getItem("saccade_storage") || "",
    handoffDone: localStorage.getItem("saccade_handoff_done") === "true",
    fields
  });
})()
"#;
