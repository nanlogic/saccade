# Canvas / WebGL Runtime Probe Report

Date: 2026-06-14

## Goal

Move BP-011 from a vague backlog warning to a repeatable dogfood blocker with concrete evidence.

Saccade must be able to dogfood local games, visualizations, and canvas-heavy frontend work. If canvas/WebGL pages are slow or partially missing visual layers, Saccade cannot be the default browser for our own development.

## Live Game Probe

Target:

```text
http://127.0.0.1:4173/
```

Saccade command:

```sh
printf '{"id":1,"method":"audit"}\n{"id":2,"method":"close"}\n' | \
  RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker \
  --url http://127.0.0.1:4173/ \
  --width 1440 --height 900 \
  --rendering-profile servo-modern
```

Result:

```text
UNSUPPORTED (log once): POSSIBLE ISSUE: unit 1 GLD_TEXTURE_INDEX_2D is unloadable and bound to sampler type (Float) - using zero texture because texture unloadable
```

Artifacts:

- Saccade screenshot: `runs/browser_session_worker/worker_1781443202266_10321/audit_completed_rev1.png`
- Saccade replay: `runs/browser_session_worker/worker_1781443202266_10321/replay.jsonl`
- Chrome reference screenshot: `runs/webgl_runtime/chrome_game_reference_1781443202266/chrome_page.png`
- Chrome manifest: `runs/webgl_runtime/chrome_game_reference_1781443202266/chrome_reference_manifest.json`

Observation:

- Chrome shows the gameplay layer: grid, player/cup, strawberries, and projectiles.
- Saccade shows the HUD/title/background but misses the gameplay canvas layer in the captured frame.
- The Saccade run still produces an audit response and screenshot, so this is not a total browser crash. It is a rendering/runtime correctness blocker.

## Scripted Live-Game Pixel Probe

Added:

```sh
python3 scripts/probe_webgl_game_runtime.py \
  --url http://127.0.0.1:4173/ \
  --wait-sec 3 \
  --timeout-sec 75
```

The probe:

- captures Saccade and Chrome at the same viewport and wait time,
- copies both screenshots into one run directory,
- normalizes screenshots to a common CSS viewport before pixel comparison,
- checks the gameplay ROI for high-frequency edge structure and saturated visual content,
- records canvas/page structure from both engines,
- records whether Saccade emitted GL texture warnings,
- writes a machine-readable `report.json`.

Latest result:

```text
WEBGL_GAME_PROBE route=blocked_missing_gameplay_layer chrome_edge=0.036279 saccade_edge=0.000754 chrome_sat=0.002607 saccade_sat=0.004288 gl_warning=True diagnosis=render_pipeline_after_dom_ready report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/game_probe_1781449261494/report.json
```

Artifacts:

- Report: `runs/webgl_runtime/game_probe_1781449261494/report.json`
- Chrome screenshot: `runs/webgl_runtime/game_probe_1781449261494/chrome_page.png`
- Saccade screenshot: `runs/webgl_runtime/game_probe_1781449261494/saccade_page.png`
- Chrome CSS-normalized metric screenshot: `runs/webgl_runtime/game_probe_1781449261494/chrome_page_metric.png`
- Saccade CSS-normalized metric screenshot: `runs/webgl_runtime/game_probe_1781449261494/saccade_page_metric.png`
- Chrome page/canvas probe: `runs/webgl_runtime/game_probe_1781449261494/chrome/chrome_webgl_page_probe.json`
- Saccade page/canvas probe: `runs/webgl_runtime/game_probe_1781449261494/saccade_webgl_page_probe.json`

Observation:

