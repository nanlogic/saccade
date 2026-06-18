# Canvas / WebGL Runtime Probe Report

Date: 2026-06-14

## Goal

Move BP-011 from a vague backlog warning to a repeatable dogfood blocker with concrete evidence.

Saccade must be able to dogfood local games, visualizations, and canvas-heavy frontend work. If canvas/WebGL pages are slow or partially missing visual layers, Saccade cannot be the default browser for our own development.

## 2026-06-14 Official Servo.app Check

Wayne manually tested the downloaded official Servo.app on the same local game:

```text
http://127.0.0.1:4173/
```

Result: official Servo.app can run the game.

This changes the interpretation of BP-011. The local game is not proven to be a
hard Servo engine limitation. The likely gap is now in Saccade's embedder path:
rendering context setup, surface/HiDPI sizing, WebView configuration, GL
initialization, or screenshot/readback/audit capture.

Wayne also checked `ign.com` in official Servo.app and saw the same bad behavior
as Saccade. Treat IGN as an upstream Servo/site compatibility limitation for now,
not a Saccade-specific productization blocker.

Next investigation target: make Saccade's embedded Servo path match official
Servo.app on the local game before spending time on broad real-site fixes.

Strategy record:

- `docs/servoshell_source_strategy.md`

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

### Fill-Mode Matrix

Added:

- Background variants for parametric canvas sizes:
  - default `gradient` background,
  - `solid` full-canvas fill,
  - `transparent` foreground drawing over DOM background.
- Runner preset `--preset fill`.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --preset fill \
  --repeat 2 \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest fill result:

```text
CANVAS_REDUCTIONS variants=12 blocked=2 green_or_review=10 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781457595886/report.json
```

Result matrix:

| Variant | Repeat | Route | Saccade backing | Saccade edge | Saccade saturation | GL warning |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| `bare-size-960x540` | 2/2 | `green_or_needs_review` | `960x540` | `0.025286` | `0.007344` | true |
| `bare-solid-size-960x540` | 2/2 | `green_or_needs_review` | `960x540` | `0.022429` | `0.007772` | true |
| `bare-transparent-size-960x540` | 2/2 | `green_or_needs_review` | `960x540` | `0.022759` | `0.007615` | true |
| `bare-size-1152x648` | 2/2 | `blocked_missing_gameplay_layer` | `1152x648` | `0.0` | `0.0` | false |
| `bare-solid-size-1152x648` | 2/2 | `green_or_needs_review` | `1152x648` | `0.027869` | `0.007582` | true |
| `bare-transparent-size-1152x648` | 2/2 | `green_or_needs_review` | `1152x648` | `0.028254` | `0.00746` | true |

Observation:

- At the previously stable red size (`1152x648`), only the gradient-backed variant is red.
- Solid full-canvas fill and transparent foreground drawing are both captured at `1152x648`.
- This narrows BP-011 from "large Canvas2D" to the large Canvas2D gradient/background paint path plus screenshot readback/presentation timing.
- The GL warning is inverted in this matrix: the red gradient runs have no warning, while the green solid/transparent runs do have warnings.

### Gradient Split Matrix

Added:

- `gradient2` and `gradient3` size variants.
- `gradient-only` variants without foreground shapes.
- Full-window `static` gradient versus `full-solid`.
- Runner preset `--preset gradient`.

Runner:

```sh
python3 scripts/probe_canvas_reductions.py \
  --preset gradient \
  --repeat 2 \
  --wait-sec 2 \
  --timeout-sec 75
```

Latest gradient result:

```text
CANVAS_REDUCTIONS variants=14 blocked=5 green_or_review=9 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781459417372/report.json
```

Result matrix:

