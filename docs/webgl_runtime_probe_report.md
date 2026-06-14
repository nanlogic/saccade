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

## Canvas2D Reductions

Added:

```text
test_pages/canvas_runtime/index.html
```

The fixture draws a full-window Canvas2D scene with variants for:

- `static`: synchronous static draw with CSS-sized backing scale `1`.
- `dpr`: synchronous static draw with DPR-scaled canvas backing.
- `animated`: `requestAnimationFrame` redraw loop.
- `hud`: DPR-scaled canvas plus DOM HUD overlay.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants static dpr animated hud \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest result:

```text
CANVAS_REDUCTIONS variants=4 blocked=4 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781450500515/report.json
```

Artifacts:

- Aggregate report: `runs/webgl_runtime/canvas_reductions_1781450500515/report.json`
- Static variant report: `runs/webgl_runtime/game_probe_1781450500546/report.json`
- Static Chrome metric screenshot: `runs/webgl_runtime/game_probe_1781450500546/chrome_page_metric.png`
- Static Saccade metric screenshot: `runs/webgl_runtime/game_probe_1781450500546/saccade_page_metric.png`

Observation:

- All four Canvas2D variants route `blocked_missing_gameplay_layer`.
- The static full-window Canvas2D variant is enough to reproduce the missing-layer failure.
- Chrome static Canvas2D has `edge_ratio=0.052731` and `saturated_ratio=0.005621`; Saccade has `edge_ratio=0.0` and `saturated_ratio=0.0`.
- Both engines report one visible `canvas#game`; Saccade still captures a blank gameplay ROI.
- These reductions did not emit the GL texture warning (`gl_warning=false`), so the warning is correlated with some live-game paths but is not required for the Canvas2D missing-layer failure.
- DPR backing scale, animation timing, and DOM HUD overlay are not required triggers.

### Sizing / Backing Matrix

Added variants:

- `small-static`: centered `720x420` CSS canvas with 1x backing.
- `small-dpr`: centered `720x420` CSS canvas with DPR backing.
- `small-attribute`: centered `720x420` attribute-sized canvas.
- `alpha-false`: full-window Canvas2D with `alpha:false`.
- `dom-background`: full-window transparent Canvas2D over a DOM background.
- `dpr-no-transform`: full-window DPR backing without `ctx.setTransform`.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --preset sizing \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest result:

```text
CANVAS_REDUCTIONS variants=7 blocked=4 green_or_review=3 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781452258085/report.json
```

Result matrix:

| Variant | Route | Saccade edge | Saccade saturation | GL warning |
| --- | --- | ---: | ---: | --- |
| `static` | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | false |
| `small-static` | `green_or_needs_review` | `0.021242` | `0.007302` | false |
| `small-dpr` | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | false |
| `small-attribute` | `green_or_needs_review` | `0.021129` | `0.007302` | false |
| `alpha-false` | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | false |
| `dom-background` | `green_or_needs_review` | `0.033431` | `0.006314` | true |
| `dpr-no-transform` | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | false |

Observation:

- Saccade can capture Canvas2D content: `small-static` and `small-attribute` are green enough for review.
- Full-window opaque/background-painted Canvas2D remains red even with 1x backing and no GL warning.
- DPR backing makes the smaller canvas red too, so DPR/backing texture size is a separate trigger.
- `dom-background` is green despite one GL warning, so the warning is neither required nor sufficient for the captured-layer failure.
- Attribute-sized small canvas behaves like CSS-sized small canvas at 1x, so CSS sizing alone is not the current red trigger.

### Size Threshold Matrix

Added:

- Parametric variants such as `size-960x540`, `size-1152x648`, and `dpr-size-360x210`.
- Runner preset `--preset threshold`.
- Aggregate report fields for largest canvas CSS rect and backing size.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --preset threshold \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest result:

```text
CANVAS_REDUCTIONS variants=7 blocked=5 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781454026421/report.json
```

Result matrix:

| Variant | Route | Saccade rect | Saccade backing | Saccade edge | Saccade saturation |
| --- | --- | ---: | ---: | ---: | ---: |
| `small-static` | `green_or_needs_review` | `724x424` | `722x422` | `0.021242` | `0.007302` |
| `size-960x540` | `green_or_needs_review` | `964x544` | `962x542` | `0.028963` | `0.007318` |
| `size-1152x648` | `blocked_missing_gameplay_layer` | `1156x652` | `1154x650` | `0.0` | `0.0` |
| `size-1280x720` | `blocked_missing_gameplay_layer` | `1284x724` | `1282x722` | `0.0` | `0.0` |
| `static` | `blocked_missing_gameplay_layer` | `1440x759` | `1440x759` | `0.0` | `0.0` |
| `dpr-size-360x210` | `blocked_missing_gameplay_layer` | `364x214` | `724x424` | `0.00967` | `0.007117` |
| `small-dpr` | `blocked_missing_gameplay_layer` | `724x424` | `1444x844` | `0.0` | `0.0` |