- Chrome gameplay ROI has `edge_ratio=0.036279`; Saccade has `edge_ratio=0.000754` after CSS viewport normalization.
- Chrome layer is classified present; Saccade layer is classified missing.
- Both engines report one visible `canvas#game`, so the failure is not "DOM/script did not create the canvas."
- The live game canvas reports `context_type=none_or_2d`, so this blocker now covers the canvas/compositor/GL texture path, not only WebGL shader code.
- Chrome viewport/canvas is `1440x900 @ DPR 1`; Saccade viewport/canvas is `1440x759 CSS` with `2880x1518` backing at `DPR 2`.
- The latest Saccade run again captured `GLD_TEXTURE` / texture unloadable output.
- This gives BP-011 a repeatable red gate for the real game path.

## Minimal Fixture

Added:

```text
test_pages/webgl_runtime/index.html
```

The fixture draws:

- a 2D canvas gradient and circles,
- a WebGL textured quad,
- visible runtime status for 2D, WebGL context, shader, texture upload, `readPixels`, frame count, average frame time, and GL error.

Saccade command:

```sh
(sleep 3; printf '{"id":1,"method":"audit"}\n{"id":2,"method":"close"}\n') | \
  RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker \
  --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/webgl_runtime/index.html \
  --width 1000 --height 760 \
  --rendering-profile servo-modern
```

Result:

```text
UNSUPPORTED (log once): POSSIBLE ISSUE: unit 5 GLD_TEXTURE_INDEX_RECTANGLE is unloadable and bound to sampler type (Float) - using zero texture because texture unloadable
```

Fixture status visible in Saccade screenshot:

```text
canvas2d=ok
webglContext=ok
shader=ok
texture=ok
readPixels=ok_132_204_22
frames=3
avgFrameMs=135.22
lastError=none
```

Artifacts:

- Saccade screenshot: `runs/browser_session_worker/worker_1781443347692_12895/audit_completed_rev1.png`
- Saccade replay: `runs/browser_session_worker/worker_1781443347692_12895/replay.jsonl`

## Scripted Runtime Gate

Added:

```sh
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-webgl-runtime
```

The gate:

- opens the minimal fixture in the live `browser-session-worker`,
- waits for the page to draw frames,
- calls `webgl_runtime_probe`,
- runs an audit for screenshot/replay artifacts,
- captures GL texture warnings from worker stderr/stdout,
- prints `route=green` or `route=blocked`.

Latest result:

```text
WEBGL_RUNTIME DIAG route=green canvas2d=ok webgl_context=ok texture=ok read_pixels=ok_132_204_22 frames=30 avg_frame_ms=18.38 last_error=none gl_warning=false screenshot=runs/browser_session_worker/worker_1781445119728_26509/audit_completed_rev1.png replay=runs/browser_session_worker/worker_1781445119728_26509/replay.jsonl
```

## Interpretation

BP-011 is now a P1 dogfood blocker, not P2 polish.

Current evidence says:

- 2D canvas is healthy on the minimal fixture.
- Simple WebGL can create a context, upload a texture, draw, read pixels, and sustain a healthy scripted baseline on the minimal fixture.
- The live-game pixel probe now reproduces the missing gameplay layer after CSS viewport normalization.
- The live-game page/canvas probe narrows the failure to `render_pipeline_after_dom_ready`: both engines have a visible `canvas#game`, but Saccade misses the rendered gameplay pixels.
- The macOS GL path can still emit texture unloadable warnings under some Saccade/WebRender page paths.
- The real local game loses important gameplay visual layers in Saccade while Chrome shows them.

## Next Step

Debug the Saccade/Servo GL runtime path before broad game/canvas dogfood:

1. Use `scripts/probe_webgl_game_runtime.py` as the live-game red/green gate.
2. Add small reductions for the likely triggers: full-window Canvas2D, full-window Canvas2D with DPR backing scale, animation timing, CSS background plus canvas, and DOM HUD over canvas.
3. Keep WebGL reductions too, but classify the current live game as canvas/compositor/GL texture path until a WebGL context is actually observed.
4. Keep routing canvas/WebGL-heavy product judgement to Chrome/reference until the real game path is green too.
