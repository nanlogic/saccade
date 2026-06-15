# Reflex Bridge Audit

Date: 2026-06-14

## Purpose

This audit captures the old Saccade/MOUSEMAX fast path that must survive the
move to official ServoShell.

Current product direction:

```text
official ServoShell human layer
  + Saccade in-process reflex bridge
  + existing detector / motor / replay crates
```

The WebDriver adapter remains useful for product flows, safety gates, and
ordinary form work. It is not the reflex runtime for local games, MOUSEMAX, or
mouseaccuracy.

## Old Fast Path

The existing fast path lives mainly in:

- `crates/saccade_browser/src/arena_run.rs`
- `crates/saccade_detect/src/lib.rs`
- `crates/saccade_motor/src/lib.rs`
- `crates/saccade_replay/src/lib.rs`

The important loop is:

```text
webview.paint()
  -> rendering_context.read_to_image(...)
  -> FrameObservation { pixels, dom_rects, t_paint_ns, t_readback_ns }
  -> DetectionPipeline::on_frame(...)
  -> MotorController::on_frame(...)
  -> webview.notify_input_event(MouseMove / MouseButton Down / Up)
  -> ReplayLogger::try_log(...)
```

This is why the old path can be fast: readback, detection, decision, and input
dispatch are all in the same process and share one monotonic run clock.

## Timing Contracts Already Present

- `FRAME_INTERVAL` is currently 20 ms in the old arena and real-site runners.
- `MotorController` rejects stale frames older than 20 ms by default.
- `MotorController` enforces an 8 ms inter-click floor by default.
- `ClickReceipt` records target-first-seen, decision, move, down, and up
  timestamps.
- `BenchmarkResult` records p95 `detect_to_dispatch`, p95
  `first_visible_to_dispatch`, capture, and detect latencies.
- MOUSEMAX validation already fails if p95 `detect_to_dispatch` is above 5 ms.

These contracts are the right shape. The bridge should preserve them instead of
inventing a new benchmark vocabulary.

## Reusable Pieces

Keep:

- `FrameObservation`, `GameFrameReport`, `RenderedTarget`, `ClickReceipt`, and
  replay event schemas from `saccade_core` / `saccade_replay`.
- `DetectionPipeline` as the first local-game detector path.
- `MotorController` as the first decision path.
- Non-blocking replay logging: `ReplayLogger::try_log` never blocks the hot
  path and drop-prefers frame reports if the channel is full.
- Servo internal input semantics: page-coordinate `MouseMove`, `MouseButton
  Down`, and `MouseButton Up` sent through `webview.notify_input_event`.

Change or re-check:

- The current pixel detector allocates per-frame temporary vectors. That is
  acceptable for a local-game v0 measurement, but MOUSEMAX-level acceptance
  should reuse buffers before calling the final gate done.
- The current old embedder is pinned to `servo = 0.2.0`; the bridge target is
  official ServoShell 0.3.0 source/runtime, because that human layer renders the
  local game correctly.
- External WebDriver action dispatch is not accepted as a reflex path unless it
  unexpectedly proves the same latency and ownership, which current evidence
  does not show.

## Bridge Insertion Points Needed

The official ServoShell source bridge needs only a thin hot path:

1. Hook after a webview paint/frame-ready point where the rendered surface can
   be read into RGBA or equivalent frame bytes.
2. Build `FrameObservation` with ServoShell viewport scale, page zoom, game
   area, and monotonic timestamps.
3. Run `DetectionPipeline` and `MotorController` in-process.
4. Dispatch internal page-coordinate mouse events to the same webview.
5. Emit replay events through the existing non-blocking logger.

The bridge must not put network, LLM calls, formatting-heavy logs, or blocking
file I/O in this path.

## Official ServoShell Source Map

Official source checkout used for this audit:

```text
/Users/waynema/Documents/GitHub/servo-saccade-upstream
commit 54288c9d6
```

Important source locations:

- `ports/servoshell/window.rs`
  - `ServoShellWindow::repaint_webviews()` currently does:
    `make_current() -> webview.paint() -> rendering_context.present()`.
  - This is the best first observe-only bridge point.
- `components/shared/paint/rendering_context.rs`
  - `RenderingContext::read_to_image(DeviceIntRect) -> Option<RgbaImage>` is
    part of the official rendering context trait.
  - The trait docs say double-buffered contexts should read the back buffer
    after Servo renders and before `present()`.
