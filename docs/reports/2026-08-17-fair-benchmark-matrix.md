# Fair comparison matrix: 3 tasks × 2 orders

Date: 2026-08-17. Candidate `0.3.22`
(`c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`).
Official baseline `@playwright/mcp@0.0.79` from
`benchmarks/playwright-mcp.lock.json`.

**Verdict: all six runs are `INVALID`. No speed, token, or payload claim in
either direction is authorized by this matrix.**

## Lanes

- Saccade lane: Saccade Truth for all observation, Claude Code's own Chrome tool
  (`form_input`, `computer.left_click`) for all execution, same ordinary-Chrome
  tab. Saccade `tab_id` equals the Claude Chrome `tabId` in every run;
  `browser_instance_id` is `browser.7898d0e6…` throughout.
- Playwright lane: official `@playwright/mcp@0.0.79` only, driven by
  `codex exec`, headless isolated Chrome.
- Both lanes received the same URL and the same natural-language goal, with no
  selectors, scripts, or state passed between them.

## Results

| Run | Lane | Completed | ms | Tool calls | Initial bytes | Re-observations | Input tokens |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| selenium saccade-first | saccade | yes | 107556 | 8 | 8403 | 3 | unmeasured |
| selenium saccade-first | playwright | yes | 34175 | 6 | 2763 | 2 | 112746 |
| selenium playwright-first | saccade | yes | 22736 | 6 | 8403 | 2 | unmeasured |
| selenium playwright-first | playwright | yes | 29755 | 6 | 2773 | 2 | 112657 |
| angular saccade-first | saccade | yes | 67982 | 8 | 74641 | 2 | unmeasured |
| angular saccade-first | playwright | yes | 48246 | 8 | 969 | 0 | 143223 |
| angular playwright-first | saccade | yes | 30200 | 8 | 74641 | 2 | unmeasured |
| angular playwright-first | playwright | yes | 43248 | 10 | 1369 | 2 | 174290 |
| demoqa saccade-first | saccade | yes | 58691 | 10 | 12320 | 4 | unmeasured |
| demoqa saccade-first | playwright | yes | 34617 | 5 | 3753 | 1 | 98134 |
| demoqa playwright-first | saccade | yes | 29381 | 7 | 12320 | 2 | unmeasured |
| demoqa playwright-first | playwright | yes | 42449 | 6 | 24766 | 0 | 178832 |

Both lanes completed 6/6 tasks with browser-proven evidence
(`Received!`, `Pizza`, `Thanks for submitting the form`).

## Why every run is INVALID

`evidence_errors` is identical in all six runs:

```json
{"saccade": ["model_input_tokens_missing", "delta_latency_missing"], "playwright": []}
```

The Playwright lane produced complete evidence. The Saccade lane is missing two
required fields, and both are missing for the same structural reason: the lane
was executed by an interactive Claude session rather than a single-process
agent.

1. `usage.input_tokens` — an interactive Claude session does not expose per-lane
   token usage. `claude -p --output-format stream-json` reports it, but the
   local `claude` CLI exits `Not logged in`.
2. `browser_metrics.action_return_to_delta_read_ms` — the action and the delta
   read happen in different conversation turns, so the measured interval is
   dominated by model turnaround. Recording it would have overstated Saccade's
   delta latency by orders of magnitude, so it was left null. This is the same
   condition the harness already names
   `requires_timed_same_tab_executor_events`.

For the same reason the Saccade `ms` column is **not comparable** to the
Playwright one: it is interactive wall-clock, not agent execution time. The
spread within one identical task (107556 ms vs 22736 ms for selenium, differing
only in how many actions were batched per turn) shows the number tracks
conversation shape, not the Truth Layer.

## What the valid columns do show

Initial payload is the one metric measured comparably on both sides, and it does
**not** favor Saccade:

| Task | Saccade initial Truth | Playwright initial snapshot |
| --- | ---: | ---: |
| selenium | 8403 | 2763 / 2773 |
| angular | 74641 | 969 / 1369 |
| demoqa | 12320 | 3753 / 24766 |

Mean initial bytes: Saccade 31788, Playwright 6065. On the Angular Material docs
page Saccade's first view is 55–77× larger, because Saccade compiles the whole
document — 240 objects including full site navigation, each with geometry —
while Playwright's snapshot is a narrower filtered tree. Playwright was larger in
exactly one run (demoqa playwright-first, 24766 bytes).

Tool calls are close (Saccade mean 7.8, Playwright mean 6.8). Re-observation
counts are low and similar in both lanes.

No initial-payload or token-cost advantage for Saccade is demonstrated here. Any
"low model-token cost" claim needs the Saccade lane's token measurement, which
this matrix does not have.

## Qualitative observations

- **Angular Material overlay.** Saccade observed the dynamically inserted select
  overlay: 226 → 242 objects, the select moving to `expanded: true`, and
  `Steak` / `Pizza` / `Tacos` appearing. After selection the overlay
  disappeared, the options were withdrawn, and the committed value showed as
  `has_value: true` with `description: "Favorite foodPizza"`.
- **demoqa is semantically weak.** Its `First Name`, `Last Name`, `Email`,
  `Mobile`, and `Current Address` controls carry no accessible name, so Truth
  reported `name: null` for all five. The lane had to identify them by document
  order and `required` state. This is a genuine Truth limitation on that page,
  not a harness artifact.
- **Protected boundary held.** The selenium page's `Password` field was reported
  but never filled, and no editable value ever appeared in Truth — only
  `has_value`. A grep of all six Saccade lane evidence files for the five task
  sentinel strings returns zero hits.
- Every Agent-owned tab was closed at lane end; `tabs.list` returned empty each
  time.
- `dynamic_replacement_recoveries` was 0 and `stale_events` 0 in all six runs.

## Harness defects found and fixed

- `@playwright/mcp` 0.0.79 removed `--output-mode`, which the harness passed as
  `--output-mode stdout`. The server refused to start, the Playwright lane made
  zero tool calls, and the run looked like a Playwright failure. Because
  `stdout` was already 0.0.78's default, the flag was simply dropped. This was
  fallout from moving the lock from 0.0.78 to 0.0.79.
- A lane that never reached its browser MCP now reports
  `browser_mcp_unavailable_no_tool_calls` instead of an unexplained zero-tool
  result, so broken plumbing can never be credited to the other lane.

Both defects would have biased results toward Saccade if left unfixed.

## Known fairness limitation

The two lanes do not share a model. The Saccade lane is Claude Code; the
Playwright lane is `codex exec`, because the harness builds both lanes from
`common_codex_command` and only Codex can host the Playwright MCP here. Model
and browser route are therefore confounded. Even once the two missing Saccade
fields are measurable, a defensible comparison needs both lanes on the same
model — for example by driving the Playwright lane from `claude -p` with
`--mcp-config` instead of `codex exec`.

## Unblocking a valid matrix

One external step: log the local `claude` CLI in. That single change supplies
the token accounting and lets the Saccade lane run inside one process, which
also makes `action_return_to_delta_read_ms` measurable. Running both lanes from
that same CLI would remove the model confound at the same time.

Evidence:
`~/Library/Application Support/Saccade Dev/evidence/20260817-fair-benchmark/`
