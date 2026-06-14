use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use euclid::{Point2D, Scale};
use servo::{
    CSSPixel, EmbedderControl, InputEvent, Key as ServoKey, KeyState, KeyboardEvent, LoadStatus,
    MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent, NamedKey as ServoNamedKey,
    Opts, RenderingContext, SelectElement, SelectElementOptionOrOptgroup, Servo, ServoBuilder,
    WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WheelDelta, WheelEvent, WheelMode,
    WindowRenderingContext,
};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    ElementState, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::{RenderingProfile, RenderingProfileSettings};

const DEFAULT_WIDTH: u32 = 1440;
const DEFAULT_HEIGHT: u32 = 1000;

#[derive(Debug, Clone)]
pub struct DogfoodBrowserConfig {
    pub url: Url,
    pub width: u32,
    pub height: u32,
    pub auto_close_after: Option<Duration>,
    pub rendering_profile: Option<RenderingProfile>,
    pub profile_dir: Option<PathBuf>,
}

impl DogfoodBrowserConfig {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            auto_close_after: None,
            rendering_profile: None,
            profile_dir: None,
        }
    }
}

pub fn run_dogfood_browser(config: DogfoodBrowserConfig) -> Result<()> {
    let rendering_settings = RenderingProfile::resolve_with_default(
        config.rendering_profile,
        RenderingProfile::ServoModern,
    )?;
    if rendering_settings.profile == RenderingProfile::ChromeReference {
        anyhow::bail!(
            "chrome-reference is a configuration stub; use the Chrome reference capture path for UI parity"
        );
    }
    if let Some(profile_dir) = config.profile_dir.as_ref() {
        std::fs::create_dir_all(profile_dir)
            .with_context(|| format!("failed to create profile dir {}", profile_dir.display()))?;
    }
    let event_loop = EventLoop::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let mut app = DogfoodBrowserApp::new(&event_loop, config, rendering_settings);

    event_loop
        .run_app(&mut app)
        .context("dogfood browser event loop failed")
}

#[derive(Debug, Clone)]
struct SelectChoice {
    id: usize,
    label: String,
    disabled: bool,
}

struct ActiveSelect {
    control: SelectElement,
    choices: Vec<SelectChoice>,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BrowserLoadState {
    Starting,
    Loading,
    HeadParsed,
    Complete,
}

impl BrowserLoadState {
    fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Loading => "loading",
            Self::HeadParsed => "head",
            Self::Complete => "complete",
        }
    }
}

impl From<LoadStatus> for BrowserLoadState {
    fn from(status: LoadStatus) -> Self {
        match status {
            LoadStatus::Started => Self::Loading,
            LoadStatus::HeadParsed => Self::HeadParsed,
            LoadStatus::Complete => Self::Complete,
        }
    }
}

struct ShellTitleParts<'a> {
    profile: &'a str,
    load_state: BrowserLoadState,
    can_go_back: bool,
    can_go_forward: bool,
    page_title: Option<&'a str>,
    current_url: &'a str,
    active_select_label: Option<&'a str>,
}

fn format_shell_title(parts: ShellTitleParts<'_>) -> String {
    let back = if parts.can_go_back { "y" } else { "n" };
    let forward = if parts.can_go_forward { "y" } else { "n" };

    if let Some(label) = parts.active_select_label {
        return format!(
            "Saccade [{}] select={label} | back={back} fwd={forward} reload=Cmd+R | {}",
            parts.profile, parts.current_url
        );
    }

    let title = parts
        .page_title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(parts.current_url);
    format!(
        "Saccade [{}] load={} back={back} fwd={forward} | {title} | {}",
        parts.profile,
        parts.load_state.label(),
        parts.current_url
    )
}

struct DogfoodBrowserState {
    window: Window,
    servo: Servo,
    rendering_context: Rc<WindowRenderingContext>,
    url: Url,
    current_url: RefCell<Url>,
    webview: RefCell<Option<WebView>>,
    cursor_x: Cell<f32>,
    cursor_y: Cell<f32>,
    modifiers: Cell<ModifiersState>,
    load_state: Cell<BrowserLoadState>,
    page_title: RefCell<Option<String>>,
    active_select: RefCell<Option<ActiveSelect>>,
    started_at: Instant,
    auto_close_after: Option<Duration>,
    rendering_settings: RenderingProfileSettings,
}

impl DogfoodBrowserState {
    fn page_point(&self) -> WebViewPoint {
        WebViewPoint::Page(Point2D::<f32, CSSPixel>::new(
            self.cursor_x.get(),
            self.cursor_y.get(),
        ))
    }

