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

## 2026-07-29: Link and file selection extend the Registry

Accepted: `link` uses the existing native primary-click primitive and verifies
only a document transition. A delayed navigation may therefore complete after
the bounded receipt window while the receipt remains unverified; OS delivery
or scrolling alone is never upgraded to success.

Accepted: `file_input` uses a finite native chooser primitive and `upload`
operation. The Agent may supply one absolute accessible regular non-symlink
file path only in the immediate action payload. The path is not forwarded to
the Extension and is absent from observations, receipts, diagnostics, logs,
and evidence. Directory, symlink, and multi-file selection remain unsupported.

Accepted: an unambiguous visible button that creates an ephemeral native file
input may project as `file_input`. Its safe name must describe choosing or
uploading files, and verification requires a real file-input `change` with a
non-empty selection. That verifies chooser acceptance, not remote upload
persistence; a server-loaded page effect must prove the latter.

Accepted development evidence: authenticated itch.io dogfood selected the
37.8 MB Gear Up PDF with `accepted_by_os + verified`, leaked no path into the
receipt, observed the file-row count advance from three to four, and confirmed
the fourth row plus `Graphics=true` disclosure in a fresh edit document.
Catalog status remains `implementation` and Edge/release evidence remains
pending.

Accepted: managed development browser profiles have an explicit generation
independent of Extension version, while unpacked Extension directories are
versioned. A broken or stale MV3 worker can be isolated by advancing the profile
generation without reading or copying cookies; the prior profile remains
untouched.

## 2026-07-29: Browser-owned confirmation remains human-only

Accepted: `saccade.system.capabilities` lists `browser_owned_confirm` as a
restricted surface with human confirmation required. Chrome and Edge do not
expose these dialogs to the page Extension as revision-bound objects. Saccade
does not intercept `window.confirm`, send an unrestricted key, or add a browser
chrome automation route. A receipt stays unverified until page state proves the
result after the human confirms.

## 2026-07-29: Image identity uses an explicit semantic bridge

Accepted: a named image may declare a bounded
`data-saccade-image-identity`. The Extension exposes it as a non-actionable
image description so an Agent can assert the same identity after a fresh
document load. Saccade does not expose image URLs, screenshots, or pixels and
does not claim pixel equality when a page omits the bridge.

## 2026-07-29: Radio, switch, tab, and menu item stay independent loops

Accepted: all four controls reuse the finite native primary-click primitive,
but they do not collapse into a generic clickable widget. Radio and switch
verify checked-state transitions, tab verifies becoming selected, and the v1
menu-item action verifies an expanded-state transition. Native radio behavior
must also preserve group exclusivity in the browser fixture.

Accepted development evidence: Chrome run `20260729T192723Z` and Edge run
`20260729T192757Z` each produced 12 `accepted_by_os + verified` receipts on the
same source candidate, rejected the consumed stale token, passed Profile
filtering, and projected the explicit image identity without an action token.
Catalog rows remain `implementation` with release evidence `pending`.

## 2026-07-29: Public dogfood precedes compatibility claims

Accepted: a deterministic fixture proves the control contract but does not by
itself prove public-page compatibility. A control-family claim requires Saccade
to run independently on public pages through Extension, Native Host, Runtime,
MCP, and native input. A separate Playwright harness may compare accessible
names, state transitions, and screenshots only after Saccade passes. Playwright
is not a fallback and cannot create or upgrade a Saccade receipt.

Accepted evidence: run `20260729T211221Z` matched radio, switch, tab, and menu
item on W3C WAI-ARIA examples in Chrome and Edge. The external gate exposed and
fixed missing names for ARIA radios, `aria-hidden` text leaking into switch
names, and native-anchor precedence over explicit menu-item roles.

## 2026-07-29: Structural reading is bounded and non-actionable

Accepted: the Extension projects visible headings, paragraphs, list items,
table cells, alerts, and status messages as `kind=text`. These objects carry
bounded visible text and safe structural state only. They never carry names,
affordances, or action tokens. The protocol rejects actionable text objects.

