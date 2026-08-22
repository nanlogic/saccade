# Speed and stability convergence evidence

Date: 2026-08-19

Candidate: `8be096270574424581c54296b8226e92024e2865a7a2af14b14565c416b7d9e8`
(`0.3.23`). MCP contract:
`6b5eb2ab18b9ab0948d12701ad177e9c34f1efb0edf67463c3bfa83b969a2ca5`.

## Implemented

- The fixed MCP control plane is 5,587 bytes for Saccade versus 18,711 bytes
  for the locked Playwright MCP 0.0.79 in the same accounting. Initialize and
  tool descriptions no longer repeat Profile behavior. Capabilities deliver it
  once and expose Runtime, Profile, candidate, and contract identities.
- Software action preparation has a zero-wait immediate path and a bounded
  Collector-local actionability wait. Two stable animation frames plus current
  visible, topmost, focus, enabled, document, identity, and token checks are
  required. Only temporary enablement may rebase; replacement stays stale.
- Benchmark evidence separates control plane, discovery, steady state, model
  usage, stability, and infrastructure. API overloads, rate limits, timeouts,
  zero-tool runs, and stale contracts invalidate evidence rather than scoring a
  lane loss.
- Unknown oracle-generated queues cover lengths 1/5/10/25/50 and three change
  classes. The runner can resume only prior PASS reports and preserves seeds.
- `saccade.act` returns a compact bounded summary of public transition signals,
  so an inline completion marker need not be rediscovered through another read.

## Browser and stress evidence

The Chrome/Edge denominator report is:

`~/Library/Application Support/Saccade Dev/evidence/20260819T181616Z/denominator-63.json`

Both browsers used the same candidate. Each has 56 local passes, seven truthful
limitations, zero local blockers, passing lifecycle, and 137/137 latency
samples. Chrome p50/p95 was 26.236 ms and Edge p50/p95 was 22.459 ms. Chrome's
same-turn fast-path comparison was +9.65%, inside the stated 10% ceiling; Edge
improved.

The 100-loop actionability evidence is:

`~/Library/Application Support/Saccade Dev/evidence/20260819-actionability-wait-100.json`

Animation, temporary overlay, delayed enablement, and replacement recovery each
passed 100/100. Replacement was rejected stale before an exact-object recovery;
it was never silently rebound.

## Long-horizon status

Evidence root:

`~/Library/Application Support/Saccade Dev/evidence/20260819-long-horizon-final/`

All 30 paired reports are PASS: five lengths, three change modes, and both lane
orders. The independent oracle marker is the ground truth for both lanes; a
model's contradictory final JSON is retained as a diagnostic and cannot erase
or manufacture the marker.

Mean results across both orders:

| Mode / length | Saccade calls | Playwright calls | Saccade time | Playwright time | Saccade steady bytes | Playwright steady bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| same identity / 1 | 5 | 5 | 27.7 s | 24.9 s | 1.8 KB | 1.0 KB |
| same identity / 5 | 9 | 13 | 39.0 s | 46.4 s | 8.0 KB | 6.1 KB |
| same identity / 25 | 29 | 70 | 45.1 s | 68.0 s | 38.9 KB | 42.5 KB |
| same identity / 50 | 55 | 104 | 51.0 s | 76.1 s | 77.5 KB | 64.5 KB |
| replacement / 50 | 54.5 | 103.5 | 71.6 s | 64.8 s | 154.4 KB | 63.2 KB |
| navigation / 50 | 103 | 103.5 | 57.3 s | 67.0 s | 156.7 KB | 60.0 KB |

The real break-even in this sample begins at five same-identity steps: Saccade
uses fewer calls and lower mean elapsed time from that point, reaching roughly
half the calls at length 50. This is the long-flow advantage the test was meant
to isolate. It is not a blanket token claim: non-cached model input is noisy and
does not consistently favor either lane.

Replacement and navigation expose the honest cost boundary. Saccade still
reduces calls for replacement, but its full replacement transitions are much
larger and the 50-step run is slower. True document navigation needs one new
document view per record, so call counts converge and Saccade steady bytes are
larger. Full→delta is advantageous when the document and useful identity stay
live, not when every step deliberately destroys them.

## Static verification

- Rust workspace: 71 Runtime tests, 12 closed-loop tests, protocol/Host/SDK and
  doc tests pass.
- Extension plus setup: 81/81 tests pass.
- Python: 146/146 tests pass.
- Workspace Clippy with warnings denied, Python compilation, architecture gate,
  formatting, candidate identity, and diff whitespace checks pass.

## Current-contract compatibility matrices

The current candidate and MCP contract passed the Selenium, DemoQA React, and
Angular Material tasks in both lane orders (6/6 paired reports):

- `~/Library/Application Support/Saccade Dev/evidence/20260819T194530Z/`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T194833Z/`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T195149Z/`

The one-time generated native, reveal, and replacement tasks also pass both
orders (6/6):

`~/Library/Application Support/Saccade Dev/evidence/20260819-unknown-current-contract/`

The six public read-only sites pass both orders (12/12 paired reports):

`~/Library/Application Support/Saccade Dev/evidence/20260819-public-current-contract/`

Across those public reports Saccade averaged 8.7 KB discovery, 21.8 seconds,
4.0 calls, and 17.6k non-cached input tokens. Playwright averaged 16.8 KB,
33.2 seconds, 4.6 calls, and 15.6k non-cached input tokens. The sample supports
lower Saccade discovery bytes and elapsed time, but not a lower model-token
claim. One generic prompt defect was fixed: a read-only task naming several
targets now requests one combined structural/action working set rather than
letting its actionable target suppress the required heading.

## Remaining release gates

- Claude was started in a fresh Saccade-only session after the Codex evidence
  froze, but Anthropic returned `API Error: Rate limit reached` before the first
  tool call. The report is correctly `INVALID`, with zero foreign browser tools
  and no product result:
  `~/Library/Application Support/Saccade Dev/evidence/20260819-claude-current-contract-soft-smoke/`.
  Repeat this same smoke after the account limit resets; Claude's own
  tab/snapshot route is not equivalent.
- Upload/download, restricted iframe, Canvas, and WebGL remain separately
  reported capability boundaries rather than fabricated combined scores.
