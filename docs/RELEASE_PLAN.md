# Developer Preview release plan

## Release target

The first public release has two user-facing components:

1. the Saccade Extension from the Chrome Web Store or Edge Add-ons;
2. `npx -y @saccade/setup`.

The explicit setup command installs the headless local Runtime, user-level
Native Messaging manifests, and local MCP entries for supported Codex and
Claude clients. It does not install a visible application, run through an npm
`postinstall` hook, request Accessibility, or configure the Reference Actuator.
`docs/SETUP_TARGET.md` is the normative setup contract.

The first candidate targets local macOS clients with current Chrome and Edge.
Windows uses the same command after its platform Runtime and lifecycle tests
pass. Cloud-only Agent sessions cannot reach the local Extension and Native
Host and are incompatible with this release.

The preview covers the complete machine inventory: 34 protocol Truth roles,
12 reusable variants, and 6 structural/push boundaries. The separate 15-family
Reference Actuator catalog is development tooling, not the release surface.

## Product gates

- Default MCP exposes exactly capabilities, tab list/open/Agent-owned close,
  and `truth.read`.
- Capabilities `/6` identify `truth_layer`, push/resources, live/economy
  observation modes, and
  `execution_owner: agent_client`; default views contain no action authority.
- Full-to-delta, Profile bans, dynamic replacement, delayed render,
  same-origin iframe, open Shadow DOM, stream gap/reset, and unsolicited
  Resource updates pass in Chrome and Edge from the same commit.
- Every projected object carries current document and viewport bounds.
  Movement, resizing, scrolling, and rendered animation produce geometry
  updates on the same stable identity.
- Each common control has public recognition and state evidence from two
  independent sites, with multiple implementation types where applicable.
- Codex and Claude each act with their own tool in the same browser tab while
  Saccade reports the resulting delta. A client that cannot share the browser
  instance is reported as incompatible.
- Canvas/WebGL, closed shadow roots, cross-origin frames, browser-owned
  dialogs, built-in PDF, and semantically weak pages report limitations.

## Setup gates

- The project owns the `@saccade` npm scope and the published package has
  verifiable provenance.
- The command downloads the correct platform Runtime and verifies its release
  checksum before installation.
- Setup writes only user-level Runtime, Native Messaging, Profile, and local
  Agent-client configuration paths.
- Setup detects and configures supported Codex and Claude clients without
  overwriting unrelated MCP configuration.
- First install, repeat install, update, failed-update rollback, doctor,
  browser restart, Host restart, uninstall, and explicit purge pass in isolated
  user homes.
- Updates and ordinary uninstall preserve the user's Profile. A new default
  Profile is created only when none exists.
- Default setup requires no Accessibility permission and never installs or
  configures the Reference Actuator.
- Chrome Web Store and Edge Add-ons packages use the final Extension identity
  expected by the Native Messaging manifests.

## Evidence gates

Freeze one commit, Runtime and Extension versions, browser versions, setup
package version, and platform artifact checksums. Run the complete Truth,
lifecycle, public-site, and setup matrices against that candidate. Store
initial transfer size, delta size, revisions, observed transitions, current
geometry, failures, and limitations. Never store editable contents, protected
values, file paths, locators, cookies, browser storage, or action authority.

The fair Playwright comparison begins both lanes with the same unknown URL and
natural-language task. The Saccade lane uses Truth plus the Agent client's own
same-tab tool. The Playwright lane uses official Playwright MCP. Neither lane
receives prepared selectors, scripts, control names, or state from the other.
Record completion, discovery time, initial bytes and token estimate, delta
latency, re-observation count, stale recovery, tool calls, total time, and every
failure reason.

Reference Actuator results remain labeled development evidence. Its receipts,
input permissions, and failures cannot establish product execution capability
and are not a release gate.

## Work order

1. Converge candidate `0.3.22`, remove contradictory current plans, verify one
   Extension identity across source, install, Chrome, Edge, and Runtime, then
   run the full local denominator.
2. Run the order-reversed 3-task core-product Playwright comparison. Incomplete
   clocks, token accounting, same-tab proof, or delta metrics make a run
   `INVALID`, never favorable evidence.
3. Finish the explicit setup lifecycle, exact-candidate doctor, unpublished
   macOS Runtime artifact/checksum draft, and isolated Codex/Claude homes.
4. Complete source-diverse, client-owned public compatibility evidence and one
   Claude same-tab loop, then freeze commit, versions, browser versions, store
   origins, and checksums and rerun the complete matrix.

## Current readiness

- Candidate `0.3.22` is content-addressed as
  `c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`.
  Source, setup metadata, isolated Chrome, isolated Edge, Runtime expectation,
  and the complete denominator report that same identity.
- The 11-scenario page-driven lifecycle matrix, pushed deltas, Resource
  subscription, stream gap/reset, 137-event latency probes, and the complete
  local Truth inventory pass in Chrome and Edge for that candidate. Chrome
  p95 is 33.272 ms and Edge p95 is 31.742 ms in the final denominator run.
- Two consecutive macOS cold cycles each started with no normal test-browser
  window, used the fixed Extension wake surface, opened an Agent-owned tab,
  observed 63 objects, and closed it truthfully in 1.90 s and 1.72 s.
- The explicit 63-row denominator has current-candidate Chrome and Edge
  evidence for every row: 56 local passes, 7 truthful limitations, and zero
  local blockers. All 63 publication outcomes remain blocked until their
  declared public/client-owned evidence requirements are met. The current
  evidence is
  `~/Library/Application Support/Saccade Dev/evidence/20260815T005149Z/denominator-63.json`.