    fn update_window_title(&self) {
        let active_select_label = self.active_select.borrow().as_ref().map(|active| {
            active
                .choices
                .get(active.cursor)
                .map(|choice| choice.label.as_str())
                .unwrap_or("(no selectable option)")
                .to_string()
        });
        let page_title = self.page_title.borrow().clone();
        let current_url = self.current_url.borrow().to_string();
        let (can_go_back, can_go_forward) = self
            .webview
            .borrow()
            .as_ref()
            .map(|webview| (webview.can_go_back(), webview.can_go_forward()))
            .unwrap_or((false, false));

        self.window.set_title(&format_shell_title(ShellTitleParts {
            profile: self.rendering_settings.profile.name(),
            load_state: self.load_state.get(),
            can_go_back,
            can_go_forward,
            page_title: page_title.as_deref(),
            current_url: current_url.as_str(),
            active_select_label: active_select_label.as_deref(),
        }));
    }

    fn handle_browser_shortcut(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }

        let modifiers = self.modifiers.get();
        if !modifiers.super_key() {
            return false;
        }

        let Some(webview) = self.webview.borrow().as_ref().cloned() else {
            return false;
        };

        match character_key(event).as_deref() {
            Some("r") | Some("R") => {
                self.load_state.set(BrowserLoadState::Loading);
                webview.reload();
                self.update_window_title();
                true
            }
            Some("[") => {
                if webview.can_go_back() {
                    self.load_state.set(BrowserLoadState::Loading);
                    webview.go_back(1);
                }
                self.update_window_title();
                true
            }
            Some("]") => {
                if webview.can_go_forward() {
                    self.load_state.set(BrowserLoadState::Loading);
                    webview.go_forward(1);
                }
                self.update_window_title();
                true
            }
            _ => false,
        }
    }

    fn handle_active_select_key(&self, event: &KeyEvent) -> bool {
        if event.state != ElementState::Pressed || self.active_select.borrow().is_none() {
            return false;
        }

        match &event.logical_key {
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.move_active_select(1);
                true
            }
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.move_active_select(-1);
                true
            }
            WinitKey::Named(WinitNamedKey::Enter) | WinitKey::Named(WinitNamedKey::Tab) => {
                self.submit_active_select();
                true
            }
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.dismiss_active_select();
                true
            }
            _ => false,
        }
    }

    fn move_active_select(&self, direction: isize) {
        let mut active_select = self.active_select.borrow_mut();
        let Some(active) = active_select.as_mut() else {
            return;
        };
        let Some(next) = next_selectable_choice(&active.choices, active.cursor, direction) else {
            return;
        };
        active.cursor = next;
        active.control.select(vec![active.choices[next].id]);
        drop(active_select);
        self.update_window_title();
    }

    fn submit_active_select(&self) {
        let Some(mut active) = self.active_select.borrow_mut().take() else {
            return;
        };
        if let Some(choice) = active.choices.get(active.cursor) {
            active.control.select(vec![choice.id]);
        }
        active.control.submit();
        self.update_window_title();
    }

    fn dismiss_active_select(&self) {
        self.active_select.borrow_mut().take();
        self.update_window_title();
    }

    fn close_webview(&self) {
        self.active_select.borrow_mut().take();
        self.webview.borrow_mut().take();
    }
}

impl WebViewDelegate for DogfoodBrowserState {
    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        *self.current_url.borrow_mut() = url;
        *self.page_title.borrow_mut() = None;
        self.load_state.set(BrowserLoadState::Loading);
        self.update_window_title();
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        *self.page_title.borrow_mut() = title;
        self.update_window_title();
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        self.load_state.set(status.into());
        self.update_window_title();
        if status == LoadStatus::Complete {
            self.window.request_redraw();
        }
    }

    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.window.request_redraw();
    }

    fn show_embedder_control(&self, _webview: WebView, embedder_control: EmbedderControl) {
        if let EmbedderControl::SelectElement(mut select) = embedder_control {
            let choices = flatten_select_choices(&select);
            let selected_options = select.selected_options();
            let cursor = choices
                .iter()
                .position(|choice| selected_options.contains(&choice.id) && !choice.disabled)
                .or_else(|| choices.iter().position(|choice| !choice.disabled))
                .unwrap_or(0);

            if let Some(choice) = choices.get(cursor) {
                select.select(vec![choice.id]);
            }

            *self.active_select.borrow_mut() = Some(ActiveSelect {
                control: select,
                choices,
                cursor,
            });
            self.update_window_title();
        }
    }
}

enum DogfoodBrowserApp {
    Initial {
        waker: Waker,
        config: DogfoodBrowserConfig,
        rendering_settings: RenderingProfileSettings,
    },
    Running {
        state: Rc<DogfoodBrowserState>,
    },
    Finished,
}

