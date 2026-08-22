# Provisioned Agent-client tab claim: completing `saccade.tabs.open` arm/confirm

Date: 2026-08-17. Extension candidate rebuilt to
`dab535dd1f6c8ad0d827e627c81e50ee231a6efadddf0a0d5f0b34df8287ab0e`
(version `0.3.22`, previously
`c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`).
Wire schemas unchanged: `saccade.observation/1`, `saccade-extension-host/1`,
`saccade.capabilities/6`. Public MCP tool count unchanged: exactly five.

## Why

`docs/reports/2026-08-17-same-tab-handoff-blocker.md` established that some
Agent clients can act only in tabs they created themselves, so a
Saccade-created tab is unusable to them and the same-tab closed loop cannot
complete. The fix must not make new tabs Agent On by default, because that
would convert a per-tab human consent boundary into an ambient one.

## What was audited

`extension/src/service_worker.js` carried a half-finished claim from a prior
session. Present and correct on arrival:

- `CLAIM_TTL_MS`, the session-only `pendingClaim` variable with its
  never-persist comment, `claimedAgentTabs`, and `activeClaim()` expiry check.
- `tabProvenance()` returning `agent_client` for claimed tabs, and
  `tabStatus()`/`tabs.list` already surfacing `provenance`.
- `armTabClaim()` and `confirmTabClaim()` including the uniform
  `tab claim could not be confirmed` rejection, the one-shot consumption of the
  claim on every confirm attempt, and the `user_shared` exclusion.
- `handleHostCommand` dispatch for `claim: "arm"` / `claim: "confirm"`, and the
  `claim must be arm or confirm` rejection of any third mode.
- ACL persistence of `claimed`, `forgetTab()` clearing a latched claim, and
  `resetAclForBrowserStartup()` clearing everything.

Missing, which is why the loop could not close:

1. **Nothing ever latched a tab.** `confirmTabClaim` requires
   `claim.latchedTabId !== null`, but no code path ever set it, so every
   confirm failed. This was the functional dead end.
2. **The public MCP schema still exposed only `{url, active}`.** Both
   `crates/saccade_runtime/src/mcp.rs` (`validate_arguments` and the tool's
   `inputSchema`) and `crates/saccade_runtime/src/session.rs` rejected `claim`,
   `claim_id`, and `tab_id` as unexpected arguments, so the extension logic was
   unreachable from any real Agent.
3. **No revocation on Native Host session disconnect** for claimed tabs.
4. **`tabs.onRemoved` bypassed `forgetTab`**, so a removed tab left its
   `claimedAgentTabs` entry and any latch behind.
5. **No tests at all**, and no documentation.

The existing partial implementation was completed in place; nothing was
rewritten or replaced.

## What was completed

**`extension/src/service_worker.js`**

- `noteClaimCandidate(tab)` on `tabs.onCreated` and `considerClaimCandidate()`
  on both `tabs.onCreated` and `tabs.onUpdated`. Only the event payload for a
  tab created *after* arming is inspected — no `tabs.query`, no enumeration, no
  read of any other tab. An already-authorized tab is never a candidate, so a
  `user_shared` tab cannot be captured. A candidate is decided exactly once when
  its URL settles: matching HTTP(S) origin latches, anything else is dropped
  permanently even if that tab later navigates onto the armed origin. Latching
  clears the remaining candidate set, so a second qualifying tab cannot be
  claimed by the same intent.
- `revokeClaimedTabs()` on native port disconnect, revoking claimed tabs only
  and leaving `user_shared` and `saccade_tabs_open` ownership untouched.
- `tabs.onRemoved` now routes through `forgetTab`, and `forgetTab` clears the
  claim's candidate set.

**`crates/saccade_runtime/src/mcp.rs`**

- `tabs.open` accepts `claim` (`arm` | `confirm`), `claim_id`, and `tab_id`.
  `confirm` requires both `claim_id` and `tab_id`; any other mode forbids them;
  `active` is forbidden alongside a claim because a claim creates no tab.
- The published `inputSchema` is a flat object accepted by Agent tool
  registries, including Claude. Its description states the cross-field rules;
  Runtime validation authoritatively enforces the arm/confirm combinations.
- The macOS zero-window browser wake is skipped for claim modes: a claim never
  creates a tab, and the Agent client already owns a live browser.
- Initialization instructions describe the arm → create → confirm loop in
  vendor-neutral terms.

**`crates/saccade_runtime/src/session.rs`**

- `tabs.open` plumbs the claim parameters to the Extension with a per-mode
  allow-list. `arm` returns immediately with no tab identity and no observation
  wait; `confirm` waits for first Truth from the confirmed tab and reports
  `observation_ready`.

**`extension/popup.js`** — an Agent On tab claimed by a client now says so, and
still offers Stop sharing. No change to the Agent Off or unsupported paths.

**Not touched:** Collector, control modules, Profile boundary, and
protected-value redaction. `docs/PROFILE_ARCHITECTURE.md` was deliberately left
alone because the Profile boundary is unaffected.

## Tests

New: `extension/tests/tab_claim.test.js` (13 tests). It loads the real Service
Worker source in a `node:vm` realm behind a recording Chrome double whose
`tabs.create` and `windows.create` throw, so a claim that tried to create a tab
would fail loudly.