- `ports/servoshell/desktop/headed_window.rs`
  - `PlatformWindow::rendering_context()` returns the offscreen rendering
    context used for Servo webview content.
  - Human mouse events are translated into webview-relative device pixels and
    sent through `webview.notify_input_event(...)`.
- `ports/servoshell/running_app_state.rs`
  - `notify_new_frame_ready()` marks the window as needing repaint.
  - WebDriver input uses the same `webview.notify_input_event(...)`, but through
    the external WebDriver command path.
  - WebDriver screenshot calls `webview.take_screenshot(...)`, which is not the
    reflex path because it waits for page/rendering stability.
- `components/servo/webview.rs`
  - `take_screenshot()` is explicitly asynchronous and waits for stable page
    conditions. It is useful for diagnostics, not the millisecond control loop.

Preferred first bridge patch:

```text
ServoShellWindow::repaint_webviews()
  -> make_current()
  -> webview.paint()
  -> if reflex enabled:
       read_to_image(current viewport)
       build FrameObservation
       log observe-only replay frame
  -> present()
```

After observe-only timing is proven, add:

```text
DetectionPipeline::on_frame(...)
  -> MotorController::on_frame(...)
  -> WebView::notify_input_event(MouseMove / MouseButton Down / Up)
```

Do not call `WebView::take_screenshot()` in the reflex hot path.

## Current Gate Status

- R0 Browser Render Gate: pass with official ServoShell on
  `http://127.0.0.1:4173/`.
- R1 Input Ownership Gate: blocked for WebDriver, pending for in-process bridge.
- R2 Frame Truth Gate: partial pass. The official ServoShell source bridge now
  captures observe-only repaint frames with `RenderingContext::read_to_image`.
  The required final shape is still the old `FrameObservation`, not a full-page
  screenshot agent channel.
- R3 Reflex Latency Gate: pending. Local game v0 target is p95
  observe-to-input dispatch <= 16 ms; MOUSEMAX remains p95 detect-to-dispatch
  <= 5 ms.

## Bridge Evidence - 2026-06-14

Official ServoShell source build:

```sh
./mach build --dev -j 4 --media-stack dummy
```

Result:

```text
Succeeded in 0:09:14
target/debug/servoshell --version => Version: Servo 0.3.0-54288c9d6
```

Bridge commit in `/Users/waynema/Documents/GitHub/servo-saccade-upstream`:

```text
6e02f55f1 add saccade observe-only reflex bridge
```

Observe-only run:

```sh
SACCADE_REFLEX_OBSERVE_PATH=/Users/waynema/Documents/GitHub/SACCADE/runs/reflex_observe/observe_1781488060000/frames.jsonl \
SACCADE_REFLEX_OBSERVE_MAX_FRAMES=120 \
cargo run -q -p saccade-servoshell -- probe \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/debug/servoshell \
  --url http://127.0.0.1:4173/ \
  --screenshot-mode guarded-diagnostic \
  --timeout-sec 35
```

Result:

```text
SACCADE_SERVOSHELL_PROBE ok=true
report=runs/servoshell_adapter/probe_1781488077618/report.json
frames=runs/reflex_observe/observe_1781488060000/frames.jsonl
```

Frame summary:

```text
frames=5
readback_ok=5/5
size=1024x740
title=Blend or Die - Prototype
dropped_logs=0
readback_ms p50=5.55 p95=7.05 max=7.86
```

This proves the official ServoShell 0.3 source path can expose non-screenshot
frame truth from the repaint path. It does not yet prove input ownership or
closed-loop game play.

## Verification Notes

Command:

```sh
cargo test -q -p saccade_core -p saccade_detect -p saccade_motor -p saccade_replay
```

Result on 2026-06-14: pass after rerun.

One initial run reported the synthetic 1280x600 pixel detector timing test at
4.896 ms against a 3.0 ms unit-test limit; an immediate single-test rerun
reported 1.609 ms, and the full command then passed. Treat this as evidence
that final reflex gates must use replay p95 over many frames/runs rather than a
single wall-clock unit-test sample.

## Next Work Packet

1. Locate and build official ServoShell source matching the installed
   ServoShell 0.3.0 runtime closely enough for local testing.
2. Add the smallest compile-time bridge module that can emit one
   `FrameObservation` from the visible webview.
3. Run the local game with observe-only frame reports first.
4. Add internal input dispatch only after frame timestamps are measured.
5. Compare replay latency against the old embedded runner before touching
   broader product UI.
