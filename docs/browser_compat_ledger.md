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
| BP-001 | P1 | open | CSS Grid / responsive | `form_controls` narrow window keeps two columns and right-side controls clip/overflow. | Manual screenshots after HiDPI fix; fixture CSS has unconditional `grid-template-columns: 1fr 1fr`. | Add focused grid percentage/min-auto fixtures; classify page CSS versus Servo sizing. |
| BP-002 | P1 | open | Native controls | Servo control rects can differ widely from Chrome while hit-tests still pass. | Visual parity `form_controls` yellow; previous report noted large native control rect delta. | Split metrics by input/select/date/number/button/checkbox/radio/textarea. |
| BP-003 | P1 | open | Browser shell | No URL bar, Back, Forward, Reload, visible current URL, or loading state. | `docs/blockers.md`; real GitHub/Gist dogfood. | Build shell basics before more daily dogfood. |
| BP-004 | P0 | open | Editors | GitHub/Gist editor looked visible but exposed zero-size/non-actionable body editor to Saccade. | Manual dogfood; `inspect_editors` showed editor candidate with `0x0` rect. | Create local GitHub-like editor reduction, then retest real Gist. |
| BP-005 | P2 | routed | Public visual proof | Servo browser chrome/page rendering does not look identical to Chrome/Safari on `mouseaccuracy.com`. | Demo parity requirements; Chrome/Safari references exist. | Use Chrome/Safari/Firefox references for public visual proof; keep Servo as action/replay evidence. |
| BP-006 | P2 | open | Fonts / sizing | After HiDPI fix, font scale is much better but native controls and button text still look rough. | Manual screenshots of `form_controls`. | Add font metrics fixture and compare computed font/line-height/text rects. |
| BP-007 | P1 | fixed | Viewport / HiDPI / resize | Window resize used to resize the surface without updating JS/layout viewport; Retina scale made text too small. | `960c66d`; `1280x759 -> 1360x759 -> 1000x668` runtime resize verification. | Keep as regression in productization suite. |

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
