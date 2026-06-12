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
DEVMAX SERVO FIXTURES PASS total=5 detected=5 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/servo_selftest_1781223967984
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

Covered fixtures:

- `blank_page`
- `invisible_text`
- `offscreen_button`
- `modal_blocks_page`
- `canvas_chart_blank`

## Important Boundary

This is `engine=servo-rendered-probe-v0`.

It uses screenshot pixels for canvas-region blank checks. It does not capture real browser console/network events yet. It does prove the report path can consume browser-computed layout/style truth and screenshot evidence instead of only static fixture markers.

## Runtime Note

Pinned Servo/winit cannot recreate an event loop multiple times in one process. The selftest runs each Servo audit in a child `devmax audit --engine servo --replay` process. This matches the existing Saccade pattern of isolating Servo window loops from orchestration code.
