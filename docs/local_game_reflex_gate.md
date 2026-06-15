# Local Game Reflex Gate

Date: 2026-06-14

## North Star

The current launch-critical target is:

```text
An LLM can play our designed browser game through Saccade.
```

This does not mean the LLM thinks on every frame. The frame loop must be local
and deterministic:

```text
LLM strategy / high-level intent
  -> Saccade reflex runtime
  -> browser frame truth
  -> target/state detector
  -> motor decision
  -> low-latency browser/native input
  -> verification + replay
```

The LLM may choose goals, modes, or policies. It must not sit in the millisecond
control loop.

## Non-Negotiable Requirements

1. **Browser handles the game for humans**
   - The human-visible browser must render the local game correctly enough to
     inspect and trust the demo.
   - Official ServoShell is the current preferred human layer because it runs
     the local game better than the old embedded `servo=0.2.0` path.

2. **Agent has millisecond control**
   - The agent path cannot degrade to Chrome/Playwright-style DOM automation.
   - WebDriver may remain a product/safety adapter, but it is not the reflex
     runtime.
   - A passing reflex runtime must measure observe-to-input latency, not guess
     it.

3. **Truth layer stays safe**
   - General product truth defaults to redacted DOM/layout/action/form truth.
   - Screenshots are not the default agent truth channel for real user pages.
   - Pixel/frame truth is allowed only in explicitly non-sensitive reflex modes
     such as local games and benchmarks.

## Pass Gates

### R0: Browser Render Gate

Status: pass.

- `http://127.0.0.1:4173/` loads in official ServoShell.
- The game screenshot is nonblank and visually recognizable.
- Page truth reports title `Blend or Die - Prototype`.

Evidence:

```text
runs/servoshell_adapter/probe_1781484941056/report.json
runs/servoshell_adapter/probe_1781484941056/screenshot.png
```

Fast-path audit:

```text
docs/reflex_bridge_audit.md
```

### R1: Input Ownership Gate

Status: partial pass on fixture and local game.

Required evidence:

- Saccade can move/control the player in the local game through a browser or
  native input path.
- The page state visibly changes and the replay records the input basis.
- WebDriver-only control is not enough unless measured latency and reliability
  meet the reflex target.

Current evidence:

- The ServoShell bridge dispatched an internal test click through
  `WebView::notify_input_event` without using WebDriver click or DOM
  `dispatchEvent`.
- Fixture:
  `test_pages/browser_session/index.html`.
- Frame log:
  `runs/reflex_input/input_dom_1781488695005_f3/frames.jsonl`.
- The test click landed inside the page button rect
  `{x:152,y:221,w:180,h:48}` at `(240,250)`.
- WebDriver was used only after the fact to read page state. It reported
  `revision="1"`, button text `Verified`, and status text
  `Agent action verified in the same browser session.`
- Dispatch timing from bridge log:
  `dispatch_ns=343125` (`0.343 ms`), `dropped_logs=0`.
- The bridge also dispatched an env-gated drag gesture through the same input
  path against `http://127.0.0.1:4173/`.
- Game drag evidence:
  `runs/reflex_input/game_drag_1781489118629_fast/frames.jsonl`.
- During that run, the game's public local debug state showed camera movement:
  `camera.x` changed from `691` to `724` while the game stayed in
  `mode="running"`.
- Drag timing from bridge log:
  6 internal drag events, `dispatch_ns` min `45417`, max `249417`,
  `dropped_logs=0`.
- Frame truth for the same run:
  31 frame logs, 31 `readback_ok=true`.
- Release bridge evidence:
  `runs/reflex_input/release_game_drag_1781491400/frames.jsonl`.
- In release headless mode, game time kept pace with wall time
  (`time_scale=0.999`), the bridge logged 180/180 readback frames, and the drag
  moved the local game camera by `+20px`.
- Live command interface evidence:
  `runs/reflex_live/live_release_1781495324/report.json`.
- The release bridge accepted external JSONL commands through
  `SACCADE_REFLEX_COMMANDS_PATH` and returned receipts through
  `SACCADE_REFLEX_RECEIPTS_PATH`.
- The live probe produced `ping:ok=1`, `drag:scheduled=1`, and
  `drag_phase:dispatched=9` receipts.
- The same run kept game time at `time_scale=1.002`, logged 420/420 readback
  frames, and moved the local game camera by `+21px`.
- Drag phase dispatch timing in release:
  p50 `0.023 ms`, p95 `0.075 ms`, max `0.078 ms`.
- Live click command evidence:
  `runs/reflex_live_click/click_release_1781496285/report.json`.
- The release bridge accepted a JSONL `click` command at `(242,245)` against
  `test_pages/browser_session/index.html`, returned `click:dispatched`, and the
  page changed to `revision=1` / button `Verified`.
- Click dispatch timing in release: `0.196 ms`.

Remaining R1 work:

- Replace the fixed fixture click with detector/motor-owned local game input.
- Record replay timestamps for observe, decision, dispatch, and verification.
- Route detector facts through the generic Browser Fact Stream instead of
  adding game-specific controller APIs.

### R2: Frame Truth Gate

Status: partial pass, observe-only.

Required evidence:

