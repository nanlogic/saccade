# Architecture decisions

## 2026-07-27 — One final product route

Accepted: Saccade is an open closed-loop control runtime for authenticated
Chrome/Edge tabs. The only production route is Extension → Native Host mode →
owner-only local IPC → MCP mode.

## 2026-07-27 — One runtime executable, separate modes

Accepted: Native Host and MCP share runtime code and one shipped executable but
retain separate process modes, stdin/stdout framing, launch lifecycles, and
protected-data boundaries.

## 2026-07-27 — Control SDK and generated public coverage

Accepted: control families are modular and implement one closed-loop contract.
A machine-readable Catalog becomes the source for the public support matrix,
fixtures, conformance checks, and evidence links. End users do not install the
SDK; reviewed modules are bundled with releases.

## 2026-07-27 — Platform delivery

Accepted: macOS ships a signed/notarized DMG and Windows ships a signed Setup.
They share Extension, protocol, Catalog, modules, and release version while
using platform-specific system input, signing, permissions, and registration.

## 2026-07-27 — Historical engines

Rejected for production: CEF and Servo browser shells. Their algorithms,
fixtures, and evidence may be studied or selectively ported, but they cannot
re-enter the default runtime or create an alternate browser route.

## 2026-07-27 — Coverage and verification

Accepted: common controls require full closed-loop proof; uncommon controls
require truthful basic recognition and explicit limitations. OS input delivery
or a page revision change alone is not a verified semantic action.

## 2026-07-28: Profiles provide behavior and ban named controls

Accepted: a public Profile contains `name`, Agent-facing `behavior`, and a
`ban` list. Each ban entry names a control and may include a text `condition`.
Matching ignores case and whitespace differences. A missing condition bans the
named control. A present condition bans it only when the control's associated
text contains that condition.

Accepted: control modules retain one closed loop and do not read Profile data.
The Native Host filters banned controls before MCP exposure and accepts action
tokens only from its current filtered observation. Human control remains
available. `catalog/profile.schema.json` defines the public JSON.

Accepted: `saccade.observation/1` and `saccade-extension-host/1` remain
unchanged. The Runtime exposes the active Profile name and behavior through
`saccade.system.capabilities` as `saccade.capabilities/4`.

## 2026-07-28: First Control SDK slice frozen

Accepted: the SDK v1 module boundary, Catalog fields, Registry dispatch,
finite native primitives, and verifier contract are frozen for button, text
field, checkbox, and select. Paired managed macOS run `20260728T224742Z`
verified click, type, click, and select in Chrome for Testing and Microsoft Edge
through the same Extension, Native Host, Runtime, MCP, and native-input route.

Accepted: local development evidence freezes the engineering contract but does
not publish support. Catalog rows remain `implementation` with browser evidence
`pending` until signed-product and release-candidate gates pass. Later control
families extend the Catalog and Registry without changing Profile fields or the
two v1 wire schemas.

## 2026-07-29: First editable family extends the frozen SDK

Accepted: search field, textarea, contenteditable, and spin button extend the
Catalog and Registry by reusing the finite Unicode-text primitive and
`has_value` verifier. Each role keeps its own safe-state projection;
contenteditable names come only from external accessible metadata, never from
editable text. Readonly variants expose no action token or affordance.

Accepted: paired managed macOS run `20260729T043308Z` verified all eight
current controls in Chrome for Testing and Microsoft Edge through the same
Extension, Native Host, Runtime, MCP, and native-input route. Editable inputs
and fixture sentinels were absent from saved evidence. This is development
evidence only: Catalog status remains `implementation` and release evidence
remains `pending`. Profile fields and both v1 wire schemas are unchanged.

## 2026-07-29: Ordinary native mouse accuracy has a separate gate

Accepted: `./scripts/dev.sh accuracy all` runs 24 static semantic button
targets in managed Chrome and Edge. It covers left, center, right, and scrolled
positions with eight targets each at 32, 40, and 48 CSS pixels. It requests
only button action tokens through MCP and requires `accepted_by_os` plus a
verified pressed-state transition for every target. It does not use a reflex
loop, locator, Agent coordinate, screenshot, CDP, or browser input API.

Accepted: macOS primary click now uses a CoreGraphics HID-system event source
and the reviewed legacy human-input sequence `move → 50 ms → down → 50 ms →
up`. Only this finite native event behavior was migrated from private legacy
commit `8c4defb3f8b0`; the old CEF/Servo MouseAccuracy route was not migrated.

Accepted: the gate addresses the exact managed browser PID and repairs its
isolated profile's crash-exit marker before launch. Targets are split across
baseline, moved, and moved-and-resized window phases so prepared screen bounds
cannot rely on launch geometry. This followed a truthful failed run where a
Codex Pet layer-3 window intercepted right-side clicks. DOM topmost cannot
preflight unrelated OS windows under the v1 schema; an intercepted click
therefore remains unverified. Paired run
`20260729T053405Z` passed 24/24 in Chrome and 24/24 in Edge on reused managed
profiles; dynamic-window Chrome run `20260729T064702Z` passed 24/24 with zero
misses. Local evidence does not promote Catalog status.

Accepted: a stale prepare remains rejected. When the collector is newer than
the Host after startup or reconnection, that rejection also triggers a fresh
full observation so the next revision-bound attempt can recover.

## 2026-07-29: Reflex targets support native and soft input backends

Accepted: the single Extension → Native Host → owner-only IPC → MCP route has
two explicitly reported input backends. `native` posts real OS input and returns
`accepted_by_os`. `soft` is restricted to the Catalog-backed `reflex_target`,
dispatches a software pointer event inside the Extension, and returns
`accepted_by_software`. Both use the same opaque token, prepare, revision,
visibility, topmost, replay, reobservation, and semantic-verification checks.
Agents never provide or receive coordinates or locators.

Accepted: one MCP call may run the bounded reflex hot loop locally. Ordinary
stale targets are reobserved and never replayed. A hit is verified only when
the same loop class advances its safe occurrence counter. MouseAccuracy's
narrow semantic bridge recognizes only current `.target:not(.hit)` objects and
uses score advancement as the occurrence. Its canvas is not treated as a
general actionable surface.

Accepted: Profile remains exactly `name / behavior / ban`; it neither selects
an input backend nor changes a control loop. The new semantic role and dispatch
statuses are additive under the existing v1 wire names. The reflex Catalog row
remains `implementation` pending release evidence.

Accepted development evidence: managed Chrome run `20260729T064526Z` reached
`Insane + Tiny`, recorded 31 `accepted_by_software + verified` score advances
with zero failures, and measured 14.72 ms p50 / 15.76 ms p95
observation-to-receipt latency. It does not promote Catalog status.
