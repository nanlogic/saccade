# Saccade final architecture

Status: accepted direction, 2026-07-27.

## Product objective

Saccade is an open closed-loop browser control runtime for authenticated tabs.
Agents connect through MCP. Contributors extend browser-control coverage
through a declarative Control Catalog, audited control-family modules,
conformance fixtures, and evidence.

## The single route

```text
Agent
  → MCP mode
  → owner-only local IPC
  → Native Host mode
  → Native Messaging
  → Chrome/Edge Extension
  → agent-owned or explicitly shared tab
```

There is no production CEF/Servo shell and no Playwright, CDP, screenshot,
vision, or page-script fallback. The Runtime executes only strategies compiled
into its Registry. The current v1 Registry contains no Agent-provided
direct-coordinate strategy.

## Product layers

### Runtime

One cross-platform executable supplies separate modes:

```text
saccade-runtime native-host
saccade-runtime mcp
saccade-runtime doctor
saccade-runtime repair
```

The modes share protocol, session, control-registry, verifier, audit, download,
and packaging libraries. They do not share stdin/stdout: Chrome owns the Native
Messaging channel and each Agent owns its MCP channel.

`saccade.system.capabilities` lists browser-owned confirmation dialogs as a
restricted surface. Chrome and Edge do not expose those dialogs to the page
Extension as revalidatable objects. Saccade requires human confirmation and
does not intercept `window.confirm`, synthesize an Enter key, or add a browser
chrome fallback.

The Runtime validates identity, revision, focus, topmost state, and geometry,
selects a registered input backend, waits for fresh observations, and records receipts.
It also loads the three-field Profile described in `PROFILE_ARCHITECTURE.md`.
The Profile supplies behavior text to the Agent and bans named controls from
the Agent surface. A separate user-local input-policy log records verified
per-page control experience; it is not Profile data and is never committed.

### Truth Layer

The Truth Layer is the evidence boundary. It defines semantic objects, runtime
identity, revision, provenance, affordances, target representations,
limitations, disclosure, and receipts. The current v1 schema uses opaque action
tokens and withholds locators, coordinates, editable values, and protected
values. The active Profile may remove named controls from this projection.

The Extension-to-Host evidence stream remains a complete
`saccade.observation/1` snapshot. MCP maintains a per-Agent Browser view over
that evidence: the first view for a document is complete, while later views
contain only appeared, updated, and disappeared semantic objects plus refreshed
opaque authorities. Navigation, a stream gap, or a sufficiently large layout or
topology change resets the view with one new complete projection. Full internal
snapshots remain available to verification and local evidence; they are not
repeated in Agent tool results. Exact document/viewport bounds, per-object
evidence revisions, and loop-class tokens stay inside that evidence boundary;
the Agent view retains semantic visibility and opaque object/action identity.

### Control SDK

The SDK contains:

- a machine-readable Control Catalog;
- control-family module contracts and registry;
- allowlisted native-input primitives;
- declarative postcondition verifiers;
- fixtures and Chrome/Edge conformance runners;
- evidence and public-matrix generators.

End users do not install the SDK. Initially, new modules enter releases only
through reviewed source contributions. Runtime-downloaded arbitrary code is
out of scope.

## Closed-loop contract

Every actionable control instance follows:

```text
discover
  → observe
  → prepare
  → revalidate
  → execute a registered input backend
  → reobserve
  → verify a control-specific postcondition
  → receipt | failed | limited
```

Each Catalog entry declares either `software_preferred` or `native_required`.
Software-preferred click controls use a token-bound Extension pointer sequence;
editable, select, and file-input controls require real OS input. A user-local
receipt-backed rule may strengthen one page/control from software to native.
An accepted input event is not automatically a successful control action.
For example, checkbox success requires a checked-state transition; link success
may require a document transition or an agent-owned new tab. If the semantic
postcondition cannot be proved, the receipt says delivered/unverified rather
than successful.

An Agent may submit a bounded form plan once. The Runtime resolves every
initial token to runtime object identity before the first side effect, then
executes the plan locally. Every item still performs its own prepare,
revalidate, registered input, reobserve, verifier, and receipt transaction
against a fresh revision. Later items are refreshed by the same document-local
object identity; the Agent does not re-read or re-plan the page between fields.
The result contains value-free step summaries and one final Agent-view delta.
Submit buttons and navigation remain separate actions.

## Control-module boundary

Each cataloged control declares its safe state, affordances, implementation
family, input policy, primitive, verifier, limitations, fixtures, and browser evidence.
Similar controls share code. Agents continue to use generic observe/act tools;
the Registry dispatches the correct module from the opaque token.

Modules cannot send arbitrary code to the Host. The Host exposes a finite set of
primitives such as pointer movement, allowed buttons, wheel, allowlisted key
chords, Unicode text, selection, and bounded drag. The `native` backend uses OS
input. The `soft` backend is restricted to the Registry's finite click roles
and dispatches from the Extension only after the same token and revision
revalidation. It accepts no Agent coordinate or locator. The current v1 route
uses opaque action tokens.