| Variant | Repeat | Route | Saccade backing | Saccade edge | Saccade saturation | GL warning |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| `bare-gradient2-size-1152x648` | 2/2 | `blocked_missing_gameplay_layer` | `1152x648` | `0.0` | `0.0` | false |
| `bare-size-1152x648` | 1/2 | mixed | `1152x648` | `0.031669` / `0.0` | `0.007234` / `0.0` | mixed |
| `bare-gradient2-only-size-1152x648` | 2/2 | `green_or_needs_review` | `1152x648` | `0.0` | `0.0` | true |
| `bare-gradient3-only-size-1152x648` | 2/2 | `green_or_needs_review` | `1152x648` | `0.0` | `0.0` | true |
| `bare-solid-size-1152x648` | 2/2 | `green_or_needs_review` | `1152x648` | `0.027869` | `0.007582` | true |
| `static` | 2/2 | `blocked_missing_gameplay_layer` | `1440x759` | `0.0` | `0.0` | false |
| `full-solid` | 2/2 | `green_or_needs_review` | `1440x759` | `0.032745` | `0.006468` | true |

Observation:

- Two-stop linear gradient plus foreground is enough to produce stable red at `1152x648`.
- Full-window gradient plus foreground is stable red; full-window solid plus the same foreground is stable green.
- The three-stop gradient at `1152x648` is still unstable, flipping green/red across two repeats.
- Gradient-only variants are not proven green by the current gate. Chrome's gradient-only image has too little edge structure for the gameplay-layer classifier, so these variants need a different smooth-gradient metric before they can carry a verdict.
- The GL warning remains inverted: stable red gradient+foreground runs have no warning, while stable green solid runs do have warnings.

### Smooth-Gradient Metric

Added to `scripts/probe_webgl_game_runtime.py`:

- `max_channel_range`
- `luma_range`
- `luma_stdev`
- smooth-layer thresholds `min_smooth_channel_range=10.0` and `min_smooth_luma_range=4.0`

The classifier now uses the smooth metric only when Chrome does not have enough edge/saturation structure for the normal gameplay-layer gate. Foreground-rich pages still use the stricter edge/saturation path.

Verification:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-only-size-1152x648 bare-gradient2-size-1152x648 \
  --repeat 2 \
  --wait-sec 2 \
  --timeout-sec 75
```

Result:

```text
CANVAS_REDUCTIONS variants=4 blocked=2 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781461281103/report.json
```

Observation:

- `bare-gradient2-only-size-1152x648` is green across two repeats with Chrome smooth signal `channel_range=39`, `luma_range=14.666667` and Saccade smooth signal `channel_range=19`, `luma_range=8.333333`.
- `bare-gradient2-size-1152x648` remains red across two repeats: Chrome has foreground edge/saturation, while Saccade has `channel_range=0` and `luma_range=0`.
- This proves the gradient-only layer can be captured in Saccade. The stable red path is gradient plus foreground drawing, not smooth gradient alone.

### Gradient Ordering Matrix

Added to `test_pages/canvas_runtime/index.html` and the `gradient` preset:

- `bare-gradient2-foreground-first-size-1152x648`: draws foreground first, then paints the gradient behind it with `destination-over`.
- `bare-gradient2-delayed-foreground-size-1152x648`: paints the gradient first, then waits one animation frame before drawing foreground.

Verification:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-size-1152x648 \
    bare-gradient2-only-size-1152x648 \
    bare-gradient2-foreground-first-size-1152x648 \
    bare-gradient2-delayed-foreground-size-1152x648 \
  --repeat 2 \
  --wait-sec 2 \
  --timeout-sec 75
```

Result:

```text
CANVAS_REDUCTIONS variants=8 blocked=6 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781463104679/report.json
```

Result matrix:

| Variant | Repeat | Route | Saccade edge | Saccade saturation | Saccade channel range | Saccade luma range | GL warning |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| `bare-gradient2-size-1152x648` | 2/2 | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | `0` | `0.0` | false |
| `bare-gradient2-only-size-1152x648` | 2/2 | `green_or_needs_review` | `0.0` | `0.0` | `19` | `8.333333` | true |
| `bare-gradient2-foreground-first-size-1152x648` | 2/2 | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | `0` | `0.0` | false |
| `bare-gradient2-delayed-foreground-size-1152x648` | 2/2 | `blocked_missing_gameplay_layer` | `0.0` | `0.0` | `19` / `0` | `8.333333` / `0.0` | mixed |