| Requirement | Covered by |
| --- | --- |
| Arm creates/reads/authorizes nothing | `arm creates, reads, and authorizes nothing` asserts the recorded Chrome call log is empty and `tabs.list` stays empty |
| Claims expire after a short TTL | `a claim expires after its short TTL` (asserts `CLAIM_TTL_MS === 30_000`, shifts the sandbox clock) |
| Only the first new matching tab is locked | `only the first matching new tab is latched…` |
| Correct token + tab_id + origin succeeds | same test, plus the pending-URL variant |
| Wrong token / tab_id / origin / expired all fail and consume | `every mismatch fails uniformly and consumes the single-use claim` (4 cases, each asserting the identical message and a failed retry) |
| Non-matching tab during the window is never authorized | `a tab whose URL settles after creation is latched only when the origin matches` |
| Pre-existing tab never claimable | `a pre-existing tab on the armed origin is never claimable` |
| Ordinary user tabs stay Agent Off | `ordinary user tabs stay Agent Off and user_shared lifecycle is unaffected` |
| Successful confirm yields `agent_client` | asserted on the confirm reply, `tabs.list`, and popup status |
| Cleanup on Stop sharing / tabs.close / tab removed / host disconnect / browser startup | `a claimed tab is revoked by…` (4 scenarios) and `browser startup clears every claimed tab…` |
| `user_shared` lifecycle unaffected | share still works, claim cannot steal it, `tabs.close` still refuses it, host disconnect leaves it authorized |
| Protected-value redaction unaffected | `protected-value redaction rules are untouched by the claim`, plus the Collector is asserted to contain no claim logic |
| One generic Chrome/Edge codepath | `the claim is one generic Chrome/Edge codepath with no browser branch` — the claim region is asserted free of `BROWSER_FAMILY`, `userAgent`, `Edg/`, debugger, screenshot, and Playwright references, and the whole worker free of vendor names |

New Rust tests: `mcp::tests::rpc_and_first_slice_tools_are_strict` gained the
published-schema assertions (enum, conditional requirements, still exactly five
tools, no vendor string anywhere in the tool JSON) and eight new
`validate_arguments` cases. `session::tests::tabs_open_claim_arms_without_a_tab_and_confirms_one_agent_client_tab`
covers the Host forwarding for both modes.

Modified: `extension/tests/protocol.test.js` — the `tabs.open` source slice now
targets the last (Saccade-created) branch, since arm and confirm are matched
ahead of it. The assertions themselves are unchanged.

### Commands run and results

| Command | Result |
| --- | --- |
| `node --test extension/tests/*.test.js` | 43 passed, 0 failed |
| `cargo test --workspace --offline` | all suites pass (36 runtime lib, 12 protocol, closed-loop, etc.), 0 failed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | clean |
| `node --test packages/setup/test/*.test.js` | 14 passed, 0 failed |
| `python3 -m unittest` for `test_dev_profile`, `test_dev_probe`, `test_benchmark_agent_fair`, `test_external_dogfood`, `test_public_truth_cases` | OK |
| `python3 -m unittest` for `test_dev_lifecycle`, `test_lifecycle_truth`, `test_summarize_fair_matrix`, `test_build_setup_release`, `test_audit_public_evidence` | OK (18) |
| `python3 -m unittest` for `test_truth_latency`, `test_denominator_evidence` | OK (9) |
| `python3 -m unittest` for `test_benchmark_same_model_fair`, `test_run_same_model_matrix`, `test_run_claude_same_tab`, `test_probe_no_window_recovery` | OK |
| `python3 -m py_compile scripts/*.py` | OK |
| `python3 scripts/check_single_architecture.py` | `single architecture gate: ok` |
| `git diff --exit-code` on generated Catalog outputs | clean |

`test_dev_lifecycle` initially failed because the Extension candidate is content
addressed and the Service Worker changed. `scripts/write_extension_candidate.py`
regenerated `extension/candidate.json` and `extension/src/candidate_identity.js`
(verified idempotent), and `packages/setup/release.json` was repointed at the new
candidate as `check_single_architecture.py` requires.

## Real-Chrome verification: NOT PERFORMED

This was **not** verified against a real Chrome instance with a real Native
Host, a real tab claim, a real Truth read, and a real delta. A dev runtime
(`Saccade Dev Runtime.app`) and an attached managed Chrome were live during this
session, but both were running the *previous* build and the previous Extension
candidate, so they cannot exercise the new claim path. Doing the verification
requires rebuilding the runtime and reinstalling the unpacked Extension through
`./scripts/dev.sh`, which restarts the managed browser and tears down the
attached session — a user-visible side effect not taken unilaterally here.

Everything reported above is from unit and lifecycle tests against the real
Service Worker source and the real Rust MCP/session code. **No claim is made
that the end-to-end browser loop has been observed working.** The next step is a
`./scripts/dev.sh` reinstall followed by an arm → client-created tab → confirm →
`truth.read` → act → revision-bounded delta run, recorded as its own report.

## Invariants not fully verified

- **Real-Chrome behavior of `tabs.onCreated` for tabs created by another
  automation client.** The tests model both the settled-URL and `pendingUrl`
  shapes, and the implementation handles a tab that reports no URL at creation
  and settles later. Chrome's exact event ordering for a foreign client's tab
  has not been observed against this build.
- **Whether a claimed tab is reachable by the specific Agent clients that
  motivated the change.** The claim removes Saccade's side of the blocker; it
  cannot fix a client that refuses to act in a tab for unrelated reasons. That
  is a per-client question for the follow-up closed-loop run.
