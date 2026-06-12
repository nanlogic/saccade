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

It writes Chrome, Saccade, and diff screenshots plus `visual_parity_manifest.json` and an HTML report.

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
runs/visual_parity/parity_1781290368953/index.html
```

Result:

```text
VISUAL PARITY PASS fixtures=7 report=/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290368953/index.html
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

## Worker Fix

The first parity run exposed blank Saccade screenshots for complex pages even though DOM truth/action maps were correct. The worker now waits briefly after load completion and retries screenshot capture when the readback is nearly all white. This turns invalid blank artifacts into usable visual evidence.

## Still Pending

- Chrome-side click verification.
- Browser URL-bar parity artifacts for public demos.
- Firefox reference capture.
- Run MOUSEMAX and FORMMAX gates before making `servo-modern` the dogfood default.
- Root-cause remaining Servo/Chrome differences: font metrics, canvas/SVG, sticky/scroll, media-query coverage, and DPR/window chrome.
