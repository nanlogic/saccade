use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
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
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

pub fn devmax_probe(url: Url) -> Result<Value> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = ProbeApp::new(&event_loop, url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(value)) => Ok(value),
        Some(Err(message)) => bail!(message),
        None => bail!("DEVMAX probe exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    ProbeRequested,
    Done,
}

struct ProbeState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    target_url: Url,
    started_at: Instant,
    phase: Cell<Phase>,
    pending_probe: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    result: Rc<RefCell<Option<std::result::Result<Value, String>>>>,
}

impl WebViewDelegate for ProbeState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }
}

enum ProbeApp {
    Initial {
        waker: Waker,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<Value, String>>>>,
    },
    Running {
        state: Rc<ProbeState>,
    },
    Finished,
}

impl ProbeApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<Value, String>>>>,
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

        if state.started_at.elapsed() > PROBE_TIMEOUT {
            finish_err(
                &state,
                event_loop,
                format!("DEVMAX probe timed out after {PROBE_TIMEOUT:?}"),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

        match state.phase.get() {
            Phase::Load if webview.load_status() == LoadStatus::Complete => {
                request_probe(&state, &webview);
                state.phase.set(Phase::ProbeRequested);
            }
            Phase::ProbeRequested => {
                let Some(probe) = finish_probe(&state.pending_probe) else {
                    return;
                };
                match serde_json::from_str(&probe) {
                    Ok(value) => finish_ok(&state, event_loop, value),
                    Err(error) => finish_err(
                        &state,
                        event_loop,
                        format!("failed to parse DEVMAX probe JSON: {error}; raw={probe:?}"),
                    ),
                }
                state.phase.set(Phase::Done);
                *self = Self::Finished;
            }
            Phase::Done => {}
            _ => {}
        }

        state.window.request_redraw();
    }
}

impl ApplicationHandler<WakerEvent> for ProbeApp {
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
                .with_title("Saccade DEVMAX Probe")
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

        let state = Rc::new(ProbeState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            target_url: target_url.clone(),
            started_at: Instant::now(),
            phase: Cell::new(Phase::Load),
            pending_probe: Rc::new(RefCell::new(None)),
            result: result.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.target_url.clone())
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
        if let Self::Running { state } = self {
            state.servo.spin_event_loop();
            match event {
                WindowEvent::CloseRequested => {
                    finish_err(
                        state,
                        event_loop,
                        "window closed before DEVMAX probe finished",
                    );
                    *self = Self::Finished;
                }
                WindowEvent::RedrawRequested => {
                    if let Some(webview) = state.webviews.borrow().last() {
                        webview.paint();
                        state.rendering_context.present();
                    }
                }
                WindowEvent::Resized(new_size) => {
                    state.rendering_context.resize(new_size);
                    for webview in state.webviews.borrow().iter() {
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

fn request_probe(state: &Rc<ProbeState>, webview: &WebView) {
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
) -> Option<String> {
    pending
        .borrow_mut()
        .take()
        .map(|result| result.unwrap_or_else(|error| format!("ERROR {error}")))
}

fn finish_ok(state: &Rc<ProbeState>, event_loop: &ActiveEventLoop, value: Value) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(Ok(value));
    }
    event_loop.exit();
}

fn finish_err(state: &Rc<ProbeState>, event_loop: &ActiveEventLoop, message: impl Into<String>) {
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

const PROBE_JS: &str = r##"
(() => {
  const viewport = { width: window.innerWidth || 0, height: window.innerHeight || 0 };
  const body = document.body;
  const bodyText = body ? (body.innerText || body.textContent || "").trim() : "";

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

  function colorEqual(a, b) {
    return a && b && a === b && a !== "rgba(0, 0, 0, 0)";
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
    const action = {
      index,
      label: (el.innerText || el.value || el.getAttribute("aria-label") || el.getAttribute("href") || el.tagName).trim(),
      tag: el.tagName.toLowerCase(),
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      rect,
      offscreen: offscreen(rect),
      visible: visibleRect(rect) && style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0",
      blockedBy: null
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

  const invisibleText = [];
  for (const el of elements) {
    const text = (el.innerText || el.textContent || "").trim().replace(/\s+/g, " ");
    if (!text) continue;
    const style = getComputedStyle(el);
    const rect = rectOf(el);
    if (!visibleRect(rect) || style.display === "none" || style.visibility === "hidden" || style.opacity === "0") continue;
    if (colorEqual(style.color, style.backgroundColor)) {
      invisibleText.push({
        text: text.slice(0, 80),
        selector: label(el),
        color: style.color,
        backgroundColor: style.backgroundColor,
        rect
      });
    }
  }

  return JSON.stringify({
    engine: "servo-rendered-probe-v0",
    title: document.title || "",
    url: location.href,
    viewport,
    bodyTextLength: bodyText.length,
    bodyChildCount: body ? body.children.length : 0,
    blankPage: bodyText.length === 0 && (!body || body.children.length === 0),
    actions,
    invisibleText
  });
})()
"##;
