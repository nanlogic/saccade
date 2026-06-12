# Saccade Demo Comparison Pack

Date: 2026-06-12

## What Was Added

Added two public-demo artifact scripts:

```bash
scripts/capture_native_browser_ui.py --browser chrome --url <url> --output-dir <dir>
scripts/capture_native_browser_ui.py --browser firefox --url <url> --output-dir <dir>
scripts/build_demo_comparison_pack.py --fixtures dashboard --timeout-sec 60
```

`capture_native_browser_ui.py` attempts a real macOS browser-window screenshot with native browser chrome through AppleScript plus `screencapture`.

`build_demo_comparison_pack.py` combines:

- native Chrome/Safari browser UI capture attempts,
- direct Saccade worker screenshot evidence from the visual parity run,
- Chrome page-content and pixel-diff thumbnails for comparison,
- Chrome hit-test verification summary,
- a single `demo_review.html` for public review.

## Latest Evidence

```text
/Users/waynema/Documents/GitHub/SACCADE/runs/demo_pack/demo_1781306995672/demo_review.html
```

Current machine result:

```text
chrome native UI: captured
safari native UI: captured
firefox native UI: capture_unavailable (Firefox is not installed on this machine)
layout_probe visual parity: PASS_ACTION_GREEN, Chrome hit-test 1/1
dashboard visual parity: PASS_ACTION_YELLOW_VISUAL, Chrome hit-test 5/5
form_controls visual parity: PASS_ACTION_YELLOW_VISUAL, Chrome hit-test 10/10
modal_overlay visual parity: PASS_ACTION_YELLOW_VISUAL, Chrome hit-test 2/2, skipped blocked actions 4
scroll_sticky visual parity: PASS_ACTION_YELLOW_VISUAL, Chrome hit-test 11/11
canvas_svg visual parity: PASS_ACTION_YELLOW_RASTER, Chrome hit-test 1/1
responsive_cards visual parity: PASS_ACTION_GREEN, Chrome hit-test 5/5
total Chrome hit-test: 35/35, skipped blocked actions 4
Saccade worker screenshots: embedded in demo_review.html
```

The demo pack serves the default local fixture over `127.0.0.1` for native browser UI capture. This avoids Safari's `file://` load confirmation dialog while keeping the visual parity runner's normal fixture flow unchanged.

If native capture fails, it usually means macOS Screen Recording permission is missing:

```text
macOS screen capture is unavailable; grant Screen Recording permission to the terminal/Codex app
```

The pack still succeeds in that case because this is a public-demo artifact path. It records missing native screenshots as structured evidence instead of pretending they exist.

## How To Rerun With Native Screenshots

Grant macOS Screen Recording permission to the terminal/Codex host app, then rerun:

```bash
scripts/build_demo_comparison_pack.py --fixtures dashboard --timeout-sec 60
```

For a fuller local review:

```bash
scripts/build_demo_comparison_pack.py --fixtures all --native-browsers chrome safari firefox --timeout-sec 60
```

## Boundary

Native browser UI screenshots are for public/demo credibility. They do not replace Saccade truth, safety policy, replay, or Chrome hit-test verification.