impl DogfoodBrowserApp {
    fn new(
        event_loop: &EventLoop<WakerEvent>,
        config: DogfoodBrowserConfig,
        rendering_settings: RenderingProfileSettings,
    ) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            config,
            rendering_settings,
        }
    }

    fn after_spin(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Running { state } = self else {
            return;
        };

        if state
            .auto_close_after
            .is_some_and(|timeout| state.started_at.elapsed() >= timeout)
        {
            state.close_webview();
            event_loop.exit();
            *self = Self::Finished;
        }
    }
}

impl ApplicationHandler<WakerEvent> for DogfoodBrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Self::Initial {
            waker,
            config,
            rendering_settings,
        } = self
        else {
            return;
        };

        let display_handle = match event_loop.display_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("failed to get display handle: {error}");
                event_loop.exit();
                return;
            }
        };

        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title(format!("Saccade - {}", config.url))
                .with_inner_size(LogicalSize::new(config.width.max(1), config.height.max(1))),
        ) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("failed to create Saccade window: {error}");
                event_loop.exit();
                return;
            }
        };

        let window_handle = match window.window_handle() {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("failed to get window handle: {error}");
                event_loop.exit();
                return;
            }
        };

        let initial_physical_size = window.inner_size();
        let rendering_context =
            match WindowRenderingContext::new(display_handle, window_handle, initial_physical_size)
            {
                Ok(context) => Rc::new(context),
                Err(error) => {
                    eprintln!("failed to create rendering context: {error:?}");
                    event_loop.exit();
                    return;
                }
            };

        if let Err(error) = rendering_context.make_current() {
            eprintln!("failed to make GL context current: {error:?}");
            event_loop.exit();
            return;
        }

        let mut servo_builder = ServoBuilder::default()
            .preferences(rendering_settings.servo_preferences())
            .event_loop_waker(Box::new(waker.clone()));
        if let Some(profile_dir) = config.profile_dir.clone() {
            let mut opts = Opts::default();
            opts.config_dir = Some(profile_dir);
            opts.temporary_storage = false;
            servo_builder = servo_builder.opts(opts);
        }
        let servo = servo_builder.build();
        servo.setup_logging();

        let state = Rc::new(DogfoodBrowserState {
            window,
            servo,
            rendering_context,
            url: config.url.clone(),
            current_url: RefCell::new(config.url.clone()),
            webview: RefCell::new(None),
            cursor_x: Cell::new(0.0),
            cursor_y: Cell::new(0.0),
            modifiers: Cell::new(ModifiersState::empty()),
            load_state: Cell::new(BrowserLoadState::Starting),
            page_title: RefCell::new(None),
            active_select: RefCell::new(None),
            started_at: Instant::now(),
            auto_close_after: config.auto_close_after,
            rendering_settings: rendering_settings.clone(),
        });

        let webview = WebViewBuilder::new(&state.servo, state.rendering_context.clone())
            .url(state.url.clone())
            .hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32))
            .delegate(state.clone())
            .build();
        *state.webview.borrow_mut() = Some(webview);
        state.update_window_title();

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
                    state.close_webview();
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
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview
                            .set_hidpi_scale_factor(Scale::new(state.window.scale_factor() as f32));
                        webview.resize(new_size);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    state.cursor_x.set(position.x as f32);
                    state.cursor_y.set(position.y as f32);
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::MouseInput {
                    state: button_state,
                    button,
                    ..
                } => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        webview.notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                            mouse_button_action(button_state),
                            servo_mouse_button(button),
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if let Some(webview) = state.webview.borrow().as_ref() {
                        let (x, y, mode) = wheel_delta(delta);
                        webview.notify_input_event(InputEvent::Wheel(WheelEvent::new(
                            WheelDelta { x, y, z: 0.0, mode },
                            state.page_point(),
                        )));
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    state.modifiers.set(modifiers.state());
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    if state.handle_active_select_key(&event)
                        || state.handle_browser_shortcut(&event)
                    {
                        return;
                    }

                    if let (Some(webview), Some(keyboard_event)) = (
                        state.webview.borrow().as_ref().cloned(),
                        servo_keyboard_event(&event),
                    ) {
                        webview.notify_input_event(InputEvent::Keyboard(keyboard_event));
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

fn flatten_select_choices(select: &SelectElement) -> Vec<SelectChoice> {
    let mut choices = Vec::new();
    for option_or_group in select.options() {
        match option_or_group {
            SelectElementOptionOrOptgroup::Option(option) => {
                choices.push(SelectChoice {
                    id: option.id,
                    label: option.label.clone(),
                    disabled: option.is_disabled,
                });
            }
            SelectElementOptionOrOptgroup::Optgroup { label, options } => {
                for option in options {
                    choices.push(SelectChoice {
                        id: option.id,
                        label: format!("{label} / {}", option.label),
                        disabled: option.is_disabled,
                    });
                }
            }
        }
    }
    choices
}

fn next_selectable_choice(
    choices: &[SelectChoice],
    cursor: usize,
    direction: isize,
) -> Option<usize> {
    if choices.is_empty() {
        return None;
    }

    let len = choices.len();
    for step in 1..=len {
        let next = if direction >= 0 {
            (cursor + step) % len
        } else {
            (cursor + len - step) % len
        };
        if !choices[next].disabled {
            return Some(next);
        }
    }
    None
}

fn wheel_delta(delta: MouseScrollDelta) -> (f64, f64, WheelMode) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            ((x * 76.0) as f64, (y * 76.0) as f64, WheelMode::DeltaLine)
        }
        MouseScrollDelta::PixelDelta(delta) => (delta.x, delta.y, WheelMode::DeltaPixel),
    }
}

