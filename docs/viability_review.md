# Saccade M-1 Browser Viability Review

Checked: 2026-06-11

Scope: viability only. No Rust, Cargo workspace, Servo pin, or scaffold has been created.

Sources checked:
- `SACCADE_BUILD_SPEC_v4.md`
- Servo blog, "Servo is now available on crates.io", 2026-04-13: https://servo.org/blog/2026/04/13/servo-0.1.0-release/
- docs.rs `servo` latest page, observed as `servo 0.2.0` at check time: https://docs.rs/crate/servo/latest
- docs.rs `servo` API pages for `WebView`, `RenderingContext`, `WebViewDelegate`, `UserContentManager`, `InputEvent`, and `Servo`
- doc.servo.org API pages, observed as `servo 0.3.0` at check time: https://doc.servo.org/servo/

## 1. Stock Servo viability

Yes: stock Servo plausibly provides the four browser-route capabilities Saccade needs, without starting from a fork.

Rendered frame readback:
- `RenderingContext::read_to_image(source_rectangle)` exists in the current docs.rs `servo` API and accepts a sub-rectangle, returning an RGBA image buffer.
- The docs.rs page says the read can happen after render and before `present()`, which is exactly the hot-loop shape this project wants.

Frame readiness:
- `WebViewDelegate::notify_new_frame_ready(webview)` exists and is documented as the embedder signal to present a new frame.
- `WebView::paint()` paints into the `RenderingContext`.
- `Servo::spin_event_loop()` is present and documented as the loop that runs delegate methods and event processing.
- `WebView::animating()` exists, which is useful for pages driven by animation or `requestAnimationFrame`.

Browser-level input:
- `WebView::notify_input_event(InputEvent)` exists and returns an `InputEventId`.
- `WebViewDelegate::notify_input_event_handled(...)` exists for processing results.
- `InputEvent` includes mouse move, mouse button, wheel, touch, keyboard, IME, and editing variants. This is the right entry point for an engine-level click path.

Recon probes:
- `WebView::evaluate_javascript(...)` exists for read-only probes.
- `WebView::user_content_manager()` exists, and `UserContentManager` supports adding scripts/styles to pages on reload.
- `WebView::take_screenshot(...)` exists for debug/reference evidence, though it should not be used in the hot loop.

The key caveat: docs.rs currently reports `servo 0.2.0` while doc.servo.org reports `servo 0.3.0`. The API surface looks aligned enough for viability, but M0 must treat doc.servo.org as possibly-ahead-of-crates and trust only the pinned local `cargo doc` output.

## 2. Top five kill risks

1. Live site web compatibility fails in Servo. Existential.
   The benchmark is the real `mouseaccuracy.com/classic` page. If it does not load, animate, run its JS, draw targets, or accept ordinary browser input in Servo, the Servo route cannot satisfy the headline claim.

2. Engine input does not match the site's scoring path. Existential.
   `notify_input_event` exists, but M1 must prove that Servo's mouse move/down/up sequence produces whichever site event actually scores hits (`mousedown`, `click`, pointer events, or similar). If the page never counts those events, the route fails.

3. Coordinate calibration cannot be closed to zero misses. Existential.
   The whole benchmark depends on mapping rendered pixels, CSS/layout rects, device pixels, viewport position, and input coordinates into one consistent click target. Servo's coordinate APIs are plausible, but only runtime calibration can prove the chain.

4. Frame readback plus detection plus input cannot meet the latency budget. Existential for MOUSEMAX.
   `read_to_image` is available, but p95 `detect -> input_dispatched <= 5 ms` is a performance claim, not an API claim. M0/M1 can smoke-test feasibility; later milestones must measure it honestly.

5. Servo API/version/build churn costs more than the project can absorb. Annoying unless M0 fails outright.
   Servo is pre-1.0 and its own release note says breaking monthly changes are expected, with LTS releases available. The mitigation is to pin once, commit `Cargo.lock`, run local `cargo doc`, and keep all Servo usage inside `saccade_browser`.

## 3. What M0 and M1 prove or disprove

M0 proves or disproves that a pinned stock Servo crate can be embedded locally:
- Cargo can resolve and build the pinned crate on the target machine.
- Local `cargo doc -p servo --no-deps` produces the actual API map.
- A blank/minimal WebView can be created and driven through the event loop.
- `notify_new_frame_ready -> paint -> read_to_image` yields real pixels.
- `notify_input_event` can deliver mouse move/down/up to a local test page.
- `evaluate_javascript`, `take_screenshot`, and user-content injection are callable under the pin.

M1 proves or disproves that the real benchmark page is compatible with that embedder:
- The real page loads, renders, and animates in Servo.
- The controls for Epic, Tiny, and Start can be discovered at runtime and clicked through engine input.
- The game technology is classified: canvas, DOM, SVG, or mixed.
- The game area, result strings, score/miss nodes, consent/ad behavior, iframes, and console errors are recorded.
- The actual scoring event, spawn cadence, target size/lifetime, and target coexistence behavior are measured.
- A minimal safe click/recon run shows whether the page reacts correctly before building the full reflex stack.

If M0 fails, stop before architecture work. If M1 fails because of Servo/web-compat, pivot before investing in M2+ integration.

## 4. Backup if Servo fails

The backup is CEF/Chromium, not arena-only.

Arena-only remains useful for deterministic local development and CI, but it cannot prove the browser product or the public benchmark claim. If Servo fails because the live page is incompatible, because input cannot be accepted, or because readback/input timing cannot be made credible, the next browser-engine candidate should be CEF/Chromium with the same purity rules: rendered-frame truth, browser-level input, no direct DOM event dispatch for benchmark clicks, and no LLM in the hot loop.

Kill only if the browser-truth loop itself is not defensible in any practical engine. Do not call arena-only a pass for Saccade.

SACCADE_BROWSER_VERDICT: GO_SERVO_WITH_BACKUP
