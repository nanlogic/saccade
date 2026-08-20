# Saccade and Playwright: public browser-agent results

Date: 2026-08-20

Saccade lets a local MCP-compatible Agent work through a supported browser task
from the user's instruction to a verified page transition. The Agent does not
need a Saccade-specific model. We have run closed loops with Codex and Claude.

Saccade uses independent code and browser-extension APIs. It contains no
Playwright runtime or Chromium fork. A Chrome or Edge Extension compiles an
authorized page into stable semantic objects. It sends one full view, then
revision-bound deltas, through the Native Host and local Runtime. The MCP
adapter gives the Agent a small working set and exposes bounded actions for
supported objects.

```text
authorized Chrome or Edge tab
  -> Extension semantic compiler
  -> Native Messaging Host
  -> owner-only local IPC
  -> Runtime and MCP
  -> Agent
```

We adopted two useful ideas from Playwright: revalidate the target before an
action, and keep retries local and bounded. Saccade applies those ideas to
stable semantic objects instead of selectors, screenshots, or coordinates.

## Test method

The ordinary task matrix used the same Codex model and low reasoning effort in
both lanes. Every task ran twice, once with Saccade first and once with
Playwright first. Both lanes received the same URL, natural-language task, and
machine-checked completion conditions.

- Saccade candidate: `0.3.23 / 2d8a877e3dc1b5c9a003aa3662ea9ddad506a7033aba286e1c48e21fe8af2612`
- Saccade Runtime contract after the final fix: `ceec0b059a7215aa94669f77d94e846b54baa7bab9cf9eb481844bd263c01c20`
- Playwright: official `@playwright/mcp@0.0.79`
- Browser: Chrome in both lanes
- Orders: Saccade then Playwright, and Playwright then Saccade
- Infrastructure, evidence, and control-plane errors in the final runs: zero

Neither Agent received selectors, XPath, DOM queries, JavaScript evaluation,
coordinates, screenshots, source code, search results, or human help.

We have prepared Extension `0.3.24` as the production-named package. That
change replaces the development name and advances the candidate identity; it
does not change the tested action or observation code. We still need to rerun
the exact `0.3.24` package before treating these numbers as release evidence
for that candidate.

## Ordinary tasks

Both products completed their lane in all 16 final paired reports.

| Task, mean of both orders | Saccade | Playwright | Completion |
| --- | ---: | ---: | ---: |
| DemoQA React form | 32.64 s, 6 calls | 36.79 s, 6 calls | 2/2 each |
| Angular Material select | 35.40 s, 6 calls | 48.35 s, 10.5 calls | 2/2 each |
| Best Buy homepage | 23.72 s, 4 calls | 38.36 s, 4 calls | 2/2 each |
| GitHub `openai-python` | 20.53 s, 4 calls | 29.90 s, 6 calls | 2/2 each |
| IGN homepage | 19.35 s, 4 calls | 43.70 s, 7.5 calls | 2/2 each |
| Mythcast Era homepage | 19.74 s, 4 calls | 22.54 s, 3.5 calls | 2/2 each |
| Nanlogic homepage | 22.83 s, 4 calls | 23.38 s, 3.5 calls | 2/2 each |
| NanMesh homepage | 23.06 s, 4 calls | 19.58 s, 3 calls | 2/2 each |
| **All 16 paired runs** | **24.66 s, 4.5 calls** | **32.82 s, 5.5 calls** | **16/16 each** |

In this sample, Saccade used 24.9% less end-to-end time and 18.2% fewer browser
calls. Playwright finished the NanMesh task faster and used fewer calls on
three homepage tasks. The result supports a measured advantage for this
matrix, not a general speed claim.

## Transfer and model usage

| Mean per lane | Saccade | Playwright | Difference |
| --- | ---: | ---: | ---: |
| Initial browser transfer | 11.91 KB | 11.33 KB | Saccade +5.1% |
| Complete browser transcript | 25.75 KB | 14.84 KB | Saccade +73.5% |
| Total input tokens | 98,195 | 105,435 | Saccade -6.9% |
| Non-cached input tokens | 32,931 | 28,299 | Saccade +16.4% |

Saccade did not win the payload comparison. Its full-to-delta protocol reduced
re-observation calls, but action receipts and the capabilities contract made
the recorded browser transcript larger. Total input tokens fell while
non-cached input rose. We report those numbers separately because browser bytes
and model tokens measure different costs.

## A continuously moving target

We also tested the public `saccade.act` route on MouseAccuracy with `Insane`
difficulty, `Tiny` targets, and a 30-second round. We reversed the lane order.
No model drove this test, so it measures deterministic execution rather than
planning or model-token use.

| Order | Saccade public `saccade.act` | Playwright locator |
| --- | ---: | ---: |
| Saccade first | 88 verified actions, 0 failures, 0 stale | 0 hits, 30 locator timeouts |
| Playwright first | 88 verified actions, 0 failures, 0 stale | 0 hits, 30 locator timeouts |

Saccade averaged 24.779 ms per verified action receipt. Every receipt required
an advance in the target's semantic occurrence counter; a revision change by
itself did not pass.

The Playwright lane received a favorable test-only page loop and the selector
`.target:not(.hit):visible`. Its normal locator click waited for actionability
and timed out as the target kept moving. This result shows an advantage for
Saccade's exact-object `reflex_target` path on this target class. It does not
show that Saccade beats Playwright on ordinary controls.

## Homepage video semantics

The Mythcast Era homepage includes a hero video with author-provided accessible
metadata. Both products found the page statement `Hero video made with Veo.`
Saccade also returned the accessible description beginning `Two worlds in deep
space` in both lane orders. Playwright's semantic snapshot did not return that
description in either order.

| Required video-semantic evidence | Saccade | Playwright |
| --- | ---: | ---: |
| Two reversed-order runs | 2/2 | 0/2 |

Saccade marked the media object `opaque_video`. It read metadata supplied by
the page; it did not inspect decoded frames or infer visual content. Neither
lane used screenshots or pixels.

## Limits

These tests used one machine, one date, one model configuration, and Chrome.
Public sites change. Some pages may appear in model training data. The matrix
does not cover every website, protected login data, arbitrary Canvas/WebGL,
restricted cross-origin frames, or Windows.

Saccade exposes editable fields without exposing their current values. It
keeps passwords, SSNs, and EINs behind the protected-value boundary. Unsupported
actions return an explicit handoff instead of falling back to Playwright, CDP,
screenshots, or arbitrary coordinates.

The MouseAccuracy and video results cover narrow capabilities. The ordinary
matrix gives the broader comparison: both products completed every final run,
Saccade finished faster with fewer calls in aggregate, and Playwright produced
the smaller browser transcript.

## Try the source build

Saccade remains a developer preview. The Chrome Web Store package and
`@saccade/setup` release are not public yet. Developers can clone this
repository and follow the managed setup commands in the main README. Please
open a GitHub issue with the URL, task, browser version, and the first truthful
failure you observe. Do not include passwords, protected identifiers, cookies,
or browser storage in a report.