fn mouse_button_action(state: ElementState) -> MouseButtonAction {
    match state {
        ElementState::Pressed => MouseButtonAction::Down,
        ElementState::Released => MouseButtonAction::Up,
    }
}

fn servo_mouse_button(button: WinitMouseButton) -> MouseButton {
    match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Back,
        WinitMouseButton::Forward => MouseButton::Forward,
        WinitMouseButton::Other(value) => MouseButton::Other(value),
    }
}

fn servo_keyboard_event(event: &KeyEvent) -> Option<KeyboardEvent> {
    let state = match event.state {
        ElementState::Pressed => KeyState::Down,
        ElementState::Released => KeyState::Up,
    };
    let key = servo_key(event)?;
    Some(KeyboardEvent::from_state_and_key(state, key))
}

fn servo_key(event: &KeyEvent) -> Option<ServoKey> {
    if event.state == ElementState::Pressed {
        if let Some(text) = event.text.as_ref() {
            let text = text.to_string();
            if !text.is_empty() && !text.chars().any(char::is_control) {
                return Some(ServoKey::Character(text));
            }
        }
    }

    match &event.logical_key {
        WinitKey::Character(text) => {
            let text = text.to_string();
            if text.is_empty() {
                None
            } else {
                Some(ServoKey::Character(text))
            }
        }
        WinitKey::Named(WinitNamedKey::Space) => Some(ServoKey::Character(" ".to_string())),
        WinitKey::Named(named) => map_named_key(*named).map(ServoKey::Named),
        WinitKey::Unidentified(_) | WinitKey::Dead(_) => None,
    }
}

fn character_key(event: &KeyEvent) -> Option<String> {
    match &event.logical_key {
        WinitKey::Character(text) => Some(text.to_string()),
        _ => None,
    }
}

fn map_named_key(key: WinitNamedKey) -> Option<ServoNamedKey> {
    match key {
        WinitNamedKey::Enter => Some(ServoNamedKey::Enter),
        WinitNamedKey::Tab => Some(ServoNamedKey::Tab),
        WinitNamedKey::Escape => Some(ServoNamedKey::Escape),
        WinitNamedKey::Backspace => Some(ServoNamedKey::Backspace),
        WinitNamedKey::Delete => Some(ServoNamedKey::Delete),
        WinitNamedKey::ArrowDown => Some(ServoNamedKey::ArrowDown),
        WinitNamedKey::ArrowLeft => Some(ServoNamedKey::ArrowLeft),
        WinitNamedKey::ArrowRight => Some(ServoNamedKey::ArrowRight),
        WinitNamedKey::ArrowUp => Some(ServoNamedKey::ArrowUp),
        WinitNamedKey::End => Some(ServoNamedKey::End),
        WinitNamedKey::Home => Some(ServoNamedKey::Home),
        WinitNamedKey::PageDown => Some(ServoNamedKey::PageDown),
        WinitNamedKey::PageUp => Some(ServoNamedKey::PageUp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_title_includes_url_load_and_nav_state() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Complete,
            can_go_back: true,
            can_go_forward: false,
            page_title: Some("Example Domain"),
            current_url: "https://example.com/",
            active_select_label: None,
        });

        assert!(title.contains("Saccade [servo-modern]"));
        assert!(title.contains("load=complete"));
        assert!(title.contains("back=y"));
        assert!(title.contains("fwd=n"));
        assert!(title.contains("Example Domain"));
        assert!(title.contains("https://example.com/"));
    }

    #[test]
    fn shell_title_marks_select_mode_without_hiding_url() {
        let title = format_shell_title(ShellTitleParts {
            profile: "servo-modern",
            load_state: BrowserLoadState::Loading,
            can_go_back: false,
            can_go_forward: true,
            page_title: Some("Ignored while select is active"),
            current_url: "https://example.com/form",
            active_select_label: Some("us-east"),
        });

        assert!(title.contains("select=us-east"));
        assert!(title.contains("back=n"));
        assert!(title.contains("fwd=y"));
        assert!(title.contains("reload=Cmd+R"));
        assert!(title.contains("https://example.com/form"));
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
