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

## Current Gate Status

- R0 Browser Render Gate: pass with official ServoShell on
  `http://127.0.0.1:4173/`.
- R1 Input Ownership Gate: blocked for WebDriver, pending for in-process bridge.
- R2 Frame Truth Gate: pending. The required shape is the old
  `FrameObservation`, not a full-page screenshot agent channel.
- R3 Reflex Latency Gate: pending. Local game v0 target is p95
  observe-to-input dispatch <= 16 ms; MOUSEMAX remains p95 detect-to-dispatch
  <= 5 ms.

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