- Saccade receives game-frame truth at reflex speed.
- Preferred channel: in-process ServoShell frame/readback bridge.
- Temporary non-sensitive diagnostic channel may use game-page debug state or
  pixels only to validate local game progress, not as the product safety model.

Current evidence:

- Official ServoShell source build `Servo 0.3.0-54288c9d6` succeeded with
  `./mach build --dev -j 4 --media-stack dummy`.
- Wayne's manual headed check of the downloaded official macOS Servo.app
  (`Servo 0.3.0-302457869`) on `http://127.0.0.1:4173/` did not show the
  earlier severe lag. Treat official headed Servo.app as the current human
  rendering reference, pending measured FPS/time-scale capture.
- Release source build `Servo 0.3.0-805e6a423` succeeded and removed the local
  game slowdown:
  `docs/servoshell_runtime_matrix.md`.
- Saccade observe-only bridge commit in the Servo checkout:
  `6e02f55f1 add saccade observe-only reflex bridge`.
- The local game probe passed through the locally built ServoShell binary:
  `runs/servoshell_adapter/probe_1781488077618/report.json`.
- The bridge captured 5 repaint frames in:
  `runs/reflex_observe/observe_1781488060000/frames.jsonl`.
- All 5 frames had `readback_ok=true`, 1024x740 RGBA, title
  `Blend or Die - Prototype`, and `dropped_logs=0`.
- Short-sample readback timing: p50 5.55 ms, p95 7.05 ms, max 7.86 ms.

### R3: Reflex Latency Gate

Status: partial pass for release bridge + v0 policy loop.

Current evidence:

- External command -> in-process input dispatch now works in release ServoShell:
  `docs/reflex_live_interface.md`.
- Latest release live-command probe:
  `runs/reflex_live/live_release_1781495324/report.json`.
- The measured number is bridge dispatch time for scheduled drag phases, not
  full observe->detect->decision->dispatch latency.
- Local game release loop evidence:
  `runs/local_game_reflex/loop_release_1781525581/report.json`.
- Replay:
  `runs/local_game_reflex/loop_release_1781525581/replay.jsonl`.
- The v0 loop uses public `canvas.dataset.debug` and DOM panel visibility as a
  temporary detector, an orbit/upgrade motor policy, and JSONL replay. Browser
  input still enters only through the ServoShell reflex bridge.
- Result: `ok=true`, duration complete, 57 drag commands, 57 command receipts,
  627 drag phase dispatch receipts, 1400/1400 readback frames, p95 dispatch
  `0.071 ms`, p95 full-window readback `8.30 ms`, game `time_scale=0.993`,
  HP delta `0`, camera delta `+38,+29`.
- Limitation: this proves the release bridge + motor/replay loop can drive the
  local game, but it does not yet prove visual detector ownership. The v0 policy
  survived and moved/shoots, but did not collect drops (`fill_delta=0`).
- Browser Fact Stream v0 now exists as the generic control-plane truth
  interface:
  `docs/browser_fact_stream.md`.
- Current fact stream evidence:
  `runs/browser_fact_stream/facts_visual_1781527623/report.json`.
- It detects new nodes, actionable controls, sensitive fields, canvas surfaces,
  and fixture-grade canvas `visual_object_seen` facts with redaction. It does
  not yet classify local-game fruit/enemy/drop semantics; that should arrive by
  adapting crop/pixel, canvas-observe, or Servo native emitters to the same
  `visual_object_seen` schema.
- Existing MOUSEMAX arena replay targets now convert to the same fact schema:
  `runs/browser_fact_stream/mousemax_1781528244/report.json`.
- That conversion emitted 45 `visual_object_seen` facts and matched the old
  run's `targets_seen=45` after filtering two tracker appearances outside the
  benchmark game area.

Local game v0 pass:

- p95 observe-to-input dispatch <= 16 ms.
- 30+ seconds of stable control without freezing or losing the input path.
- Replay includes timestamps for observe, decision, dispatch, and verify.

MOUSEMAX/mouseaccuracy pass remains stricter:

- p95 detect-to-dispatch <= 5 ms.
- Zero misses / stale clicks under the benchmark acceptance table.

## Kill / Pivot Conditions

Saccade is not differentiated if the best available runtime is only:

```text
DOM observe -> WebDriver/Playwright action -> slow verification
```

If the official ServoShell route cannot expose ms-level frame truth and input
control, the next move is not more WebDriver glue. The next move is:

1. build/clone official ServoShell source,
2. add a thin in-process Saccade bridge around frame truth, input, safety, and
   replay,
3. keep official human rendering/UI behavior intact.

If that bridge cannot meet R1-R3, then the game/reflex demo is a kill/pivot
gate for the project.

## Immediate Work

1. Inspect the old Saccade/MOUSEMAX reflex code path and identify exactly which
   pieces must survive:
   - frame capture/readback,
   - target/state detector,
   - input dispatch,
   - replay timestamps.
2. Inspect official ServoShell source/build path and locate the minimum bridge
   insertion points.
3. Build a Local Game Reflex v0:
   - no LLM in frame loop,
   - no WebDriver hot loop,
   - local non-sensitive game only,
   - measured observe-to-input latency.
4. Replace the temporary debug-state detector in
   `scripts/run_local_game_reflex_loop.js` with Browser Fact Stream facts plus
   bridge crop/pixel `visual_object_seen` facts, then reuse the same
   motor/replay harness.
