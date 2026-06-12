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
    CSSPixel, InputEvent, JSValue, LoadStatus, MouseButton, MouseButtonAction, MouseButtonEvent,
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

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const ACT_VERIFY_DELAY: Duration = Duration::from_millis(160);

pub fn run_browser_session_worker(url: Url) -> Result<()> {
    let artifacts = WorkerArtifacts::new()?;
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let (tx, rx) = mpsc::channel();
    let reader_proxy = event_loop.create_proxy();
    thread::spawn(move || read_commands(tx, reader_proxy));

    let mut app = WorkerApp::new(&event_loop, url, rx, artifacts);
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
        let run_id = format!("worker_{}", unix_ms()?);
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
}

#[derive(Clone, Copy)]
enum ProbeMethod {
    Truth,
    Actions,
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
    page_revision: Cell<u64>,
    pending_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    latest_probe: RefCell<Option<Value>>,
    current: RefCell<Option<ActiveRequest>>,
    stdout: RefCell<io::Stdout>,
}

impl WebViewDelegate for WorkerState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum WorkerApp {
    Initial {
        waker: Waker,
        target_url: Url,
        command_rx: Option<Receiver<WorkerInput>>,
        artifacts: Option<WorkerArtifacts>,
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
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            target_url,
            command_rx: Some(command_rx),
            artifacts: Some(artifacts),
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
        if webview.load_status() == LoadStatus::Complete {
            state.loaded_once.set(true);
        }
        if !state.loaded_once.get() {
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
            event_loop.exit();
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
        } = self
        else {
            return;
        };

        let display_handle = match event_loop.display_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("browser session worker display handle error: {error}");
                event_loop.exit();
                return;
            }
        };
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Saccade Browser Session Worker")
                .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
        ) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("browser session worker window error: {error}");
                event_loop.exit();
                return;
            }
        };
        let window_handle = match window.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("browser session worker window handle error: {error}");
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
                eprintln!("browser session worker rendering context error: {error:?}");
                event_loop.exit();
                return;
            }
        };
        if let Err(error) = rendering_context.make_current() {
            eprintln!("browser session worker GL context error: {error:?}");
            event_loop.exit();
            return;
        }

        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(waker.clone()))
            .build();
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
            page_revision: Cell::new(1),
            pending_probe: Rc::new(RefCell::new(None)),
            latest_probe: RefCell::new(None),
            current: RefCell::new(None),
            stdout: RefCell::new(io::stdout()),
        });
        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.target_url.clone())
            .hidpi_scale_factor(Scale::new(1.0))
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
                    event_loop.exit();
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
                "url": state.target_url.as_str(),
                "page_revision": state.page_revision.get(),
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
        "act" => start_act_request(state, webview, id, request.params),
        "close" => {
            respond_ok(
                state,
                id,
                json!({
                    "status": "ok",
                    "summary": "browser session worker closing",
                }),
            );
            event_loop.exit();
        }
        other => respond_error(state, id, format!("unknown worker method {other:?}")),
    }
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

fn process_current(state: &Rc<WorkerState>, webview: &WebView, _event_loop: &ActiveEventLoop) {
    let Some(current) = state.current.borrow_mut().take() else {
        return;
    };
    match current {
        ActiveRequest::Probe { id, method } => match finish_probe(&state.pending_probe) {
            Some(Ok(probe)) => match serde_json::from_str::<Value>(&probe) {
                Ok(value) => {
                    *state.latest_probe.borrow_mut() = Some(value.clone());
                    respond_ok(state, id, probe_response(state, &value, method));
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
    }
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

fn probe_response(state: &Rc<WorkerState>, probe: &Value, method: ProbeMethod) -> Value {
    let actions = action_map(probe);
    let kind = match method {
        ProbeMethod::Truth => "truth_collected",
        ProbeMethod::Actions => "actions_collected",
    };
    log_replay(
        state,
        json!({
            "kind": kind,
            "run_id": state.run_id.as_str(),
            "page_revision": state.page_revision.get(),
            "actions_seen": actions.len(),
            "sensitive_fields": sensitive_action_count(&actions),
            "body_text_length": probe.get("bodyTextLength").and_then(Value::as_u64).unwrap_or(0),
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": match method {
            ProbeMethod::Truth => "browser session truth collected from live Servo tab",
            ProbeMethod::Actions => "browser session action map collected from live Servo tab",
        },
        "url": probe.get("url").cloned().unwrap_or_else(|| json!(state.target_url.as_str())),
        "title": probe.get("title").cloned().unwrap_or(Value::Null),
        "page_revision": state.page_revision.get(),
        "dom_page_revision": probe.get("pageRevision").cloned().unwrap_or(Value::Null),
        "actions": actions,
        "truth": {
            "body_text_length": probe.get("bodyTextLength").cloned().unwrap_or(Value::Null),
            "body_child_count": probe.get("bodyChildCount").cloned().unwrap_or(Value::Null),
            "viewport": probe.get("viewport").cloned().unwrap_or(Value::Null),
            "sensitive_fields": sensitive_action_count(&actions),
        },
        "artifacts": artifact_paths(state),
    })
}

fn act_response(
    state: &Rc<WorkerState>,
    action_id: String,
    before_revision: u64,
    after_revision: u64,
    before_probe: &Value,
    after_probe: &Value,
) -> Value {
    let changed = probe_changed(before_probe, after_probe);
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
        }),
    );
    json!({
        "status": "ok",
        "runtime": "browser_session_worker_v0",
        "engine": "saccade-browser-session-worker-v0",
        "summary": "action dispatched through live Servo browser session",
        "url": after_probe.get("url").cloned().unwrap_or_else(|| json!(state.target_url.as_str())),
        "title": after_probe.get("title").cloned().unwrap_or(Value::Null),
        "page_revision": after_revision,
        "actions": action_map(after_probe),
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

fn probe_changed(before_probe: &Value, after_probe: &Value) -> bool {
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

fn action_map(probe: &Value) -> Vec<Value> {
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

fn sensitive_action_count(actions: &[Value]) -> usize {
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

fn action_labels(actions: &[Value]) -> Vec<String> {
    actions.iter().map(probe_action_label).collect()
}

fn action_id_for(action: &Value) -> String {
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

fn action_enabled(action: &Value) -> bool {
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
    })
}

fn write_report(state: &Rc<WorkerState>, latest: &Value) {
    let report = json!({
        "run_id": state.run_id.as_str(),
        "engine": "saccade-browser-session-worker-v0",
        "url": state.target_url.as_str(),
        "page_revision": state.page_revision.get(),
        "latest": latest,
        "artifacts": artifact_paths(state),
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
        let _ = fs::write(&state.report_path, bytes);
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

const PROBE_JS: &str = r##"
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
    actions
  });
})()
"##;
