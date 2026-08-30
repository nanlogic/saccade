# Current Saccade versus Playwright speed check

Date: 2026-08-29 America/Chicago

## Result

The compact projection materially reduced Saccade's Agent payload without
changing canonical Truth or execution authority. Saccade's browser path is
substantially faster and uses fewer tool calls, but the complete Agent task is
still slower on this short form. The evidence still does not support a blanket
claim that Saccade is faster than Playwright.

Two valid paired runs used opposite lane orders. Both lanes used the same
Claude CLI default model (`claude-opus-4-8`), localhost page, task, success
markers, Chrome family, and monotonic wrapper clock. Only the browser MCP
changed:

- Saccade 0.2.0 with Extension candidate
  `921de39f7cd78f6c6ca744ca50c496577aa7e96f7d3da372c8c9b649235e0445`;
- official `@playwright/mcp@0.0.79`, exact-version locked and locally cached.

The form called `preventDefault()` and made no external request. Both lanes
filled two text controls, selected one option, checked one checkbox, selected
one radio, submitted separately, and proved the local result marker. Every
lane passed in both orders.

## Order-balanced means after compaction

| Metric | Saccade | Playwright | Finding |
| --- | ---: | ---: | --- |
| Agent end-to-end | 38.78 s | 26.20 s | Saccade 48.0% slower |
| Tool calls | 8 | 10 | Saccade 20.0% fewer |
| Browser MCP time | 1.52 s | 6.88 s | Saccade 4.5× faster |
| Execution-tool time | 0.199 s | 4.414 s | Saccade 22.2× faster |
| Initial observation bytes | 3,493 | 1,053 | Saccade 3.3× larger |
| Total tool-response bytes | 8,629 | 5,518 | Saccade 56.4% larger |
| Model logical input tokens | 287,724 | 257,974 | Saccade 11.5% larger |
| Model output tokens | 2,478 | 1,267 | Saccade 95.7% larger |

Saccade's individual end-to-end results were 39.35 and 38.20 seconds. The
Playwright results were 26.78 and 25.61 seconds. Both reports were `PASS` with
no evidence or order errors.

## Improvement from the previous Saccade candidate

| Metric | Before | After | Change |
| --- | ---: | ---: | ---: |
| Agent end-to-end | 41.38 s | 38.78 s | −6.3% |
| Initial observation bytes | 6,383 | 3,493 | −45.3% |
| Batch action receipt bytes | 5,865 | 972 | −83.4% |
| Total tool-response bytes | 16,608 | 8,629 | −48.0% |
| Model logical input tokens | 302,788 | 287,724 | −5.0% |

The implementation now uses document-local short object IDs, a bounded
step-index batch receipt, and a self-describing `compact_rows/1` MCP Truth
projection. The Broker still retains complete canonical Truth. Projected
objects still carry semantic state, document- and viewport-relative geometry,
visibility, limitations, and actionable/continuous capability without sending
raw action tokens to the model.

## Next performance question

The remaining short-task gap is primarily Agent reasoning/output overhead, not
browser execution. The next valid experiment is a current-candidate length
sweep (1/5/25/50 interactions) and a bounded submit-postcondition design that
can remove a redundant marker read without exposing unrelated page churn or
weakening strict object resolution.

The first historical run remains excluded because its prompt described removed
legacy query fields and opened a tab before discovering the exact browser
instance.
