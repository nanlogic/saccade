# Saccade Visual Parity Compare Report

Date: 2026-06-12

## What Was Added

Added a local visual parity gauntlet under `test_pages/visual_parity/` and a compare runner:

```bash
scripts/visual_parity_compare.py --timeout-sec 60
scripts/selftest_visual_parity.sh
```

The runner opens each fixture twice:

- Chrome path: `scripts/capture_chrome_reference.sh`
- Saccade path: `saccade-shell browser-session-worker --url ...`

It writes Chrome, Saccade, and diff screenshots plus `visual_parity_manifest.json` and an HTML report.

## Fixture Coverage

- `dashboard`: grid, sidebar, table, bars, action buttons.
- `form_controls`: inputs, select, number/date, checkbox/radio, textarea.
- `modal_overlay`: sticky toolbar, fixed overlay, dialog geometry.
- `scroll_sticky`: scroll containers, sticky header, long table.
- `canvas_svg`: canvas drawing and SVG rendering.
- `responsive_cards`: responsive grid, long token wrapping, swatches.

## Latest Evidence

Latest full run:

```text
runs/visual_parity/parity_1781288579228/index.html
```

Result:

```text
VISUAL PARITY PASS fixtures=6 report=/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781288579228/index.html
```

The run found that dimensions and action counts match across all six fixtures, but visual differences are still material. Dashboard is the clearest example: Chrome keeps a two-column dashboard layout, while Saccade currently flows the sidebar and cards differently. This confirms visual parity is a real product gap, not just a demo concern.

## Worker Fix

The first parity run exposed blank Saccade screenshots for complex pages even though DOM truth/action maps were correct. The worker now waits briefly after load completion and retries screenshot capture when the readback is nearly all white. This turns invalid blank artifacts into usable visual evidence.

## Still Pending

- Chrome-side click verification.
- Browser URL-bar parity artifacts for public demos.
- Firefox reference capture.
- Root-cause fixes for Servo/Chrome layout differences.
