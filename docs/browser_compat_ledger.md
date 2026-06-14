# Saccade Browser Compatibility Ledger

Date: 2026-06-13

This ledger records user-visible browser differences that matter for dogfood or agent reliability.

Status values:

- `open`: measured enough to track, not fixed.
- `investigating`: active reduction or source inspection.
- `fixed`: Saccade code/fixture changed and verified.
- `engine-limited`: likely Servo/web-engine gap; use routing/fallback.
- `page-css`: page or fixture CSS is the source; record, do not patch engine.
- `routed`: not fixed, but product has a clear fallback route.

Severity:

- `P0`: unsafe action map, sensitive-policy risk, or unusable primary workflow.
- `P1`: daily dogfood pain or serious visual/layout mismatch.
- `P2`: visual polish or public-demo clarity.

## Entries

| ID | Severity | Status | Area | Symptom | Evidence | Next Step |
| --- | --- | --- | --- | --- | --- | --- |
| BP-001 | P0 | fixed | CSS Grid / responsive | Narrow `form_controls` used to become action-unsafe; controls and footer shifted/clipped differently from Chrome. | `docs/form_control_width_modes_report.md`; after local CSS workaround, `390px` `form_controls` is no longer action-map red: Chrome hit-test `8/8`, max click escape `1.0px`. | Keep as regression; remaining visual/layout drift is tracked under BP-002/BP-009. |
| BP-002 | P1 | investigating | Native controls | Servo form controls often keep intrinsic widths while Chrome expands controls to fill grid columns. | `docs/form_control_width_modes_report.md`: auto input/textarea stay about `136.5px` wide in Saccade while Chrome expands to `302-440px`; `width:100%` makes rect widths match across grid/flex/block. | Use `width:100%` and `min-width:0` for Saccade-owned forms; keep third-party pages routed until measured. |
| BP-003 | P1 | open | Browser shell | No URL bar, Back, Forward, Reload, visible current URL, or loading state. | `docs/blockers.md`; real GitHub/Gist dogfood. | Build shell basics before more daily dogfood. |
| BP-004 | P0 | open | Editors | GitHub/Gist editor looked visible but exposed zero-size/non-actionable body editor to Saccade. | Manual dogfood; `inspect_editors` showed editor candidate with `0x0` rect. | Create local GitHub-like editor reduction, then retest real Gist. |
| BP-005 | P2 | routed | Public visual proof | Servo browser chrome/page rendering does not look identical to Chrome/Safari on `mouseaccuracy.com`. | Demo parity requirements; Chrome/Safari references exist. | Use Chrome/Safari/Firefox references for public visual proof; keep Servo as action/replay evidence. |
| BP-006 | P2 | open | Fonts / sizing | After HiDPI fix, font scale is much better but native controls and button text still look rough. | Manual screenshots of `form_controls`. | Add font metrics fixture and compare computed font/line-height/text rects. |
| BP-007 | P1 | fixed | Viewport / HiDPI / resize | Window resize used to resize the surface without updating JS/layout viewport; Retina scale made text too small. | `960c66d`; `1280x759 -> 1360x759 -> 1000x668` runtime resize verification. | Keep as regression in productization suite. |
| BP-008 | P1 | open | Window bounds / large viewport | Requested large worker widths can be capped by the current macOS window/display session, making 1600/1920 comparisons invalid. | Width matrix requested `1600x700`; Saccade raw screenshot was `2880x1400`, equivalent to `1440x700 @2x`, while Chrome was `1600x700`. | Add a display-boundary/fullscreen probe and clamp/report invalid widths before using large-width benchmarks. |
| BP-009 | P0 | routed | Native textarea / vertical layout | Default textarea height differs from Chrome and causes cumulative vertical click drift in normal flow. | `docs/textarea_default_height_report.md`: default textarea is `54px` in Chrome and `32px` in Saccade at 768/1280; stacked variants produce max click escape `52px`; explicit heights make own rect heights match but cannot undo prior flow drift. | Use explicit textarea sizing for Saccade-owned pages; after resize, re-audit instead of refreshing; route third-party pages when Chrome/live hit-test shows unsafe drift. |

## Entry Template

```text
ID:
Severity:
Status:
Area:
URL/fixture:
Chrome expected:
Saccade observed:
Action-map impact:
Sensitive-policy impact:
Likely source: Saccade adapter | Servo engine | page CSS | unknown
Artifacts:
Next step:
Decision:
```