- A real Codex same-tab loop has passed: Codex acted with its own browser tool
  and Saccade reported the resulting state delta.
- Ordinary-Chrome candidate activation now fails closed on stale code and has
  passed an automatic live-candidate self-reload from one content-addressed
  build to the next. The development popup uses the existing blue-and-white
  brand icon and an explicit `DEV` badge; production builds hide that badge.
- An authenticated Steamworks onboarding dogfood passed for ordinary company,
  agreement, mailing-address, permission, and post-save observation work in an
  ordinary Chrome session. Saccade remained the Truth route and Codex owned
  same-tab execution. CAPTCHA, a restricted cross-origin account frame, and a
  server-reported account mismatch remained explicit external boundaries. The
  sanitized evidence is in
  `reports/2026-08-11-steamworks-onboarding-dogfood.md`.
- The dependency-free setup CLI has 13 isolated lifecycle/safety tests,
  including repeat install, Profile preservation, rollback, exact-candidate
  doctor, Codex/Claude clean-home configuration, uninstall, and purge. The
  unpublished arm64 Runtime draft SHA-256 is
  `550c382e3eace790c1c85b2f58cfe193471828584286fb8b00a2983d5acee20b`.
  It remains deliberately unsigned with null URL and empty store origins.
- The official comparison baseline is frozen to `@playwright/mcp@0.0.79` in
  `benchmarks/playwright-mcp.lock.json`. That version was re-resolved through
  the authorized Saccade route on 2026-08-17: `tabs.open` on the npm package
  page, one `truth.read`, the npm `status` object reading
  `Viewing @playwright/mcp version 0.0.79`, then `tabs.close`. The earlier
  `0.0.78` pin was never consumed by a comparison run, so this is a baseline
  correction rather than a re-benchmark. The historical Reference Actuator
  harnesses keep their own recorded `0.0.78` default so their retained evidence
  stays reproducible. The evidence harness rejects missing
  monotonic timing, model tokens, bytes, same-tab proof, delta latency, or
  replacement recovery. The full 3-task × 2-order matrix has now been run with
  Claude Code owning the Saccade lane's execution: both lanes completed 6/6
  tasks, but all six verdicts are `INVALID` on
  `model_input_tokens_missing` and `delta_latency_missing`. Both gaps exist
  because an interactive Claude session cannot report per-lane tokens or a
  turnaround-free action→delta interval; the local `claude` CLI is
  `Not logged in`, which is the single external unblocker. The one metric
  measured comparably, initial payload, does **not** favor Saccade
  (mean 31788 bytes versus Playwright 6065). The lanes also do not share a
  model, so model and browser route remain confounded. No speed, token, or
  payload superiority claim is authorized. Details are in
  `reports/2026-08-17-fair-benchmark-matrix.md`.
- A corrected same-model driver now exists in
  `scripts/benchmark_same_model_fair.py`: one `claude -p` binary and one
  `--model` drive both lanes, each wired to only its own MCP through
  `--mcp-config`, with wrapper-monotonic per-tool timestamps, stream-json token
  usage, automatic full-to-delta Saccade delivery, and cumulative discovery
  bytes. The 2026-08-18 unknown-page matrix ran with one Claude model and the
  locked official Playwright MCP: both corrected lanes completed 6/6. Saccade
  used one initial Truth read and zero re-observations but still transferred a
  larger first view; no blanket speed, token, or payload superiority claim is
  authorized.
- A real Claude Code same-tab loop has passed. Claude owned both clicks with its
  own Chrome tool in ordinary Chrome while Saccade supplied Truth only, and
  Saccade observed `pressed` move `false → true → false` on one stable identity
  at 0.606 ms and 0.435 ms revision-bounded reads. Saccade's `tab_id` and the
  Claude Chrome `tabId` were identical (`1680322942`, `ownership: agent`), and
  the Agent-owned tab was closed afterwards. Details are in
  `reports/2026-08-17-claude-same-tab-closed-loop.md`. This is one local-fixture
  loop: it removes the earlier `Not logged in` blocker but is not public-site
  evidence and does not by itself supply the fair-comparison Saccade lane, which
  still needs the harness's timing, token, byte, and replacement-recovery fields.
- The Chrome and Edge public diagnostics each recognize all 12 declared targets across
  Selenium, WAI-ARIA APG, Angular Material, PrimeVue, and Shoelace. Its optional
  Reference Actuator closes the same 7/12 loops in each browser; two text cases require native input
  permission and three framework cases retain explicit stimulus/postcondition
  blockers. This is observation evidence, not client-owned execution evidence,
  so source-diverse compatibility and the fair Playwright comparison remain
  incomplete. Local evidence does not promote Catalog rows to `publishable`.
  Authenticated single-browser dogfood is likewise engineering evidence and
  does not promote a site, browser, or Catalog row to `publishable`.
- Publication remains externally blocked on npm scope ownership, code-signing
  material, Chrome Web Store ID/origin, Edge Add-ons ID/origin, source-diverse
  public evidence, Claude login/equivalence, and a frozen commit. `published`
  remains false.

## Launch package

The release includes store links, the setup command, supported client and
browser versions, platform checksums, a five-minute quickstart, the public
architecture overview, a Profile example, the Truth coverage table, a
full-to-delta demo, a same-tab Agent-owned action demo, redacted diagnostics,
and explicit limitations.

Wayne approves the frozen candidate after reviewing setup behavior, evidence,
limitations, and launch copy.