Observation:

- The 1x opaque Canvas2D failure threshold is between roughly `962x542` and `1154x650` backing pixels on this machine.
- DPR backing is a separate risk: even `364x214 CSS` / `724x424 backing` is just below the edge threshold and routes red.
- The full-window live-game failure is consistent with the 1x size threshold: Saccade reports `1440x759` for the static reduction and captures no gameplay ROI pixels.
- The current matrix measures screenshot/pixel evidence, not whether the live human window visually presents the layer.

### Bare Threshold / Repeatability Matrix

Added:

- Borderless/no-shadow variants such as `bare-size-1024x576`.
- Runner preset `--preset threshold-bare`.
- Runner option `--repeat N` to catch presentation/readback flakes without hand-running the same variant.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --preset threshold-bare \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest bare result:

```text
CANVAS_REDUCTIONS variants=6 blocked=2 green_or_review=4 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781455791930/report.json
```

Bare matrix:

| Variant | Route | Saccade backing | Saccade edge | Saccade saturation | GL warning |
| --- | --- | ---: | ---: | ---: | --- |
| `bare-size-960x540` | `green_or_needs_review` | `960x540` | `0.025286` | `0.007344` | true |
| `bare-size-1024x576` | `blocked_missing_gameplay_layer` | `1024x576` | `0.0` | `0.0` | false |
| `bare-size-1088x612` | `green_or_needs_review` | `1088x612` | `0.029584` | `0.007261` | true |
| `bare-size-1152x648` | `blocked_missing_gameplay_layer` | `1152x648` | `0.0` | `0.0` | false |
| `dpr-bare-size-360x210` | `green_or_needs_review` | `720x420` | `0.007457` | `0.007156` | true |
| `dpr-bare-size-480x270` | `green_or_needs_review` | `960x540` | `0.011096` | `0.007284` | true |

Repeatability check:

```text
CANVAS_REDUCTIONS variants=3 blocked=2 green_or_review=1 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781455904824/report.json
CANVAS_REDUCTIONS variants=2 blocked=0 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781456029665/report.json
```

Observation:

- Removing border/shadow makes canvas rect and backing size exact.
- `1152x648` is red across both bare-threshold passes.
- `960x540` is green in the bare-threshold pass.
- Midpoints are not stable: `1024x576` flipped from red to green, and `1088x612` flipped from green to red.
- `--repeat 2` on `bare-size-1024x576` produced two green runs in the latest smoke.
- The failure should now be treated as size/backing plus presentation/readback timing, not a clean monotonic size threshold yet.
- The GL warning remains an unreliable classifier: it appeared in several green runs and was absent in several red runs.

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

- 2D canvas is healthy on the small minimal fixture, but full-window Canvas2D is red in the new reductions.
- Small 1x Canvas2D reductions are captured correctly, which narrows BP-011 away from "all Canvas2D is broken."
- Full-window opaque/background-painted Canvas2D and DPR-backed Canvas2D are the current minimal red triggers.
- 1x opaque Canvas2D goes red between about `962x542` and `1154x650` backing pixels in the current screenshot path.
- Bare repeatability checks show mid-size results can flip, so BP-011 must treat screenshot readback/presentation timing as part of the bug.
- Simple WebGL can create a context, upload a texture, draw, read pixels, and sustain a healthy scripted baseline on the minimal fixture.
- The live-game pixel probe now reproduces the missing gameplay layer after CSS viewport normalization.
- The Canvas2D reductions reproduce the same missing gameplay-layer symptom without requiring WebGL context creation or GL texture warnings.
- The live-game page/canvas probe narrows the failure to `render_pipeline_after_dom_ready`: both engines have a visible `canvas#game`, but Saccade misses the rendered gameplay pixels.
- The macOS GL path can still emit texture unloadable warnings under some Saccade/WebRender page paths.
- The real local game loses important gameplay visual layers in Saccade while Chrome shows them.

## Next Step

Debug the Saccade/Servo canvas/runtime path before broad game/canvas dogfood:

1. Use `scripts/probe_webgl_game_runtime.py` as the live-game red/green gate.
2. Use `scripts/probe_canvas_reductions.py` as the Canvas2D red gate.
3. Use `--repeat` for BP-011 reduction gates and classify only stable red/green results.
4. Split the background trigger into solid fill, gradient fill, transparent canvas plus shapes, and full-canvas clear/fill behavior.
5. Inspect screenshot readback flushing/settle timing and compare Saccade screenshot readback versus live window presentation if measurable.
6. Keep WebGL reductions too, but classify the current live game as canvas/compositor/paint presentation until a WebGL context is actually observed.
7. Keep routing canvas/WebGL-heavy product judgement to Chrome/reference until the real game path is green too.
