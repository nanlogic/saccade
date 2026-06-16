# Servo API Map

Pin: `servo = "=0.2.0"`

Status: reconciled during M0 after `cargo doc -p servo --no-deps` generated local docs at `target/doc/servo/index.html`.

Dependency alignment from `cargo tree -p saccade_browser`:

- `servo v0.2.0`
- `euclid v0.22.14`
- `image v0.25.10`
- `winit v0.30.13` from `saccade_browser`, matching Servo's `winit_minimal` example dependency.
- `servo-fonts v0.2.0` is locally patched at `vendor/servo-fonts-0.2.0` for a macOS M0 compile bug; see `docs/blockers.md`.

## M0 symbols

- Rendering context constructor: `servo::WindowRenderingContext::new(display_handle: DisplayHandle, window_handle: WindowHandle, size: PhysicalSize<u32>) -> Result<WindowRenderingContext, surfman::Error>`.
- Test/headless context visible in Servo tests: `servo::SoftwareRenderingContext::new(PhysicalSize<u32>) -> Result<SoftwareRenderingContext, surfman::Error>`.
- WebView creation: `servo::WebViewBuilder::new(&Servo, Rc<dyn RenderingContext>).url(Url).hidpi_scale_factor(Scale<f32, DeviceIndependentPixel, DevicePixel>).delegate(Rc<dyn WebViewDelegate>).build()`.
- Frame readiness: `WebViewDelegate::notify_new_frame_ready(&self, WebView)`.
- Page metadata callbacks: `WebViewDelegate::notify_url_changed(&self, WebView, Url)`, `notify_page_title_changed(&self, WebView, Option<String>)`, and `notify_load_status_changed(&self, WebView, LoadStatus)`.
- Load readiness: `WebView::load_status() -> LoadStatus`; complete state is `LoadStatus::Complete`.
- Paint: `WebView::paint()`.
- Runtime resize: `WebView::resize(PhysicalSize<u32>)` is the resize authority for Servo WebViews. Do not pre-resize the shared `WindowRenderingContext`; Servo's paint path checks the current rendering-context size and can return before sending layout viewport updates if Saccade already resized it. With `WebView::resize` as the only resize call, the pinned API updates the rendering surface and the page JS/layout viewport.
- Readback: `RenderingContext::read_to_image(source_rectangle: DeviceIntRect) -> Option<RgbaImage>`.
- Input: `InputEvent::MouseMove(MouseMoveEvent::new(point: WebViewPoint))`; `InputEvent::MouseButton(MouseButtonEvent::new(action: MouseButtonAction, button: MouseButton, point: WebViewPoint))`.
- Wheel input: `InputEvent::Wheel(WheelEvent::new(WheelDelta { x, y, z, mode }, point))`; Servo's pinned `winit_minimal` example maps `MouseScrollDelta::LineDelta` to `WheelMode::DeltaLine` and `PixelDelta` to `WheelMode::DeltaPixel`.
- Keyboard input: `InputEvent::Keyboard(KeyboardEvent::from_state_and_key(KeyState::Down, Key::Character(text)))` and the matching `KeyState::Up` event inserted ASCII text into a focused `<input>` in the native input selftest.
- Input handling callback: `WebViewDelegate::notify_input_event_handled(&self, WebView, InputEventId, InputEventResult)` is delivered after `WebView::notify_input_event(...)`; `InputEventResult::DispatchFailed` is the failure flag to gate on. In the native input selftest, `Consumed` remained false even though the DOM value updated.
- Select/dropdown control: a trusted click on `<select>` causes `WebViewDelegate::show_embedder_control(&self, WebView, EmbedderControl::SelectElement(...))`; Saccade can call `select.select(vec![index])` and `select.submit()` to send the chosen option back to Servo.
- Navigation controls: `WebView::reload()`, `can_go_back()`, `go_back(amount)`, `can_go_forward()`, and `go_forward(amount)` are public in the pinned WebView API. A product-safe public `stop_loading`/`stop` equivalent has not been mapped in the pinned API yet, so Stop remains a shell/product backlog item rather than a wired control.
- Input point unit: `WebViewPoint`, which is either `WebViewPoint::Device(DevicePoint)` or `WebViewPoint::Page(Point2D<f32, CSSPixel>)`. Servo's own tests pass `DevicePoint` converted with `.into()`.
- JS probe: `WebView::evaluate_javascript(script, callback)` where callback is `FnOnce(Result<JSValue, JavaScriptEvaluationError>) + 'static`.
- Screenshot: `WebView::take_screenshot(rect: Option<WebViewRect>, callback: FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static)`.
- WebView console capture: `WebViewDelegate::show_console_message(&self, WebView, ConsoleLogLevel, String)`.
- WebView resource-load capture/interception: `WebViewDelegate::load_web_resource(&self, WebView, WebResourceLoad)`; `WebResourceLoad::request()` exposes method, URL, destination, main-frame flag, redirect flag, and headers. Dropping the load continues without interception.
- Event loop shape: mirrored `servo 0.2.0`'s `examples/winit_minimal.rs` using `winit::application::ApplicationHandler`, `EventLoop::with_user_event()`, `ServoBuilder::default().event_loop_waker(...)`, `Servo::spin_event_loop()`, and `WindowRenderingContext`.

## M0 boot result

`cargo run -p mousemax -- selftest-boot` opened a 1280x800 `WindowRenderingContext`, loaded `about:blank`, loaded local `test_pages/calibration.html` through `tiny_http`, painted/read back a frame, read `page_title()`, and printed:

```text
BOOT OK title="Calibration"
```

## Native keyboard input result

`cargo run -q -p saccade-shell -- selftest-native-input` opened `test_pages/native_input/index.html`, clicked the input using native mouse events, typed `saccade42` with native keyboard events, clicked a select, handled `EmbedderControl::SelectElement`, selected `gamma`, and printed:

```text
NATIVE_INPUT PASS focused=true value_len=9 keydown=9 keypress=9 beforeinput=0 input=9 keyup=9 handled_keyboard=18 consumed_keyboard=0 dispatch_failed=0 select_value=gamma select_input=1 select_change=1 select_controls=1
```