Observation:

- Reversing draw order does not fix the captured-layer failure.
- Delaying foreground by one animation frame does not make foreground edge/saturation appear in the Saccade screenshot.
- One delayed-foreground run preserved the smooth gradient signal while still missing foreground, so the next useful split is page canvas backing/readPixels versus audit screenshot readback.

### Canvas Backing Versus Screenshot Readback

Added to `scripts/webgl_page_probe.js`:

- `pixelProbe` for 2D canvas backing pixels,
- sampled `edgeRatio`, `saturatedRatio`, `maxChannelRange`, `lumaRange`, and checksum,
- audit-first probe ordering so page-side `getImageData()` cannot warm up the screenshot gate.

Verification:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-size-1152x648 \
    bare-gradient2-delayed-foreground-size-1152x648 \
  --repeat 1 \
  --wait-sec 2 \
  --timeout-sec 75
```

Result:

```text
CANVAS_REDUCTIONS variants=2 blocked=2 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781464997166/report.json
```

Result matrix:

| Variant | Route | Saccade screenshot edge/sat | Saccade backing edge/sat | Saccade backing range/luma | Diagnosis |
| --- | --- | ---: | ---: | ---: | --- |
| `bare-gradient2-size-1152x648` | `blocked_missing_gameplay_layer` | `0.0` / `0.0` | `0.034318` / `0.011096` | `237` / `165.666667` | `screenshot_readback_after_canvas_backing` |
| `bare-gradient2-delayed-foreground-size-1152x648` | `blocked_missing_gameplay_layer` | `0.0` / `0.0` | `0.034173` / `0.01105` | `237` / `165.666667` | `screenshot_readback_after_canvas_backing` |

Observation:

- In both red Saccade runs, the page's 2D canvas backing contains foreground-like pixels.
- The audit screenshot drops those foreground pixels.
- BP-011 is now narrowed past page script, DOM readiness, and Canvas2D drawing into the embedder screenshot/readback/presentation path.

### Present Before Readback Attempt

Attempted a minimal worker change that called `RenderingContext::present()` after `WebView::paint()` and before manual `read_to_image()`.

Verification:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-size-1152x648 \
    bare-gradient2-delayed-foreground-size-1152x648 \
  --repeat 1 \
  --wait-sec 2 \
  --timeout-sec 75
```

Result:

```text
CANVAS_REDUCTIONS variants=2 blocked=2 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781465527374/report.json
```

Observation:

- `present()` did not fix the red reductions.
- The attempted runtime change was reverted.
- BP-011 remains actionable but should be parked while other browser productization work continues. The next BP-011 step, when resumed, is Servo `WebView::take_screenshot()` versus the manual `paint()+read_to_image()` audit path.

### Screenshot Path Comparison

AI-008 resumed with a focused path comparison runner:

```sh
python3 scripts/probe_canvas_screenshot_paths.py \
  --variants bare-gradient2-size-1152x648 bare-solid-size-1152x648 \
  --wait-sec 2 \
  --timeout-sec 75
```

Result:

```text
CANVAS_SCREENSHOT_PATHS variants=2 errors=0 manual_blocked=1 take_blocked=0 route=manual_readback_only report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_screenshot_paths_1781805458432/report.json
```

Evidence:

- Aggregate report: `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/report.json`
- Red manual readback screenshot: `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/bare-gradient2-size-1152x648/saccade_manual_readback.png`
- Red-case Servo `take_screenshot()` image: `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/bare-gradient2-size-1152x648/saccade_take_screenshot.png`
- Red-case Chrome reference: `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/bare-gradient2-size-1152x648/chrome_page.png`