The collector excludes hidden nodes, nested controls and images, editable
contents, and nested structural objects that would duplicate their text. A
256 KiB UTF-8 budget is lower than the protocol-wide 2 MiB disclosure limit;
reaching it reports a truncated snapshot. Same-origin frame composition and
container-level list/table objects remain separate work.

## 2026-07-29: ARIA choices reuse select without becoming generic clicks

Accepted: native select, ARIA listbox, and ARIA combobox project as the existing
`select` role. Their choices remain distinct option objects, including options
with duplicate visible names. Preparation rejects disabled, detached, or
wrong-owner choices and computes the selected option's position among enabled
siblings.

The finite native plan clicks the current owner, waits for its popup, sends
Home, sends a bounded number of Down keys, and confirms with Return. This avoids
name-based typeahead ambiguity and preserves option object identity through
verification. Paired managed run `20260729T225249Z` produced 14
`accepted_by_os + verified` receipts in both Chrome and Edge, including native
select, duplicate-name ARIA listbox identity, and ARIA combobox selection. This
restores fixture development evidence only; release evidence remains pending.

## 2026-07-29: Existing tabs are shared from one session-only popup

Accepted: the Extension action popup shows whether the active tab is Agent
Off, user-shared, or Agent-owned. Only the popup may add or remove a user tab
from the existing `chrome.storage.session` ACL. Sharing supports HTTP and HTTPS
only, configures the collector before reporting success, and rolls back the ACL
if collector setup fails.

Revocation removes the user-shared tab, discards its observation session,
clears collector tokens, and disconnects its mutation observer. Agent-owned
tabs remain visibly distinct and are revoked by closing them. The popup does
not create a second authorization store or communicate with Native Messaging
directly.

## 2026-07-29: A tab document stream cannot move backward

Accepted: revisions are monotonic within a document, while navigation creates
a new document identity. Once the Host accepts a new document for a tab, it
retires the preceding identity and rejects delayed snapshots from it. A late
old-document message cannot overwrite the current observation or verify an
action against the wrong document.

This was exposed by intermittent Chrome/Edge text-field failures whose receipt
started on one document but settled against a delayed snapshot from its
predecessor. After the Host monotonicity fix, paired run `20260729T225249Z`
completed 14 native verified receipts plus Profile and stale-token gates in
each browser. Catalog status remains `implementation` because this is local
development evidence.

## 2026-07-30: Catalog defaults and local experience select the input backend

Accepted: every Catalog control declares `software_preferred` or
`native_required`. Finite click roles prefer the token-bound Extension pointer
sequence. Editable, select, and file-input controls retain real operating-system
input. Generic `web.act` asks the Registry to choose; explicit native/soft tools
remain diagnostic gates and cannot make a native-required control soft.

Accepted: the Runtime keeps a separate user-local `saccade.input-policy/1` log
keyed by normalized page path, semantic role, and safe control name. A verified
software receipt records software success. An accepted software dispatch with
an unverified or visibly unchanged postcondition records native for the next
fresh action. `TargetInvalidated` teaches nothing. The user or Agent can inspect
the log and remember a native exception for a current token. Profile remains
exactly `name / behavior / ban`. A diagnostic software request cannot bypass a
learned native exception.

Rejected: immediately retrying native input after a software dispatch. The page
may have performed an effect that the observation cannot represent, so a second
click could duplicate a consequential action. Learning never reuses the same
token. The log omits queries, fragments, credentials, values, locators,
coordinates, and protected data and cannot weaken a Catalog native requirement.

Development proof: paired managed run `20260730T002519Z` produced seven
ordinary software-verified and eight native-verified receipts in each browser.
A trusted-event-only fixture then returned an
unverified software receipt, wrote a page-local native rule, and verified the
next fresh token through real OS input without same-token fallback. The learned
rule also rejected an explicit diagnostic software request before preparation.
Catalog status remains `implementation`; these are local development results.
