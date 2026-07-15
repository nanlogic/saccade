# CEF Day 4 Forms, Collaboration, Screenshot, and Replay Report

Date: 2026-07-15
Verdict: PASS for product contract sections 2, 3, 4, and 6 on the bounded
local gates below.

## Migrated surface

The owner-only CEF adapter now exposes fixed commands for form inventory,
non-sensitive inspection, plan compilation, verified execution, lazy-form
reveal, screenshot policy, and screenshot audit. The renderer accepts only
these commands; it does not expose a general JavaScript evaluator to the host
or page.

Form plans are bound to the current page revision and a deterministic plan ID.
They preserve existing values, reject human-owned or sensitive controls, and
return field IDs plus status rather than submitted values. Inspection returns
non-sensitive values so a co-pilot can review user work, while protected fields
return only completion state.

Replay is written as owner-only JSONL. It records command names, revisions,
counts, and result status with `values_logged=false`; assignments, inspected
values, cookies, storage, and capabilities are omitted.

## Evidence

`scripts/probe_cef_form_safety.py` passed on the signed Release app:

- 17 controls inventoried;
- 6 ordinary fields filled and verified, including dropdown, checkbox, and
  contenteditable;
- 4 unsafe overwrite attempts rejected;
- 3 existing/user values preserved;
- non-sensitive human text remained inspectable;
- password and SSN values remained redacted;
- sensitive-page screenshot rejected before capture;
- non-sensitive dashboard screenshot saved as a 2560x1518, 104844-byte PNG;
- 8 replay events passed the sensitive sentinel scan.

Final signed-build report: `runs/cef_day4_form_safety_final/report.json`.

`scripts/probe_cef_formmax.py` passed the long-table gate:

- 96 rows across 2 pages;
- 672 unique fields filled and verified in 6 lazy batches;
- 2 visible page-transition actions returned receipts;
- 3 final-page sensitive fields remained blocked;
- 28 replay events remained value-free.

Final signed-build report: `runs/cef_formmax_final/report.json`.

Core regressions also passed:

- local reflex: 3 runs of 100/100, 0 misses, with 3.1, 3.3, and 3.2 ms
  p95;
- original MouseAccuracy: 12/12 live targets, 8.4 ms p95, final collector
  ready on `/game`;
- neither reflex route used screenshots or CDP.

Reports: `runs/cef_truth_reflex/day4_final/aggregate.json` and
`runs/cef_day4_mouseaccuracy/report.json`.

## Screenshot boundary

Screenshots remain off the normal truth path. `screenshot_audit` first runs the
renderer policy gate. If any protected field is present, no pixel capture is
started. A permitted capture uses CEF's in-process `Page.captureScreenshot`
method, writes an owner-only PNG, returns only artifact metadata, and records a
value-free replay event. Remote debugging remains disabled.

The screenshot backend requires a visible rendered surface; hidden-window
automation is not a screenshot product path. Truth, forms, actions, and replay
continue to work without screenshots.

## Honest limits

This closes the four requested CEF migration pillars, not every Day 4 hostile
web case. Cross-origin frame enumeration, richer custom controls, PDF forms,
and broad public-site workflow coverage remain separate measured gates. The
adapter deliberately omits inaccessible frame values instead of weakening the
redaction boundary.
