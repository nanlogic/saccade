# Form Control Width Modes Report

Date: 2026-06-13

Command:

```bash
python3 scripts/visual_parity_width_matrix.py form_control_width_modes --widths 390 768 1280 --height 700 --timeout-sec 90 --rendering-profile servo-modern
```

Report:

- `runs/visual_parity_width_matrix/matrix_1781408488883/index.html`
- `runs/visual_parity_width_matrix/matrix_1781408488883/width_matrix_manifest.json`

## Result

| Width | Verdict | Chrome Hit-Test | Max Click Escape | Strict Control Escape |
| --- | --- | --- | --- | --- |
| 390 | `FAIL_ACTION_MAP` | `8/8` | `49.0px` | input `4.0px`, select `7.0px`, textarea `0.0px` |
| 768 | `FAIL_ACTION_MAP` | `8/8` | `31.0px` | input `0.0px`, select `0.0px`, textarea `0.0px` |
| 1280 | `FAIL_ACTION_MAP` | `8/8` | `31.0px` | input `0.0px`, select `0.0px`, textarea `0.0px` |

The page-level verdict is still red because later non-strict sections accumulate vertical layout drift. The strict section at the top is the useful reduction.

## Findings

`width: 100%` works for horizontal control sizing. In the full/min0/flex/block cases, Chrome and Saccade rect widths match even though computed `style.width` strings differ.

Auto text input and textarea remain the bad case. At 1280px, Chrome lays `input_fixed_auto` and `textarea_fixed_auto` out at `440px` wide; Saccade keeps them at about `136.5px`.

Explicit control height works for the strict top section. At 768px and 1280px, strict input/select/textarea all have `0.0px` click escape. At 390px they stay within the current 8px action safety threshold.

Textarea default height is a separate compatibility gap. Non-strict textareas are `97px` tall in Chrome and `82px` tall in Saccade. That difference compounds down the page and moves later click points by `31-49px`.

Computed `style.width` and `style.height` are not reliable enough alone for action safety. Saccade often reports content-box-like computed values such as `280px` while the rect width matches Chrome at `302px`. The action map should continue to rely on rects and Chrome hit-tests.

## Decision

For Saccade-owned pages and fixtures, use the page-owned CSS workaround:

```css
input,
select,
textarea {
  width: 100%;
}

textarea {
  height: <explicit px/rem>;
}
```

Also keep `min-width: 0` on grid/flex items that contain controls.

For arbitrary third-party pages, do not inject this globally yet. Record pages with auto textarea/control drift as compatibility failures and route them through Chrome/reference or a future engine fallback when action coordinates become unsafe.

## Local Fixture Follow-Up

The workaround was applied to `test_pages/visual_parity/form_controls/index.html` and retested:

```bash
python3 scripts/visual_parity_width_matrix.py form_controls --widths 390 768 1280 --height 700 --timeout-sec 90 --rendering-profile servo-modern
```

Report:

- `runs/visual_parity_width_matrix/matrix_1781408677983/index.html`

Result:

| Width | Verdict | Chrome Hit-Test | Max Click Escape | Meaning |
| --- | --- | --- | --- | --- |
| 390 | `FAIL_LAYOUT` | `8/8` | `1.0px` | Action-safe, still visually/layout different |
| 768 | `FAIL_LAYOUT` | `10/10` | `5.227px` | Action-safe under current 8px escape threshold |
| 1280 | `FAIL_LAYOUT` | `10/10` | `5.227px` | Action-safe under current 8px escape threshold |

This closes the local `form_controls` P0 action-map issue. It does not close the third-party compatibility issue: pages that rely on browser-default textarea/control sizing can still drift.
