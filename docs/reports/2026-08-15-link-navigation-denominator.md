# Link-navigation replacement candidate denominator

Date: 2026-08-15

Extension `0.3.22` candidate:
`c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`.

This candidate adds bounded HTTP(S) `navigation_target` Truth for links while
keeping the five-tool public MCP surface and Agent-owned execution boundary.
The local link discovery → `tabs.open` → destination Truth loop passed before
the complete clean-profile denominator.

The complete Chrome and Edge denominator is stored at
`~/Library/Application Support/Saccade Dev/evidence/20260815T005149Z/denominator-63.json`.
It reports 63 total declarations, 56 local passes, 7 truthful limitations, zero
local blockers, and 63 publication blockers. Publication status is unchanged:
each row still requires its declared source-diverse, client-owned public
evidence.

Both browsers passed pushed delta, Resource subscription, the 137-event
latency probe, control and semantic coverage, stream recovery, and the
11-scenario lifecycle matrix. Chrome p95 was 33.272 ms; Edge p95 was
31.742 ms. The candidate was measured from a dirty working tree at commit
`20c170058c0c563432baad21f5489ded7c5c497b`; this is valid local candidate
evidence, not a frozen release commit.

The unpublished macOS Runtime artifact SHA-256 is
`e4140b180e85557b483a9cd232648642decaaab8854bc653c254c8da24ac780b`.
