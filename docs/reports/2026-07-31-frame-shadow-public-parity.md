# Frame, shadow, and public parity report

Date: 2026-07-31

## Result

The existing root Truth Layer route is preserved. Same-origin iframe controls
and open-shadow controls now enter the same observation and complete verified
native-input loops. Inaccessible iframe contents and closed-shadow contents do
not enter the Agent view.

## Managed browser gates

| Gate | Chrome | Edge |
| --- | --- | --- |
| Frame/shadow fixture | PASS (`20260731T051006Z`) | PASS (`20260731T051006Z`) |
| Observed/restricted frames | 2 / 1 | 2 / 1 |
| Frame button | `accepted_by_os + verified` | `accepted_by_os + verified` |
| Open-shadow button | `accepted_by_os + verified` | `accepted_by_os + verified` |
| Common controls + Profile | PASS (`20260731T050149Z`) | PASS on rerun (`20260731T050252Z`) |

The first paired Edge regression had one native select
`visible_state_unchanged`; the complete Edge rerun passed. This is retained as
an input-reliability observation, not counted as a frame failure.

## Official public pages versus Playwright

W3C WAI-ARIA run `20260731T050337Z` used Saccade's Extension → Native Host →
Runtime → MCP → registry-selected input route. Playwright ran separately as a
reference oracle. Radio, switch, tab, and menu item matched in Chrome and Edge.

The Selenium official `web-form.html` matched benchmark passed 3/3 in Chrome
for both lanes:

| Metric (median) | Saccade | Playwright best-case |
| --- | ---: | ---: |
| Task time | 2,485.890 ms | 1,368.532 ms |
| Model-facing tokens | 2,776 | 421 |
| Passed iterations | 3 / 3 | 3 / 3 |

For this single-shot form task Saccade used 1.816x the time and 6.594x the
model-facing tokens. The result validates correctness, not a speed or token
advantage. Saccade's distinct claim remains the persistent, browser-pushed
Truth Layer and verified locator-free execution; improving initial-view
compaction remains open work.

Local evidence root: `20260731T050337Z`. These artifacts are development
evidence, not publishable release evidence.