Modules own semantic interpretation and the control-specific closed loop. They
execute through registered primitives and report what occurred. Profile
filtering happens outside the modules and cannot change their native action or
verification logic.

### User-local input policy

The Catalog is the portable default; the user's runtime history is the local
exception layer. For a normalized HTTP(S) page path plus semantic role and safe
control name, the Runtime may record `software` after a verified software
receipt or `native` after an unverified/unchanged software receipt. Query,
fragment, credentials, editable values, locators, coordinates, and protected
values are never stored. The user or Agent may explicitly remember the stronger
native choice for a current token and may inspect the log through MCP.
An explicit software diagnostic cannot bypass a learned native rule.

Learning changes only the next fresh action. Saccade never turns an unverified
software dispatch into an immediate native retry because that could activate a
control twice. `TargetInvalidated` does not teach a backend preference. A
Catalog `native_required` entry cannot be weakened by local history. Managed
tests isolate and restore the user's log.

The Profile contains `name`, Agent-facing `behavior`, and a `ban` list. The
Runtime filters banned controls before MCP exposure and rejects action tokens
that are absent from the filtered current observation. The current
`saccade.observation/1` and `saccade-extension-host/1` meanings remain unchanged.

## Coverage target

- Common controls: full semantic observation, registered action,
  control-specific verification, fail-closed and redaction evidence, and
  current Chrome/Edge proof.
- Uncommon controls: at least truthful recognition, safe state/bounds, and an
  explicit limitation for every unverified action or semantic.
- Outside the 95% core: browser/OS chrome, arbitrary closed-shadow internals,
  PDF form internals, and arbitrary Canvas/WebGL/custom widgets without audited
  semantics remain opaque or restricted.

Canvas/WebGL support prioritizes application-provided semantic bridges. Pixel
or visual detectors, if later approved, produce short-lived candidates with
explicit provenance; a changed image is not a semantic success receipt.

A canvas remains opaque unless an audited semantic bridge supplies current DOM
objects with revalidatable identity. MouseAccuracy is one such narrow bridge:
only its current `.target:not(.hit)` object is actionable. Historical hit
effects remain non-actionable and score advancement, not canvas motion, proves
success.

Named images may carry the audited `data-saccade-image-identity` bridge. The
Extension projects that bounded page-authored identity as description on a
non-actionable image object. A fresh document can prove semantic image identity
without exposing a source URL, screenshot, or pixels. Pages without the bridge
receive no pixel-identity claim.

## Platform delivery

- macOS: signed and notarized DMG containing `Saccade.app`; CoreGraphics input;
  one Accessibility confirmation; automatic Native Messaging and MCP repair.
- Windows: signed Setup; `SendInput`; automatic Native Messaging registration,
  MCP configuration, diagnostics, and repair.
- Browser: one shared Extension source; store identifiers may differ between
  Chrome Web Store and Edge Add-ons.

Both platform assets, the Extension, Catalog, public matrix, source commit,
SBOM, SHA-256 values, and machine-readable release manifest share one release
version.

## First proof slice

The first Catalog + Registry + Runtime slice contains button, text field,
checkbox, and select. The SDK v1 module contract froze after all four passed
stale, permission, focus, topmost, redaction, native-input, postcondition,
receipt, Chrome, and Edge development gates on the same source candidate.
Catalog publication still requires signed-product and release evidence.

The next Catalog extension adds search field, textarea, contenteditable, and
spin button. These modules reuse the finite Unicode-text primitive and
`has_value` verifier but keep role-specific safe projections. Paired managed
Chrome and Edge development run `20260729T043308Z` verified all eight current
actionable controls. This extension changes neither Profile fields nor the two
v1 wire schemas; its Catalog rows remain `implementation` pending release
evidence.

The 2026-07-29 toggle and command extension adds radio, ARIA switch, tab, and
menu item without changing the native primitive boundary. Radio and switch
verify checked transitions, tab verifies becoming selected, and menu item v1
verifies an expanded transition. Managed Chrome run `20260729T192723Z` and
Edge run `20260729T192757Z` each produced 12 native verified receipts on the
same source candidate. These are development artifacts; Catalog release
evidence remains pending.

The select family also covers native select, ARIA listbox, and ARIA combobox
through enabled option-object identity and bounded indexed keyboard input.
Paired managed run `20260729T225249Z` produced 14 native verified receipts in
both Chrome and Edge and covered bounded structural reading, Profile behavior,
Profile bans, and stale-token rejection. This remains development evidence.

The automatic input-policy extension adds a Catalog default for all 15 controls
and a user-local exception log without changing either v1 wire schema. Paired
managed run `20260730T002519Z` produced seven software-verified click receipts
and eight native-verified receipts in each browser, including
link navigation. Each also proved that an unverified software dispatch teaches
native only for the next fresh token, rejects a diagnostic software bypass,
and then verifies the fresh token through OS input.
These are local development artifacts; Catalog status remains `implementation`.

