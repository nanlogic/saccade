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
DEVMAX SERVO FIXTURES PASS total=4 detected=4 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/servo_selftest_1781223226303
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

Covered fixtures:

- `blank_page`
- `invisible_text`
- `offscreen_button`
- `modal_blocks_page`

## Important Boundary

This is `engine=servo-rendered-probe-v0`.

It does not use screenshot pixels yet, and it does not capture real browser console/network events yet. It does prove the report path can consume browser-computed layout/style truth instead of only static fixture markers.

## Runtime Note

Pinned Servo/winit cannot recreate an event loop multiple times in one process. The selftest runs each Servo audit in a child `devmax audit --engine servo --replay` process. This matches the existing Saccade pattern of isolating Servo window loops from orchestration code.
