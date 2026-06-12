# Saccade Rendering Strategy

Date: 2026-06-12

## Decision

Saccade does not promise Servo/Chrome pixel parity.

Saccade uses rendering profiles and routes each decision to the renderer that is appropriate for that decision.

## Profiles

### servo-safe

Pinned Servo defaults.

Use this as the baseline and regression control. Experimental rendering prefs are off.

### servo-modern

Pinned Servo plus measured experimental prefs that improve agent-action correctness.

Current measured pref:

```text
layout.grid.enabled = true
```

Reason: local visual parity evidence showed CSS Grid disabled caused dashboard layouts to fall back to block flow. Enabling Grid changed `layout_probe` max rect delta from `1126px` to `4px` and reduced dashboard diff from `0.172743` to `0.031496`.

### chrome-reference

Chrome-rendered page-content screenshot and redacted truth artifacts.

Use this when the decision is what a mainstream browser user sees: UI design review, public demos, and pixel-sensitive reports. This profile is a configuration stub in the live worker path until the Chrome adapter is implemented.

## Decision Boundary

| Decision | Default |
|---|---|
| Browser truth and replay evidence | Servo |
| Agent action map and local workflow dogfood | `servo-modern` |
| Safety policy for sensitive fields/actions | Saccade policy layer |
| UI design parity and public screenshots | `chrome-reference` |
| Pixel-perfect CSS/raster judgement | `chrome-reference` |

Visual parity reports now classify Servo-vs-Chrome diffs by decision impact:

- Green: acceptable for agent action.
- Yellow visual/raster: acceptable for agent action, but route polished UI or pixel judgement to `chrome-reference`.
- Red layout/action-map: do not trust Servo coordinates until investigated or rerouted.

The classifier uses action count/labels, Saccade click-point escape distance against the Chrome reference rect, Chrome-side non-mutating hit-tests for enabled non-sensitive Saccade actions, layout probes, screenshot dimensions, and raster/text diff ratios.

## Rules

- Never claim "Servo renders like Chrome."
- Keep `servo-safe` as the pinned baseline.
- Enable Servo experimental prefs only in named profiles and only with local evidence.
- Every Servo re-pin must rerun the rendering gauntlet and record profile/pref changes.
- A renderer crash is a reportable result. It should recommend `chrome-reference` when visual parity is the user's question.

## Gate

Focused profile validation:

```bash
scripts/validate_rendering_profiles.sh
```

Expected line:

```text
RENDERING_PROFILE PASS servo_safe_recorded=true servo_modern_grid=true layout_probe_modern_max_delta_px=<n> default_worker_profile=servo-modern
```

Broader gates used before making `servo-modern` the dogfood default:

```bash
cargo run -q -p mousemax -- run --site arena --spawn-speed Epic --target-size Tiny --duration 15 --seed 42 --replay --rendering-profile servo-modern
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay --rendering-profile servo-modern
```

The reason is simple: rendering prefs can change layout rects, and changed rects can change click coordinates.

## R2 Gate Results

`servo-modern` has passed the focused profile gate plus the current MOUSEMAX and FORMMAX local gates.

```text
RENDERING_PROFILE PASS servo_safe_recorded=true servo_modern_grid=true layout_probe_modern_max_delta_px=4.0
MOUSEMAX arena servo-modern: PASS hits=45 misses=0 targets_seen=45 false_positive_clicks=0 stale_clicks=0
FORMMAX servo-modern: PASS rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true validation_errors=0 replay_value_leaks=0
```

Artifacts:

- `/Users/waynema/Documents/GitHub/SACCADE/runs/arena/run_1781294025/result.json`
- `/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781294062952/result.json`

Dogfood and browser-session workers now default to `servo-modern`. `servo-safe` remains available as an explicit baseline profile.

Latest visual classifier evidence:

```text
/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781299261779/index.html
```

The current seven-fixture local gauntlet has no red verdicts under `servo-modern`, and all enabled non-sensitive Saccade action points hit their expected Chrome targets. It still contains visual/raster yellow verdicts, so `chrome-reference` remains mandatory for public visual parity and UI design review.