Observation order is monotonic per tab. Revisions advance within a document;
when a new document identity is accepted, the Host retires the prior identity
and rejects any delayed snapshot from it. This prevents a late pre-navigation
message from replacing the current observation or contaminating a receipt.

Public-page compatibility is a separate development gate. Saccade must first
produce its own verified receipts through the Registry-selected backend. A
Playwright harness may then run in
fresh contexts as an out-of-band reference oracle for accessible name, state,
and screenshot comparison. It is absent from the production route and cannot
create or upgrade a Saccade receipt. Run `20260729T211221Z` matched radio,
switch, tab, and menu item on public W3C examples in Chrome and Edge.

The 2026-07-30 Selenium official `web-form.html` gate exercised the incremental
Agent Browser and local form plan on a public QA fixture. Three clean Chrome
runs completed one five-control plan plus a separate Submit with 18/18 verified
receipts and no editable-value disclosure. Median Saccade task time was 2.391
seconds and median model-facing output was 4,863 tokens, down from the preceding
six-call debug-shaped gate's 4.869 seconds and 63,093 tokens. The official
Playwright MCP reference received predeclared CSS selectors with snapshots
disabled and measured 1.327 seconds and 421 tokens. This proves the corrected
Saccade architecture and a large regression improvement; it does not claim
universal performance superiority. Evidence is local development evidence and
does not change Catalog publication status.

The managed ordinary native-mouse gate uses the same semantic button token,
preparation, CoreGraphics input, reobservation, and button-effect verifier. It
does not expose or accept Agent coordinates. Run `20260729T053405Z` verified
24/24 static targets in Chrome and 24/24 in Edge at 32, 40, and 48 CSS pixels.
The macOS adapter's HID event source and move/down/up timing were migrated from
the reviewed legacy human-input gate, not from its retired CEF/Servo execution
route.

DOM hit testing proves page-level topmost state. An unrelated always-on-top OS
window can still intercept an event after preparation; v1 reports the missing
semantic effect as unverified rather than claiming success. Release evidence
must therefore use a controlled unobstructed browser window. OS-window
occlusion preflight remains a separate platform gate. Development evidence also
moves and resizes the exact managed browser PID between phases so screen bounds
are recomputed rather than assumed from launch geometry.
Managed Chrome run `20260729T064702Z` passed 24/24 native targets with zero
misses across all three phases.

The audited reflex extension adds a `reflex_target` Catalog module and one
bounded local MCP loop. One MCP request keeps observe → act → verify local to
the Runtime hot path. `native` receipts require `AcceptedByOs`; `soft` receipts
require `AcceptedBySoftware`. Both require the same loop-class occurrence or
score to advance. Profile remains `name / behavior / ban` and cannot select or
weaken an input backend. This is additive development behavior under the v1
wire names; its Catalog row remains `implementation`.

Managed Chrome development run `20260729T064526Z` drove MouseAccuracy to
`Insane + Tiny` and produced 31 score-verified software hits with zero failures.
Observation-to-receipt latency was 14.72 ms p50 and 15.76 ms p95. This is local
development evidence, not publication evidence.

The next audited slice adds link navigation and single-file selection without
changing the Profile or v1 wire names. Link verification requires a new
document identity. File selection accepts an absolute accessible regular
non-symlink path only in the immediate MCP action payload, keeps that path out
of the Extension and every receipt/evidence surface, and drives the operating
system chooser through a finite primitive. A real file-input change verifies
chooser selection; server upload persistence remains a separate page-level
fact.

The 2026-07-29 authenticated itch.io dogfood selected a 37.8 MB Gear Up PDF
through the real macOS chooser with `AcceptedByOs + Verified` and produced no
path in the receipt. The collector then used bounded visible action-group text
to distinguish four repeated file rows. Saccade made the v2 PDF public, checked
the required confirmation for `gear_up_cards.pdf`, deleted that old file, and
loaded a fresh document containing only the rules PDF, LICENSE, and v2 PDF.
The same document preserved `Graphics=true` in the project's generative-AI
disclosure.

The same run routed `Replace Cover Image` and `Add screenshots` through the
file-input loop. Three screenshot selections returned `AcceptedByOs + Verified`;
a fresh document contained three screenshot rows. The cover upload invalidated
and replaced its chooser target, but the current Truth Layer withholds image
pixels and cannot prove the new cover's pixel identity. Screenshot deletion
also exposed an itch.io browser-owned confirmation dialog, which required a
human confirmation because browser chrome remains outside the v1 route.

The preceding Link click was accepted by the OS but its document transition
arrived after the receipt settlement window, so that receipt stayed
unverified. These are local Chrome development findings; both new Catalog rows
remain `implementation` pending same-candidate Chrome/Edge release evidence.

## Non-goals

- A second embedded browser product.
- One MCP tool per control.
- Arbitrary downloaded Host modules.
- Silent or unregistered execution strategies.
- Restoring retired engines to the default workspace.
- Calling a revision change alone a verified semantic action.
