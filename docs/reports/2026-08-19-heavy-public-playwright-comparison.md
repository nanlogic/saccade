# Heavy public-site Saccade / Playwright comparison

Date: 2026-08-19

## Scope

This is a read-only product-route comparison on six current public sites:
IGN, Best Buy, the public `openai/openai-python` GitHub repository, NanMesh,
Nanlogic, and Mythcast Era. Each task ran twice, once in each lane order, with
the Codex default recommended model at low effort. Both lanes received the same
URL, natural-language goal, and machine-checked success strings.

The Saccade lane used the installed Runtime with canonical Extension Truth and
bounded semantic working sets. The Playwright lane used the exact official
`@playwright/mcp@0.0.79` lock with its isolated Chrome. Neither lane could use
search, source inspection, selectors, XPath, DOM queries, JavaScript,
coordinates, screenshots, or human assistance. GitHub intentionally used a
public repository instead of the signed-in homepage so browser state was not a
confounder.

Evidence root:
`~/Library/Application Support/Saccade Dev/evidence/20260819-heavy-public-codex-optimized-v2/`

## Validity

All 12 paired reports are `PASS`; therefore all 24 lanes produced browser-tool
evidence for their success condition. There were no timeouts, 529 responses,
zero-tool-call runs, or site challenge pages in the frozen matrix.

An earlier Mythcast Era task incorrectly described `What is Mythcast Era?` as
a navigation choice even though it is a section heading. Saccade correctly
reported that no such link/button existed while Playwright's whole-page
snapshot happened to include the heading. That failed report was retained at
`20260819-heavy-public-codex/mythcastera-saccade-first/`; the task wording was
corrected before both frozen orders were run.

## Results

Values are the mean of the two lane orders for each site.

| Site | Saccade time | Playwright time | Saccade initial bytes | Playwright initial bytes | Saccade calls | Playwright calls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| IGN | 37.7 s | 48.6 s | 14.1 KB | 14.5 KB | 4.5 | 6.5 |
| Best Buy | 29.6 s | 35.6 s | 4.2 KB | 2.2 KB | 4.0 | 4.0 |
| GitHub | 30.4 s | 26.9 s | 4.5 KB | 52.8 KB | 5.0 | 5.0 |
| NanMesh | 23.4 s | 22.8 s | 1.5 KB | 10.3 KB | 4.0 | 3.0 |
| Nanlogic | 27.5 s | 26.4 s | 2.2 KB | 8.2 KB | 4.0 | 3.0 |
| Mythcastera | 27.9 s | 22.3 s | 2.8 KB | 2.4 KB | 4.0 | 4.0 |
| **Overall** | **29.4 s** | **30.4 s** | **4.9 KB** | **15.1 KB** | **4.2** | **4.2** |

Within this matrix, Saccade transferred 67.5% fewer initial browser-result
bytes on average. It won initial bytes in 7 of 12 paired runs. End-to-end time
was effectively mixed: each lane won 6 runs, and Saccade's mean was 3.3% lower.
Tool calls were equal on mean; Saccade won 3 runs, Playwright won 5, and 4 tied.
These data support a payload-efficiency result for this sample, not a general
claim that Saccade is always faster.

Agent-reported token usage did not follow browser payload size. Saccade averaged
143,179 input and 645 output tokens, versus Playwright's 84,214 input and 445
output tokens. After subtracting cached input, Saccade was still 30.5% higher.
The likely contributors visible in the traces are Saccade's required
capabilities call, longer execution contract, and occasional extra semantic
query. Browser-result bytes and model-context tokens must therefore remain
separate metrics.

## Defect found and fixed

The first IGN run exposed a generic Runtime projection defect. `text_any`
matched all requested phrases in canonical Truth, but `max_objects` was filled
in document order. Numerous early links inherited nearby `Guides` or `News`
heading context, so they could truncate exact later links. The model then issued
separate reads for Reviews and News even though the Extension already held both.

Runtime now:

1. records per-phrase match counts;
2. reserves one distinct result for every phrase with a match before filling
   the remaining response budget; and
3. prefers a match in the object's own safe name/text/description over one
   found only through nearby heading context.

This is a Runtime-only working-set selection change. It does not change the
Extension candidate, canonical Truth, cursor/delta semantics, Profile boundary,
or either wire schema. On IGN, the first working set then returned exact
`Reviews`, `News`, and `Guides` together. Runtime tests pass 66/66 and closed-loop
tests pass 12/12.

## Negative prompt experiment

A second four-report experiment on IGN and GitHub removed the detailed Saccade
lane guidance and relied almost entirely on MCP self-description. All four
reports still passed, proving the tool is discoverable and usable, but Saccade
mean calls rose from 4.8 to 5.5 and mean input tokens rose from 151,623 to
181,407 for those same sites. The compact prompt was therefore not retained.
The evidence remains at
`~/Library/Application Support/Saccade Dev/evidence/20260819-heavy-public-codex-compact-prompt/`.

## Limits and next evidence

- These public pages may be represented in model training data. This is
  compatibility evidence, not unknown-page generalization evidence.
- This matrix used one machine, one Codex model setting, one day, Chrome for
  both products, and only read-only goals.
- It does not compare Saccade software actions, Agent-client hard execution,
  upload, download, iframe interaction, Canvas, or WebGL.
- IGN still truthfully reported restricted third-party frames; those did not
  prevent the root-page task.
- No release-wide speed, token, or precision superiority claim is authorized
  from this matrix alone.

The next fair suites should keep these read results frozen and separately test
public semantic actions, downloads/uploads, same-origin and restricted iframes,
and opaque Canvas/WebGL limitations. Execution results must not be mixed into
this read-only matrix.
