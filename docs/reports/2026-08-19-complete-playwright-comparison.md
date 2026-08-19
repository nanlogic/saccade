# 2026-08-19 complete Playwright comparison

Candidate: `4c5ff8e20489e72b75437c8772a37e27341d2d40327bfe2d3de1c03232be6d54`
(`0.3.22`). Browser: Chrome. Playwright: the exact official version pinned in
`benchmarks/playwright-mcp.lock.json`. Both orders were run for every task.

## Codex

The public Selenium, DemoQA React, and Angular Material matrix passed 6/6 for
both Saccade and Playwright. Saccade averaged 45.6 s, 6.67 tool calls, and 4,436
initial discovery bytes. Playwright averaged 38.6 s, 7.67 calls, and 16,197
initial discovery bytes. Saccade therefore transferred much less discovery
state on these complex public pages and used fewer calls, but was slower and
used more model input overall. This does not authorize a blanket superiority
claim.

The one-time generated native/reveal/replace matrix also passed 6/6 for both
lanes. Saccade averaged 59.7 s, 9.67 calls, and 2,658 initial bytes; Playwright
averaged 32.9 s, 8 calls, and 1,374 initial bytes. On these small unknown pages,
Playwright was clearly more efficient. Dynamic replacement required Saccade to
obtain the replacement identity and continue from current Truth; that recovery
was correct but not cheaper.

## Claude

The first Claude run was correctly INVALID: the Claude tool registry rejected
the top-level `oneOf` in the public `saccade.act` schema before any Saccade
browser operation. The Runtime schema was flattened while strict cross-field
validation remained server-side. After rebuilding the installed Runtime, a
real smoke passed and the complete Claude Opus 5 low public matrix passed 6/6
for both lanes.

Across those six Claude reports, Saccade averaged 50.8 s, 7.67 calls, 5,526
initial discovery bytes, and 238,109 logical input tokens. Playwright averaged
41.8 s, 8.83 calls, 8,920 initial bytes, and 234,094 logical input tokens.
Saccade again reduced discovery transfer and calls, while Playwright remained
faster and slightly lower-token overall.

## Capability boundary

The same candidate separately passed the complete Chrome and Edge Truth,
pushed-delta, resource, latency, control, and semantic-role gates. Those gates
cover same-origin iframe composition and truthful restricted/opaque boundaries.
Canvas/WebGL without an approved semantic bridge remains opaque. Upload is a
projected `file_input` handoff rather than a public `saccade.act` operation, and
download lifecycle is not equivalent to the click/select/type engine matrix.
These cases are reported separately rather than assigned a fabricated combined
score against Playwright.

Evidence roots:

- `~/Library/Application Support/Saccade Dev/evidence/20260819T120034Z`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T120321Z`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T120703Z`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-complete-unknown-codex`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-complete-public-claude-fixed`
