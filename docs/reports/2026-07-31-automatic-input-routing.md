# Automatic input routing report

Date: 2026-07-31

## Result

The normal Agent surface now exposes one action tool, `web.act`. The model does
not choose soft or native input. The Registry selects software for finite click
roles and native OS input for editable, select, and file operations. A
receipt-backed user-local rule may strengthen a page/control to native on its
next fresh token.

Explicit soft/native action tools and the reflex backend selector are absent
from normal MCP discovery and rejected while diagnostics are disabled. Managed
development probes opt in explicitly so both implementations remain testable.

## Managed Chrome and Edge gate

Paired run `20260731T052312Z` passed the complete control and Profile gates in
Chrome and Edge. Each browser produced:

- seven `accepted_by_software + verified` receipts;
- eight `accepted_by_os + verified` receipts;
- a verified receipt-backed software-to-native learning case; and
- rejection of a diagnostic soft override after the learned native rule.

An earlier Chrome attempt, `20260731T052220Z`, stopped at native select with
`visible_state_unchanged`. It is retained as native-select reliability evidence
and is not attributed to the software pointer route.

## Selenium official web form versus Playwright

Normal production MCP discovery reported nine Saccade tools; the two diagnostic
action overrides were absent. Run `20260731T052600Z` passed 3/3 complete form
tasks in both lanes. Across Saccade's 18 verified action receipts, routing was
exactly split:

- nine native receipts: text field, textarea, and select in each iteration;
- nine software receipts: checkbox, radio, and submit button in each iteration.

| Median | Saccade automatic route | Playwright best-case |
| --- | ---: | ---: |
| Task time | 2,546.157 ms | 1,331.028 ms |
| Model-facing tokens | 2,754 | 421 |
| Passed iterations | 3 / 3 | 3 / 3 |

Saccade used 1.913x the task time and 6.542x the model-facing tokens in this
single-shot benchmark. Automatic software routing is therefore proven, but it
does not by itself solve initial Truth Layer token cost or native select
latency/reliability.

The immediately preceding run `20260731T052500Z` stopped at native select after
two verified native editable steps. That failure remains local evidence. Local
artifacts are development evidence and do not make the Catalog publishable.
