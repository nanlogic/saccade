# Textarea Default Height Report

Date: 2026-06-14

Command:

```bash
python3 scripts/visual_parity_width_matrix.py textarea_default_height --widths 390 768 1280 --height 700 --timeout-sec 90 --rendering-profile servo-modern
```

Report:

- `runs/visual_parity_width_matrix/matrix_1781436098475/index.html`
- `runs/visual_parity_width_matrix/matrix_1781436098475/width_matrix_manifest.json`
- Re-run with flow-drift classifier: `runs/visual_parity_width_matrix/matrix_1781436300919/index.html`

## Result

| Width | Verdict | Chrome Hit-Test | Max Click Escape | Main Cause |
| --- | --- | --- | --- | --- |
| 390 | `FAIL_ACTION_MAP` | `1/5` | `97.0px` | cumulative vertical drift |
| 768 | `FAIL_ACTION_MAP` | `1/6` | `52.0px` | cumulative vertical drift |
| 1280 | `FAIL_ACTION_MAP` | `1/6` | `52.0px` | cumulative vertical drift |

This fixture intentionally stacks textarea variants. The page-level red verdict is expected because early default-height differences move every later action target.

The follow-up run with the classifier now reports this directly as `Possible cumulative flow drift`:

- 390px: `max_top_delta=145px`, `max_size_delta=32px`
- 768px: `max_top_delta=94px`, `max_size_delta=26px`
- 1280px: `max_top_delta=94px`, `max_size_delta=26px`

## Height Findings

At 768px and 1280px:

| Variant | Chrome Rect Height | Saccade Rect Height | Immediate Impact |
| --- | --- | --- | --- |
| default textarea | `54px` | `32px` | First control stays clickable, but introduces `22px` flow drift |
| `rows="2"` | `54px` | `32px` | Same height delta; later center starts escaping |
| `rows="3"` | `72px` | `48px` | `24px` height delta |
| `min-height:82px` | `82px` | `82px` | Own height matches; previous drift remains |
| `height:82px` | `82px` | `82px` | Own height matches; previous drift remains |
| `height:97px` | `97px` | `97px` | Own height matches; previous drift remains |
| `line-height:20px` | `58px` | `32px` | line-height alone does not normalize |
| `box-sizing:content-box;height:82px` | `100px` | `100px` | own height matches; previous drift remains |

Computed `style.height` is not the same as rect height for Saccade because it reports content-box-like values. The rect is the action source of truth.

## Browser Behavior Clarification

Mainstream browsers do not refresh the page on window resize. They update viewport dimensions, recalculate style and media queries, perform layout/reflow, repaint, and notify page JS through resize-related APIs when relevant. Form values stay in the DOM.

Textarea default size is not a stable cross-engine pixel contract. It is influenced by UA stylesheet defaults, default `rows`/`cols`, font metrics, line-height, padding, border, native control theme, and box model. Chrome/Safari/Firefox have strong web-compat pressure to look similar, but Servo can legitimately differ unless we add compatibility styling or route.

## Decision

For Saccade-owned pages:

- Do not rely on default textarea height.
- Set `width:100%` and explicit `height` or a measured `min-height`.
- Keep `min-width:0` on the grid/flex item chain.

For third-party pages:

- Re-audit after resize instead of refreshing.
- Trust action only when Saccade click points pass Chrome hit-test or equivalent live verification.
- Route pages with unsafe textarea/control drift to Chrome-reference or future Chrome-live.

## Next Step

Create an action-map classifier bucket for cumulative vertical drift. Today the verdict is correctly red, but the report should say "flow drift from default control sizing" instead of leaving it buried in raw rect deltas.
