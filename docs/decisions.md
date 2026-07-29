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

Accepted: managed browsers use fixed unobstructed window geometry and repair
their isolated profile's crash-exit marker before launch. This followed a
truthful failed run where a Codex Pet layer-3 window intercepted right-side
clicks. DOM topmost cannot preflight unrelated OS windows under the v1 schema;
an intercepted click therefore remains unverified. Paired run
`20260729T053405Z` passed 24/24 in Chrome and 24/24 in Edge on reused managed
profiles. Local evidence does not promote Catalog status.

Accepted: a stale prepare remains rejected. When the collector is newer than
the Host after startup or reconnection, that rejection also triggers a fresh
full observation so the next revision-bound attempt can recover.
