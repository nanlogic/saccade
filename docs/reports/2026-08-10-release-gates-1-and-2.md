# Release gates 1 and 2: first evidence run

Date: 2026-08-10 America/Chicago.

## Candidate identity

The clean-profile gate used one working-tree snapshot for Chrome and Edge.

| Field | Value |
| --- | --- |
| Base commit | `20c170058c0c563432baad21f5489ded7c5c497b` |
| Working tree | dirty, preserved without commit |
| Working-tree SHA-256 | `17831d7484e076c62960410d58f243635b8aaf0402b98e02b58752cd7a2bf64e` |
| Runtime | `0.1.0` |
| Extension | `0.3.19` |
| Chrome | `Google Chrome for Testing 151.0.7922.47` |
| Edge | `Microsoft Edge 151.0.4129.72` |

Candidate manifest:
`~/Library/Application Support/Saccade Dev/evidence/20260811T010454Z/candidate.json`.

This record identifies the tested local snapshot. It is not a frozen release
commit because the working tree contains uncommitted user work.

## Gate 2: same-candidate Chrome and Edge

Result: passed in both browsers.

Both clean profiles passed pushed delta, Resource subscription, all 15 control
families, 27 semantic targets, same-origin iframe, restricted iframe, open
Shadow DOM, and closed Shadow DOM boundaries.

| Measurement | Chrome | Edge |
| --- | ---: | ---: |
| Initial full | 36.166 ms | 36.566 ms |
| Single-object p95 | 20.996 ms | 21.134 ms |
| 10-object p95 | 20.758 ms | 20.790 ms |
| 100-object p95 | 26.098 ms | 26.027 ms |
| Missing markers | 0 | 0 |
| Duplicate markers | 0 | 0 |
| Empty deltas | 0 | 0 |

Evidence root:
`~/Library/Application Support/Saccade Dev/evidence/20260811T010454Z`.

The run exposed two fixture defects before passing. Public geometry turned
top-of-page marker insertion into a large layout change, so Runtime returned a
full view as designed. The fixture now keeps single-object markers out of
document flow. The iframe probe also waits for the declared frame coverage
instead of reading once during load.

## Public observation diagnostic (not Gate 1)

Result: useful collector evidence, but invalid as an end-to-end release gate.

The default four-tool MCP opened and read 12 cases across five official source
families and five implementation types. Reference Actuator supplied a
test-only stimulus in a separate process. The evidence keeps the default Truth
view and observed transition while excluding action authority, receipts, and
editable values.

Seven cases passed in Chrome and Edge:

- Selenium native select, checkbox, and radio;
- W3C APG radio, switch, tab, and menu item.

The remaining cases expose diagnostic-harness or collection findings. Native
input permission is required only by the separate Reference Actuator stimulus;
it is not required by Saccade Runtime and is not a product blocker.

| Case | Chrome | Edge |
| --- | --- | --- |
| Selenium text field | diagnostic stimulus unavailable | diagnostic stimulus unavailable |
| Selenium textarea | diagnostic stimulus unavailable | diagnostic stimulus unavailable |
| Angular Material select | failed during initial collection | blocked after open transition |
| PrimeVue select | failed to retain the declared hydrated target | blocked when soft stimulus did not open the control |
| Shoelace tab | blocked during prepared-action revalidation | blocked during prepared-action revalidation |

Chrome evidence:
`~/Library/Application Support/Saccade Dev/evidence/20260811T010628Z/chrome/public-truth/saccade.json`.

Edge evidence:
`~/Library/Application Support/Saccade Dev/evidence/20260811T010812Z/edge/public-truth/saccade.json`.

These outcomes do not establish five-source public compatibility. They prove
seven default-Truth transitions under synthetic test stimulus. They must not be
combined with Agent-owned execution evidence or described as Gate 1. The real
Gate 1 run needs Codex or another Agent's own same-tab browser tool; it does not
need Saccade Runtime input authority or macOS Accessibility permission.