Observation:

- In `bare-gradient2-size-1152x648`, the manual audit path
  `paint()+read_to_image()` produced a blank/white image:
  `edge_ratio=0.0`, `saturated_ratio=0.0`, `luma_range=0.0`.
- The same page and same worker captured the foreground correctly through
  Servo `WebView::take_screenshot()`:
  `edge_ratio=0.028048`, `saturated_ratio=0.007514`,
  `luma_range=165.666667`.
- Page-side canvas backing also contained the foreground:
  `edgeRatio=0.034318`, `saturatedRatio=0.011096`,
  `lumaRange=165.666667`.
- Control variant `bare-solid-size-1152x648` stayed green on both manual
  readback and `take_screenshot()`.

Conclusion:

- BP-011 is not a generic Canvas2D draw failure and not a blanket Servo
  `take_screenshot()` failure.
- The red reduction is currently a manual diagnostic readback/presentation
  failure. For non-hot, non-sensitive visual evidence, use
  `WebView::take_screenshot()` or Chrome/reference instead of relying only on
  manual `read_to_image()`.
- The reflex hot loop still cannot use `take_screenshot()` because it is
  asynchronous and waits for stable rendering. The remaining launch-risk split
  is to test whether the reflex/agent readback needs a different frame-ready or
  readback sequencing path, while diagnostic screenshots can route to the
  green Servo screenshot API.

### Diagnostic Screenshot Routing

The Canvas runners now separate non-hot diagnostic screenshots from the
manual/readback gate:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-size-1152x648 \
  --wait-sec 2 \
  --timeout-sec 75
```

Default result:

```text
CANVAS_REDUCTIONS variants=1 blocked=0 green_or_review=1 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781806451861/report.json
```

This uses `saccade_screenshot_method=take_screenshot` for the local
`file://` fixture. The same variant can still force the manual readback gate:

```sh
python3 scripts/probe_canvas_reductions.py \
  --variants bare-gradient2-size-1152x648 \
  --saccade-screenshot-mode manual \
  --wait-sec 2 \
  --timeout-sec 75
```

Manual result:

```text
CANVAS_REDUCTIONS variants=1 blocked=1 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781806531266/report.json
```

Interpretation:

- `take-local` is now the default for local fixture diagnostics, so reports do
  not mistake manual readback blankness for missing page canvas pixels.
- `manual` remains the explicit red gate for the low-latency
  `paint()+read_to_image()` path.
- Non-local URLs do not use the local-only screenshot method by default; the
  runner falls back to the existing manual audit behavior.

### Reflex Readback Gate

The source ServoShell reflex bridge now records lightweight readback sample
metrics:

- `sample_saturated`
- `sample_max_channel_range`
- `sample_luma_range`

These are per-frame summaries from the actual reflex
`RenderingContext::read_to_image()` path. They do not write screenshots or raw
pixels.

Positive gate:

```sh
node scripts/probe_reflex_readback_canvas.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/debug/servoshell \
  --variant bare-gradient2-size-1152x648 \
  --duration-ms 2500 \
  --window-size 1440x900
```

Result:

```text
REFLEX_READBACK_CANVAS route=readback_foreground_present ok=true frames=5 readback_ok=5 max_channel_range=235 max_luma_range=158 max_saturated_ratio=0.006338 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/reflex_readback_canvas_1781806982624/report.json
```

Negative control:

```sh
node scripts/probe_reflex_readback_canvas.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/debug/servoshell \
  --variant bare-gradient2-only-size-1152x648 \
  --duration-ms 1800 \
  --window-size 1440x900
```

Result:

```text
REFLEX_READBACK_CANVAS route=readback_blank_or_flat ok=false frames=5 readback_ok=5 max_channel_range=19 max_luma_range=9 max_saturated_ratio=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/reflex_readback_canvas_1781807000176/report.json
```

