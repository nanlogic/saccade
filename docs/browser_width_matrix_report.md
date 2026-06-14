# Browser Width Matrix Report

Date: 2026-06-13

Command:

```bash
python3 scripts/visual_parity_width_matrix.py grid_percent_100_50 form_controls responsive_cards --widths 390 768 1000 1280 1600 --height 700 --timeout-sec 90 --rendering-profile servo-modern
```

Report:

- `runs/visual_parity_width_matrix/matrix_1781407462010/index.html`
- `runs/visual_parity_width_matrix/matrix_1781407462010/width_matrix_manifest.json`

## What This Tested

Chrome and Saccade were compared at several CSS viewport widths against local fixtures for:

- CSS Grid percentage and `100%` / `50%` sizing.
- Native form control sizing inside grid layouts.
- A known-good responsive card page as a control.

Each row captures Chrome and Saccade screenshots, normalizes Retina Saccade screenshots when the scale is uniform, compares pixels, compares layout probe rects, compares action maps, and verifies Saccade click points in Chrome.

## Matrix Result

| Width | `grid_percent_100_50` | `form_controls` | `responsive_cards` |
| --- | --- | --- | --- |
| 390 | `FAIL_ACTION_MAP`, 1/2 Chrome hit-tests | `FAIL_ACTION_MAP`, 5/7 Chrome hit-tests | `PASS_ACTION_YELLOW_VISUAL`, 4/4 hit-tests |
| 768 | `FAIL_LAYOUT`, 2/2 hit-tests | `FAIL_LAYOUT`, 10/10 hit-tests | `PASS_ACTION_YELLOW_VISUAL`, 5/5 hit-tests |
| 1000 | `FAIL_LAYOUT`, 2/2 hit-tests | `FAIL_LAYOUT`, 10/10 hit-tests | `PASS_ACTION_YELLOW_VISUAL`, 5/5 hit-tests |
| 1280 | `FAIL_LAYOUT`, 2/2 hit-tests | `FAIL_LAYOUT`, 10/10 hit-tests | `PASS_ACTION_GREEN`, 5/5 hit-tests |
| 1600 | invalid for fair judgement | invalid for fair judgement | invalid for fair judgement |

The 1600 run requested `1600x700`, but Saccade captured `2880x1400`, which is `1440x700 @2x`; Chrome captured `1600x700`. That means the current macOS worker window is capped at 1440 CSS px in this session. Treat 1600 as a viewport/window-product issue, not as a layout verdict.

## Main Findings

The Retina normalization path works for 390, 768, 1000, and 1280 CSS widths. Saccade screenshots were captured at exactly 2x and normalized before pixel comparison.

`responsive_cards` is the control case: it stays action-safe from 390 through 1280 and reaches green at 1280. That tells us the compare harness itself is useful and not just producing noise.

The real bug class is native control sizing in grid/form contexts. Chrome expands `input`, `select`, `date`, `number`, and `textarea` to fill the available grid column in these fixtures. Saccade/Servo often keeps intrinsic widths such as about `136.5px` for inputs and `168px` for textareas. This matches the manual symptom: resizing the browser makes the top bar better, but many page controls do not visually or interactively stretch like Chrome.

At 390 CSS px, this becomes action-unsafe for `form_controls`: only 5 of 7 non-sensitive Saccade click points hit the same Chrome targets, with a maximum click escape of about `52.9px`.

At 768, 1000, and 1280 CSS px, hit-tests pass for `form_controls`, but layout deltas remain large. So wider desktop can be usable for some actions, but not trustworthy for UI-design review or polished browser dogfood yet.

## Current Interpretation

Likely source is a mix of fixture/page CSS and Servo/native-control behavior:

- The fixture does not explicitly set `width: 100%` on all form controls.
- Chrome's UA/control behavior makes those controls visually fill their grid areas.
- Saccade/Servo keeps some controls at intrinsic width.

Next we should split this into a smaller fixture with `auto`, `width:100%`, `min-width:0`, grid, and flex variants. If explicit CSS fixes Saccade, this is mostly page-CSS/classification work. If explicit CSS still differs, it becomes Servo/layout-control compatibility work or a Chrome-route case.

## Next Steps

1. Add `form_control_width_modes`: auto versus `width:100%`, grid versus flex, with per-control computed style and rect output.
2. Add computed-style fields to parity reports: width, min/max width, box sizing, overflow, grid template, and font metrics.
3. Fix or route the 390-width action-map failure first, because that is the current red safety gate.
4. Add a worker/fullscreen or display-boundary probe for widths above 1440 CSS px before using 1600/1920 as product benchmarks.
