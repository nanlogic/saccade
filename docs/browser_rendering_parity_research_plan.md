# Saccade Browser Rendering Parity Research Plan

Date: 2026-06-12

Audience: GPT-5.5 / Claude Fable external review

## Question

Saccade's Servo-backed renderer looked like a mobile/stacked layout compared with Chrome on dashboard-style pages. We need an external review of the evidence and a recommendation for the next rendering strategy.

## Verified Facts

- The issue is not a mobile viewport. Latest Saccade worker truth reports `1280x800`.
- The local dashboard and layout probe rely on CSS Grid.
- Pinned Servo has `layout.grid.enabled` defaulting to `false`.
- Servo's own public status says CSS Grid is experimental and can be tried with `--pref layout.grid.enabled`.
- Servo's CSS Grid tracking issue reports initial support merged, but not full spec parity.

Sources:

- Servo Grid blog: `https://servo.org/blog/2024/12/09/this-month-in-servo/`
- Servo CSS Grid tracking: `https://github.com/servo/servo/issues/34479`
- Servo media-query tracking: `https://github.com/servo/servo/issues/39068`
- MDN Grid overview: `https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Grid_layout`

## Local Evidence

Artifacts:

- Grid off focused run: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290226025/index.html`
- Grid on focused run: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290279853/index.html`
- Grid on full gauntlet: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290368953/index.html`

Focused `layout_probe`:

```text
Grid off:
  max rect delta: 1126px
  display mismatches: 3
  grid-template mismatches: 17
  layout_grid display: Chrome grid -> Saccade block

Grid on:
  max rect delta: 4px
  display mismatches: 0
  grid-template mismatches: 0
  layout_grid display: Chrome grid -> Saccade grid
```

Full gauntlet, shared fixture diff ratio:

```text
dashboard        0.172743 -> 0.031496
form_controls    0.032255 -> 0.027271
modal_overlay    0.163102 -> 0.024039
scroll_sticky    0.069547 -> 0.053494
canvas_svg       0.123413 -> 0.108889
responsive_cards 0.039516 -> 0.015893
```

Interpretation: enabling Grid fixes the large layout-class failure. Remaining diffs are smaller and likely include font metrics, canvas/SVG rendering, sticky/scroll details, media-query coverage, and DPR/window-chrome differences.

## Current Implementation

- `scripts/visual_parity_compare.py --rendering-profile servo-safe|servo-modern|chrome-reference`
- `scripts/visual_parity_compare.py --saccade-grid on|off|default` remains as a legacy compatibility shim.
- `servo-modern` enables pinned Servo `Preferences::layout_grid_enabled`.
- Chrome and Saccade truth probes now emit `layoutProbes` for elements tagged with `data-saccade-probe`, including computed display, grid-template columns/rows, gaps, and rects.

## R1 Status

Accepted and implemented as `DECISION_RENDERING_001`:

- Added explicit rendering profiles.
- Preserved `SACCADE_SERVO_GRID=1` as a legacy override.
- Added `docs/rendering_strategy.md`.
- Added focused gate `scripts/validate_rendering_profiles.sh`.
- `chrome-reference` is a live-worker stub only; no Chrome adapter is implemented in R1.

## R2/R3 Status

Accepted and implemented:

- `mousemax run` and `formmax run` accept `--rendering-profile`.
- `servo-modern` passed the current MOUSEMAX arena and FORMMAX local gates.
- Dogfood and browser-session workers now default to `servo-modern`.
- `servo-safe` remains the explicit pinned-default baseline profile.

## Review Questions

1. Should Saccade default Servo Grid on for dogfood and worker profiles, while labeling it experimental?
2. What visual parity threshold is realistic for Servo-backed review versus Chrome-backed review?
3. Which remaining CSS/web-platform features should the gauntlet add next: media queries, container queries, sticky/fixed, transforms, fonts, forms, canvas/SVG, DPR?
4. Should public demos use Chrome/Firefox screenshots for page-content parity and Servo screenshots only for engine-truth/replay evidence?
5. Do we need a "Chrome renderer mode" for UI-design agents so designers see mainstream browser output, while Saccade still owns action maps, redaction, safety, and replay?

## Proposed Next Plan

1. Make rendering profile explicit:
   - `servo-safe`: current pinned defaults.
   - `servo-modern`: Grid enabled, future measured prefs allowed.
   - `chrome-reference`: Chrome CDP screenshot/truth artifacts for parity-sensitive workflows.

2. Default local dogfood to `servo-modern` only after one more gauntlet pass with:
   - layout probe,
   - dashboard,
   - forms,
   - modal/fixed overlay,
   - scroll/sticky,
   - canvas/SVG,
   - responsive/media-query pages.

3. Expand the report to classify diffs:
   - layout rect/style diff,
   - text/font diff,
   - raster/canvas diff,
   - action-map diff,
   - viewport/DPR diff.

4. Keep public wording conservative:
   - "Servo Grid pref fixes the major dashboard layout mismatch."
   - "Servo is not yet Chrome/Safari parity."
   - "Saccade will support Chrome-reference mode for UI-review and public demos."

5. Done: `servo-modern` is the dogfood/browser-session default after the MOUSEMAX/FORMMAX gates passed.
