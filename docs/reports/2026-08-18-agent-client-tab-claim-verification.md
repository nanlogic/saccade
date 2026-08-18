# Agent-client tab claim: lifecycle fix and verification

Date: 2026-08-18
Extension candidate: `86fda146ce3024c005029c774caa4f37998a4dfa4a65ee7e30155f3ac8881b2a` / 0.3.22
Runtime: `execution_owner=agent_client`, `reference_actuator_active=false`

## Defect

`considerClaimCandidate` spent its single origin decision on the first URL that
produced a non-null origin. A tab created by an Agent client is reported as
`chrome://newtab/`, and `new URL('chrome://newtab/').origin` parses to a
non-null `chrome://newtab`, so the candidate was deleted at `tabs.onCreated`
and could never latch when the client's later navigation reached the armed
origin. The previous "has not settled on a URL yet" guard only caught `''`.

Every shipped claim test created its tab already at the final URL, so no test
covered the create-blank-then-navigate lifecycle that is the only sequence an
Agent client can actually perform.

## Fix

One lifecycle rule in `extension/src/service_worker.js`: a candidate is decided
only on the first HTTP(S) URL. Non-HTTP(S) initial pages (`chrome://newtab/`,
`about:blank`, empty) are not settled and do not consume the candidate. On the
first HTTP(S) URL exactly one decision is made — origin match latches, origin
mismatch deletes the candidate, and navigating back to the armed origin
afterwards gets no second chance.

Unchanged: 30s TTL, first-qualifying-tab-wins, single-use claim, exact
`tab_id`, origin equality, uniform failure message, Agent Off default.

Regression tests in `extension/tests/tab_claim.test.js` cover all six cases,
including the wrong-origin-then-right-origin case and post-TTL navigation.

## Verification

Three real rounds, tab always created by the Agent client's own browser tooling
(`tabs_create_mcp`), never by `saccade.tabs.open`. No claimless `tabs.open`
substitution in any round.

| Round | Target | claim | tab | confirm | pressed / semantic delta |
| --- | --- | --- | --- | --- | --- |
| 1 | local fixture `pushed_delta.html` | `claim.7f5ece72…` | 1680323181 | `agent_client` | `o1 pressed false→true`, rev 38→51 |
| 2 | local fixture `pushed_delta.html` | `claim.fcc9e7ed…` | 1680323183 | `agent_client` | `o1 pressed false→true`, rev 21→35 |
| 3 | public `en.wikipedia.org` | `claim.84b1a967…` | 1680323188 | `agent_client` | `mode=delta`, rev 7→13, 9 semantic changes |

In every round `tabs.list` reported `ownership=agent`, `provenance=agent_client`,
`observation_ready=true` for the claimed tab only.

Round 3 used a reversible semantic operation (Wikipedia Appearance panel
collapse/restore) and the page was restored before the tab was closed. The
bounded delta cost 3,545 bytes against a 1,224,972-byte full view on a
3,196-object document.

### Authorization isolation

An unrelated tab created by the same Agent client on the same origin, never
armed or confirmed, was open throughout rounds 1 and 2 and never appeared in
`tabs.list`. Agent Off held.

## Scope limits — what these rounds do NOT establish

Rounds 1 and 2 required a screenshot to locate the control and a coordinate
click, and the first click on a freshly created background tab did not land; a
`ref`-based click did not land at all, in any round. Therefore:

- Claim handoff, authorization isolation, and Truth delta: **PASS**.
- A screenshot-free, coordinate-free benchmark: **NOT ESTABLISHED**. No timing
  or efficiency claim is authorized from these rounds.

The coordinate scaling (Truth reports CSS pixels; the client's screenshot frame
is scaled, ~1.26–1.31x here) and the swallowed first click on a background tab
are Agent-client behaviors. They are recorded here as harness constraints and
are **not** attributed to Saccade; Saccade reported the correct CSS-pixel
geometry and the correct post-click state in every case.

## Evidence limits carried forward

These limits stand regardless of the feature being committed, and must not be
erased by the act of committing it.

| Limit | Status |
| --- | --- |
| Edge real agent-client claim | **NOT VERIFIED** |
| Screenshot-free / coordinate-free execution | **NOT ESTABLISHED** |
| Round 3 public-site isolation control | **ABSENT** — the unrelated same-origin control tab was closed before round 3, so isolation is proven for rounds 1 and 2 only |
| Benchmark / performance / token / precision claims | **NOT AUTHORIZED** |
| Coverage | **LIMITED** — one client, one machine, one Chrome, one public site |

## Edge

`./scripts/dev.sh lifecycle edge` passes on the new candidate: extension loads,
`execution_owner=agent_client`, all eleven lifecycle markers observed, deltas
served. Evidence:
`evidence/20260818T003711Z/edge/truth/lifecycle.json`.

That run is `stimulus=page_driven_fixture`. No Agent client capable of creating
a tab is available on Edge in this environment, so **real Edge claim execution
(arm → client-created tab → confirm) is NOT verified**. No equivalent evidence
has been substituted for it.

`denominator` was not run: it is a both-browser route that tears down the
attached ordinary Chrome session, and no Edge Agent client exists to complete
the claim rows regardless.

## Candidate consistency

`86fda146…` is identical in source `extension/candidate.json`,
`extension/src/candidate_identity.js`, `packages/setup/release.json`, the
installed extension directory, the runtime's
`expected-extension-candidate.json`, and the live extension reported by
`saccade.system.capabilities` in Chrome.