Conclusion:

- The source ServoShell reflex readback path can see the Canvas2D foreground in
  the focused reduction.
- The gate rejects a gradient-only control, so the pass is not just "any
  non-white page."
- The remaining Canvas/WebGL caution is now about broader live-game/site
  coverage, not a blanket inability for reflex `read_to_image()` to observe
  Canvas2D foreground.

### AI-008D Live Local Game Reflex Gate

AI-008D expands the focused Canvas2D reflex readback gate to the real local
game at `http://127.0.0.1:4173/`.

Important split:

- Downloaded official Servo.app runs the game at normal speed but does not have
  Saccade's `SACCADE_REFLEX_*` command/readback bridge.
- Source debug ServoShell has the bridge but is not accepted as product
  performance evidence for this gate.
- Source release ServoShell is the accepted target because it has both normal
  game timing and the in-process reflex bridge.

Command:

```sh
node scripts/run_local_game_reflex_loop.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 15000 \
  --policy visual \
  --output-dir runs/local_game_reflex/ai008d_live_game_release_1781810191
```

Result:

```text
AI-008D route=live_game_reflex_readback_green ok=true readback=1292/1292 semantic_facts=176 visual_objects=176 commands=53 receipts=53 time_scale=0.989 fill_delta=12 hp_delta=0 report=runs/local_game_reflex/ai008d_live_game_release_1781810191/report.json
```

Artifacts:

- Report: `runs/local_game_reflex/ai008d_live_game_release_1781810191/report.json`
- Review: `runs/local_game_reflex/ai008d_live_game_release_1781810191/review.html`
- Replay: `runs/local_game_reflex/ai008d_live_game_release_1781810191/replay.jsonl`
- Browser facts: `runs/local_game_reflex/ai008d_live_game_release_1781810191/facts.jsonl`
- Semantic facts: `runs/local_game_reflex/ai008d_live_game_release_1781810191/semantic_facts.jsonl`

Conclusion:

- The source-release reflex path sees live-game Canvas2D foreground through the
  actual hot-loop readback channel.
- The same artifact proves generic Browser Fact Stream visual objects,
  local-game semantic facts, visual-policy motor commands, command receipts,
  and stable game time.
- AI-008D closes the local-game reflex readback concern. BP-011 remains in
  monitoring mode for broad third-party Canvas/WebGL sites until each site has
  its own evidence artifact.

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

BP-011 is now in monitoring mode rather than a blanket "Canvas is broken" gate.

Current evidence says:

- Non-hot diagnostics should prefer Servo `WebView::take_screenshot()` or Chrome/reference. The local Canvas2D red reduction is green through `take_screenshot()` and red only through the old manual diagnostic path.
- The source ServoShell reflex bridge can see the focused Canvas2D foreground through the actual `RenderingContext::read_to_image()` path.
- The source-release ServoShell reflex bridge also passes the live local game
  gate with foreground readback, semantic facts, motor receipts, and normal
  game timing in one artifact.
- Simple WebGL can create a context, upload a texture, draw, read pixels, and sustain a healthy scripted baseline on the minimal fixture.
- Broad canvas/WebGL-heavy site judgement still needs per-site or per-game evidence. The macOS GL path can emit texture unloadable warnings, and some real pages can still stall or fail independently of Saccade-specific code.

## Next Step

Keep the evidence split explicit:

1. Use `scripts/probe_webgl_game_runtime.py` for live-game red/green checks.
2. Use `scripts/probe_canvas_reductions.py` for diagnostic screenshot checks, defaulting to `take-local`.
3. Use `scripts/probe_reflex_readback_canvas.js` for the source ServoShell reflex readback gate.
4. Use `scripts/run_local_game_reflex_loop.js --policy visual` as the live local-game AI-008D gate when the game server is running.
5. Route broad canvas/WebGL-heavy product judgement to Chrome/reference until the specific target has a Saccade evidence artifact.
