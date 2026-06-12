# Saccade Visual Parity Compare Report

Date: 2026-06-12

## What Was Added

Added a local visual parity gauntlet under `test_pages/visual_parity/` and a compare runner:

```bash
scripts/visual_parity_compare.py --timeout-sec 60
scripts/visual_parity_compare.py --timeout-sec 60 --rendering-profile servo-modern
scripts/selftest_visual_parity.sh
```

The runner opens each fixture twice:

- Chrome path: `scripts/capture_chrome_reference.sh`
- Saccade path: `saccade-shell browser-session-worker --url ...`

It writes Chrome, Saccade, and diff screenshots plus Chrome hit-test evidence, browser-frame previews, `visual_parity_manifest.json`, and an HTML report.

The manifest now includes a first-pass diff classifier:

- `PASS_ACTION_GREEN`: action map and layout are acceptable for agent action.
- `PASS_ACTION_YELLOW_VISUAL`: action map/layout are acceptable, but use Chrome for polished visual review.
- `PASS_ACTION_YELLOW_RASTER`: action map/layout are acceptable, but use Chrome for raster/canvas/pixel judgement.
- `FAIL_LAYOUT`: layout differs enough to threaten coordinates.
- `FAIL_ACTION_MAP`: viewport or action map differs enough to block agent action.

The action-map part compares action count, labels, Saccade click-point escape distance against the Chrome reference rect, action rect geometry, and a Chrome-side hit-test for enabled non-sensitive Saccade actions. The hit-test uses `elementFromPoint` plus label/control rules and does not dispatch real clicks.

The browser-frame previews are labeled report wrappers around page-content screenshots. They are useful for public/demo review because the URL and browser context are visible, but they are not native browser UI screenshots.

## Fixture Coverage

- `layout_probe`: CSS Grid/Flex/layout probe with computed-style and rect metrics.
- `dashboard`: grid, sidebar, table, bars, action buttons.
- `form_controls`: inputs, select, number/date, checkbox/radio, textarea.
- `modal_overlay`: sticky toolbar, fixed overlay, dialog geometry.
- `scroll_sticky`: scroll containers, sticky header, long table.
- `canvas_svg`: canvas drawing and SVG rendering.
- `responsive_cards`: responsive grid, long token wrapping, swatches.

## Latest Evidence

Latest full run:

```text
runs/visual_parity/parity_1781300179891/index.html
```

Result:

```text
VISUAL PARITY PASS fixtures=7 report=/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781300179891/index.html
```

Grid validation found the main "mobile-looking" layout root cause:

- With Servo Grid off, `layout_probe` showed CSS Grid computed as block flow in Saccade. Max probe rect delta was `1126px`, with `3` display mismatches and `17` grid-template mismatches.
- With `servo-modern`, `layout_probe` matched Chrome Grid computed styles. Max probe rect delta fell to `4px`, with `0` display mismatches and `0` grid-template mismatches.
- Dashboard diff improved from `0.172743` to `0.031496`.

Shared-fixture diff ratio changes after enabling Grid:

```text
dashboard        0.172743 -> 0.031496
form_controls    0.032255 -> 0.027271
modal_overlay    0.163102 -> 0.024039
scroll_sticky    0.069547 -> 0.053494
canvas_svg       0.123413 -> 0.108889
responsive_cards 0.039516 -> 0.015893
```

This confirms the dashboard issue was not a mobile viewport. The latest worker truth still reports a `1280x800` viewport.

Latest classifier verdicts under `servo-modern`:

```text
layout_probe     PASS_ACTION_GREEN          hit=1/1  skipped=0 diff=0.022229 escape=0.0px rect=0.3px
dashboard        PASS_ACTION_YELLOW_VISUAL  hit=5/5  skipped=0 diff=0.031496 escape=0.0px rect=6.0px
form_controls    PASS_ACTION_YELLOW_VISUAL  hit=10/10 skipped=0 diff=0.027271 escape=5.2px rect=662.0px
modal_overlay    PASS_ACTION_YELLOW_VISUAL  hit=2/2  skipped=4 diff=0.024039 escape=0.0px rect=4.0px
scroll_sticky    PASS_ACTION_YELLOW_VISUAL  hit=11/11 skipped=0 diff=0.053494 escape=0.0px rect=1.7px
canvas_svg       PASS_ACTION_YELLOW_RASTER  hit=1/1  skipped=0 diff=0.108889 escape=0.0px rect=1.1px
responsive_cards PASS_ACTION_GREEN          hit=5/5  skipped=0 diff=0.015893 escape=0.0px rect=0.5px
```

Interpretation: current `servo-modern` is acceptable for local agent-action dogfood on these fixtures, while Chrome remains the correct reference for polished UI, raster/canvas, and public visual parity.

`form_controls` intentionally stays yellow: Servo reports much narrower rects for several native form controls, but the Saccade click points remain within the reference tolerance. That is acceptable for agent action and still not acceptable as a polished Chrome-lookalike claim.

`modal_overlay` verifies only two actions because the backdrop correctly blocks four page-level actions. Skipped actions are not agent-clickable; they remain present in truth/action-map evidence as blocked actions.

## Worker Fix

The first parity run exposed blank Saccade screenshots for complex pages even though DOM truth/action maps were correct. The worker now waits briefly after load completion and retries screenshot capture when the readback is nearly all white. This turns invalid blank artifacts into usable visual evidence.

## Still Pending

- Native Chrome/Safari browser-UI screenshots for public demos.
- Firefox reference capture.
- Root-cause remaining Servo/Chrome differences: font metrics, canvas/SVG, sticky/scroll, media-query coverage, and DPR/window chrome.
- Broaden classifier fixtures for media queries, transforms, container queries, overlays, and generated agent-built pages.
