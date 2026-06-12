# Saccade DEVMAX Servo Probe Report

Date: 2026-06-11

## Result

DEVMAX now has a first Servo-backed rendered truth gate.

Command:

```bash
cargo run -q -p devmax -- selftest-servo-fixtures
```

Observed output:

```text
DEVMAX SERVO FIXTURES PASS total=8 detected=8 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/servo_selftest_1781224856481
```

## What It Tests

The Servo probe opens each fixture in a real Servo WebView and evaluates compact browser truth:

- document title and URL
- body text length and child count
- viewport size
- interactive element rectangles
- offscreen interactive controls
- computed foreground/background colors
- overlay blockers covering action centers
- screenshot pixel checks for canvas regions
- real mouse click verification for enabled actions
- Servo WebViewDelegate console messages
- Servo WebViewDelegate resource load requests

Covered fixtures:

- `blank_page`
- `invisible_text`
- `offscreen_button`
- `modal_blocks_page`
- `canvas_chart_blank`
- `button_no_handler`
- `console_error`
- `missing_asset`

## Important Boundary

This is `engine=servo-rendered-probe-v0`.

It uses screenshot pixels for canvas-region blank checks, real Servo mouse input for first-action click verification, and Servo delegate hooks for console/resource-load capture. Resource-load capture currently records request metadata; it does not yet include final HTTP status codes.

## Runtime Note

Pinned Servo/winit cannot recreate an event loop multiple times in one process. The selftest runs each Servo audit in a child `devmax audit --engine servo --replay` process. This matches the existing Saccade pattern of isolating Servo window loops from orchestration code.
