# Modern React zero-knowledge Agent comparison

Date: 2026-08-01  
Status: local development evidence; not publication evidence

## Task and fairness boundary

The same `gpt-5.6-terra` Agent started without page knowledge and completed the
public DemoQA React student-registration form. Each lane received only the URL
and natural-language task in
`benchmarks/tasks/demoqa_react_practice_form.json`. Navigation, discovery,
planning, failed calls, actions, verification, time, and model usage all counted.
Neither lane received selectors, DOM queries, JavaScript, coordinates,
screenshots, or site-specific execution logic.

Saccade used the production Extension → Native Host → Runtime → MCP route.
Playwright was an isolated out-of-band comparison lane and did not create or
upgrade a Saccade receipt.

## Result

Two order-reversed post-fix runs passed in both lanes:

| Order / evidence | Lane | Passed | Elapsed | Tool calls | Input tokens |
| --- | --- | ---: | ---: | ---: | ---: |
| Playwright first, `20260801T0817Z/fair-agent-demoqa-react-final-source` | Saccade | yes | 30.945 s | 6 | 118,243 |
| same | Playwright | yes | 30.204 s | 7 | 113,393 |
| Saccade first, `20260801T0820Z/fair-agent-demoqa-react-final-source-reverse` | Saccade | yes | 26.318 s | 6 | 122,554 |
| same | Playwright | yes | 31.212 s | 5 | 100,373 |
| **Two-run mean** | **Saccade** | **2/2** | **28.631 s** | **6.0** | **120,399** |
| **Two-run mean** | **Playwright** | **2/2** | **30.708 s** | **6.0** | **106,883** |

In this task and these two runs, Saccade averaged 6.8% less elapsed time, the
same number of tool calls, and 12.6% more input tokens. This is a bounded result,
not a universal speed or token-superiority claim.

The final Saccade path used one bounded form plan for seven controls and a
separate Submit action. Editable values remained absent from receipts and were
redacted from Agent benchmark artifacts. The final confirmation title
`Thanks for submitting the form` appeared in Saccade Truth Layer evidence, and
the deferred Submit button received a verified semantic-effect receipt.

## Defects found and fixed

The first external run successfully opened the confirmation modal but failed
the strict evidence check because the Truth Layer omitted its title and the
button receipt remained unverified. Investigation retained every failed run:

- `20260801T0748Z/fair-agent-demoqa-react`
- `20260801T0753Z/fair-agent-demoqa-react-dialog`
- `20260801T0758Z/fair-agent-demoqa-react-final`
- `20260801T0801Z/fair-agent-demoqa-react-final2`

The root cause was framework lifecycle, not React classification. React-Bootstrap
inserted a correctly labelled dialog while its fade transition still computed
`opacity=0`. Saccade correctly withheld hidden content, but did not observe the
later pure-CSS visibility transition. The fix:

- projects a visible dialog's bounded page-authored title as a heading without
  adding a new v1 role or exporting its subtree;
- listens for `transitionend` and `animationend` and pushes a fresh observation;
- declares form-submit/dialog-reveal buttons as `deferred_content_possible`;
- gives that verifier a bounded 750 ms settlement window;
- verifies only a newly visible heading, alert, or status—not arbitrary object
  churn or table rows.

Both final Saccade runs retained one stale Submit-token rejection caused by
ongoing third-party page mutation, then observed and completed with a fresh
token. Those failures are expected fail-closed behavior and remain counted. An
earlier run had one harmless read call with `timeout_ms` but no
`after_revision`; the final adapter normalizes that to an immediate current-view
read.

## Regression gates

- Workspace Rust tests: passed, including owner-only IPC and 12 closed-loop tests.
- Rust clippy with warnings denied: passed.
- Extension Node tests: 16/16 passed.
- Single-architecture and generated Catalog gates: passed.
- Same-candidate managed Chrome and Edge controls/Profile/dialog/stale run:
  `20260801T081531Z` passed in both browsers.

Catalog rows remain `implementation`. These local runs do not satisfy signed,
clean-machine, store-Extension, or publishable release evidence.
