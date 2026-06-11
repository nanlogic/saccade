use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use euclid::Scale;
use servo::{
    DeviceIntRect, DeviceIntSize, LoadStatus, RenderingContext, Servo, ServoBuilder, WebView,
    WebViewBuilder, WebViewDelegate, WindowRenderingContext,
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
const BOOT_TIMEOUT: Duration = Duration::from_secs(20);

pub fn selftest_boot(calibration_url: Url) -> Result<String> {
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let result = Rc::new(RefCell::new(None));
    let mut app = BootApp::new(&event_loop, calibration_url, result.clone());

    event_loop
        .run_app(&mut app)
        .context("winit event loop failed")?;

    match result.borrow_mut().take() {
        Some(Ok(title)) => Ok(title),
        Some(Err(message)) => bail!(message),
        None => bail!("boot exited without a result"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootPhase {
    AboutBlank,
    Calibration,
    RenderReadback,
    Done,
}

struct AppState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    webviews: RefCell<Vec<WebView>>,
    target_url: Url,
    phase: Cell<BootPhase>,
    new_frame_ready: Cell<bool>,
    result: Rc<RefCell<Option<std::result::Result<String, String>>>>,
}

impl WebViewDelegate for AppState {
    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.new_frame_ready.set(true);
        self.window.request_redraw();
    }
}

enum BootApp {
    Initial {
        waker: Waker,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<String, String>>>>,
        started_at: Instant,
    },
    Running {
        state: Rc<AppState>,
        started_at: Instant,
    },
    Finished,
}

impl BootApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        target_url: Url,
        result: Rc<RefCell<Option<std::result::Result<String, String>>>>,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            target_url,
            result,
            started_at: Instant::now(),
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Running { state, started_at } = self else {
            return;
        };

        if started_at.elapsed() > BOOT_TIMEOUT {
            finish_with(
                state,
                event_loop,
                Err(format!("boot timed out after {:?}", BOOT_TIMEOUT)),
            );
            *self = Self::Finished;
            return;
        }

        let Some(webview) = state.webviews.borrow().last().cloned() else {
            return;
        };

        match state.phase.get() {
            BootPhase::AboutBlank if webview.load_status() == LoadStatus::Complete => {
                state.phase.set(BootPhase::Calibration);
                webview.load(state.target_url.clone());
            }
            BootPhase::Calibration if webview.load_status() == LoadStatus::Complete => {
                state.phase.set(BootPhase::RenderReadback);
                state.new_frame_ready.set(false);
                webview.paint();
                state.window.request_redraw();
            }
            BootPhase::RenderReadback => {
                if !state.new_frame_ready.get() {
                    state.window.request_redraw();
                    return;
                }

                webview.paint();
                let rect = DeviceIntRect::from_size(DeviceIntSize::new(
                    WINDOW_WIDTH as i32,
                    WINDOW_HEIGHT as i32,
                ));
                let Some(image) = state.rendering_context.read_to_image(rect) else {
                    state.window.request_redraw();
                    return;
                };
                if image.width() == 0 || image.height() == 0 {
                    finish_with(
                        state,
                        event_loop,
                        Err("readback returned an empty image".into()),
                    );
                    *self = Self::Finished;
                    return;
                }

                let title = webview.page_title().unwrap_or_default();
                if title.is_empty() {
                    state.window.request_redraw();
                    return;
                }

                finish_with(state, event_loop, Ok(title));
                state.phase.set(BootPhase::Done);
                *self = Self::Finished;
            }
            _ => {}
        }
    }
}

impl ApplicationHandler<WakerEvent> for BootApp {
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
                .with_title("Saccade M0")
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

        let state = Rc::new(AppState {
            window,
            servo,
            rendering_context,
            webviews: RefCell::new(Vec::new()),
            target_url: target_url.clone(),
            phase: Cell::new(BootPhase::AboutBlank),
            new_frame_ready: Cell::new(false),
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
        if let Self::Running { state, .. } = self {
            state.servo.spin_event_loop();

            match event {
                WindowEvent::CloseRequested => {
                    finish_with(
                        state,
                        event_loop,
                        Err("window closed before boot finished".into()),
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
                    if let Some(webview) = state.webviews.borrow().last() {
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

fn finish_with(
    state: &Rc<AppState>,
    event_loop: &ActiveEventLoop,
    result: std::result::Result<String, String>,
) {
    if state.result.borrow().is_none() {
        *state.result.borrow_mut() = Some(result);
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
