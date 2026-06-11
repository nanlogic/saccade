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
- Load readiness: `WebView::load_status() -> LoadStatus`; complete state is `LoadStatus::Complete`.
- Paint: `WebView::paint()`.
- Readback: `RenderingContext::read_to_image(source_rectangle: DeviceIntRect) -> Option<RgbaImage>`.
- Input: `InputEvent::MouseMove(MouseMoveEvent::new(point: WebViewPoint))`; `InputEvent::MouseButton(MouseButtonEvent::new(action: MouseButtonAction, button: MouseButton, point: WebViewPoint))`.
- Input point unit: `WebViewPoint`, which is either `WebViewPoint::Device(DevicePoint)` or `WebViewPoint::Page(Point2D<f32, CSSPixel>)`. Servo's own tests pass `DevicePoint` converted with `.into()`.
- JS probe: `WebView::evaluate_javascript(script, callback)` where callback is `FnOnce(Result<JSValue, JavaScriptEvaluationError>) + 'static`.
- Screenshot: `WebView::take_screenshot(rect: Option<WebViewRect>, callback: FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static)`.
- Event loop shape: mirrored `servo 0.2.0`'s `examples/winit_minimal.rs` using `winit::application::ApplicationHandler`, `EventLoop::with_user_event()`, `ServoBuilder::default().event_loop_waker(...)`, `Servo::spin_event_loop()`, and `WindowRenderingContext`.

## M0 boot result

`cargo run -p mousemax -- selftest-boot` opened a 1280x800 `WindowRenderingContext`, loaded `about:blank`, loaded local `test_pages/calibration.html` through `tiny_http`, painted/read back a frame, read `page_title()`, and printed:

```text
BOOT OK title="Calibration"
```
