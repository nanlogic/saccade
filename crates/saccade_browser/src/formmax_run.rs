use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
use serde::{Deserialize, Serialize};
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
const WINDOW_HEIGHT: u32 = 900;
const FORMMAX_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormmaxRunReport {
    pub engine: String,
    pub rows: usize,
    pub pages: usize,
    pub filled: usize,
    pub blocked_sensitive: usize,
    pub receipt_verified: bool,
    pub validation_errors: usize,
    pub replay_events: usize,
    pub events: Vec<Value>,
    pub receipt: Value,
}

pub fn run_formmax_fixture(url: Url) -> Result<FormmaxRunReport> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = FormmaxApp::new(&event_loop, url, result.clone());

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
    DriveRequested,
    Done,
}

struct FormmaxState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webview: RefCell<Option<WebView>>,
    target_url: Url,
    started_at: Instant,
    phase: Cell<Phase>,
    pending_drive: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    result: Rc<RefCell<Option<std::result::Result<FormmaxRunReport, String>>>>,
}

impl WebViewDelegate for FormmaxState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum FormmaxApp {
    Initial {
        waker: Waker,
        target_url: Url,
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
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<FormmaxRunReport, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            target_url,
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
                format!("FORMMAX runner timed out after {FORMMAX_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webview.borrow().clone() else {
            return;
        };

        match state.phase.get() {
            Phase::Load if webview.load_status() == LoadStatus::Complete => {
                request_drive(&state, &webview);
                state.phase.set(Phase::DriveRequested);
            }
            Phase::DriveRequested => {
                let Some(raw) = finish_drive(&state.pending_drive) else {
                    return;
                };
                match serde_json::from_str::<FormmaxRunReport>(&raw) {
                    Ok(report) => {
                        finish_ok(&state, event_loop, report);
                        state.phase.set(Phase::Done);
                        *self = Self::Finished;
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
            target_url,
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
            .event_loop_waker(Box::new(waker.clone()))
            .build();
        servo.setup_logging();

        let state = Rc::new(FormmaxState {
            window,
            servo,
            rendering_context,
            webview: RefCell::new(None),
            target_url: target_url.clone(),
            started_at: Instant::now(),
            phase: Cell::new(Phase::Load),
            pending_drive: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.target_url.clone())
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

fn finish_drive(
    pending: &Rc<RefCell<Option<std::result::Result<String, String>>>>,
) -> Option<String> {
    pending
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
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
