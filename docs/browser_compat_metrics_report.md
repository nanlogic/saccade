# Browser Compatibility Metrics Report

Date: 2026-06-16

Command:

```bash
python3 scripts/browser_compat_metrics.py font_control_metrics --widths 1280 1600 --height 760 --timeout-sec 160 --rendering-profile servo-modern
```

Report:

- `runs/browser_compat_metrics/metrics_1781650486498/index.html`
- `runs/browser_compat_metrics/metrics_1781650486498/browser_compat_metrics.json`

## What This Adds

AI-006 and AI-007 are now covered by a repeatable local gate:

- `test_pages/visual_parity/font_control_metrics/` adds a focused fixture for
  system text, explicit line-height, mono text, wrapping text, input, date,
  select, textarea, and buttons.
- Chrome and Saccade probes now emit extra per-probe metrics:
  `textMetrics`, `fontFamily`, `fontWeight`, `letterSpacing`, plus client and
  scroll dimensions. Form control values are not emitted as text samples.
- `scripts/browser_compat_metrics.py` runs the existing visual parity capture,
  reads Chrome/Saccade truth, checks requested CSS viewport versus actual
  Saccade CSS/runtime geometry, and classifies rows as `GREEN`, `YELLOW`,
  `RED`, or `INVALID_VIEWPORT`.

## Result

| Width | Height | Verdict | Main reason |
| --- | --- | --- | --- |
| 1280 | 760 | `RED` | Viewport is valid, control rect delta is `0px`, but Servo text range rects differ from Chrome by up to `1130.203px`. |
| 1600 | 760 | `INVALID_VIEWPORT` | Requested `1600x760`, but Saccade actual CSS viewport/runtime logical context is `1440x760`. |

The control-rect result is encouraging for explicitly sized Saccade-owned forms:
on the new focused fixture, inputs, select, textarea, and buttons line up with
Chrome at `1280x760`.

The text-range result is not yet usable as a product truth source. Element rects
and client/scroll sizes are available, but `Range.getBoundingClientRect()` style
text rects can be wildly different in Servo on this fixture. Treat that as a
measured browser-compat issue, not as a form-action blocker.

The large-width result closes the old ambiguity: the gate now refuses invalid
large viewport comparisons before visual diff/action claims are made.

## Next

- Keep using explicit control sizing for owned forms.
- Use `browser_compat_metrics.py` before trusting 1600/1920 desktop visual
  comparisons on macOS.
- Investigate Servo text range rect behavior only if text-level layout truth
  becomes necessary for product claims; element/action rects remain the primary
  action map source.
