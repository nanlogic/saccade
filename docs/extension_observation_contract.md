# Saccade Truth Layer contract

This is the only production contract for browser authorization, observation,
action preparation, native input, receipts, downloads, and MCP exposure.

The current implementation covers fifteen Registry controls: button, link,
text field, search field, textarea, contenteditable, spin button, checkbox,
radio, ARIA switch, native select, ARIA listbox/combobox, tab, menu item,
reflex target, and file input, plus option observation. ARIA choice controls
have implementation tests and paired managed Chrome/Edge development evidence.
Other roles in this contract define the intended
Truth Layer surface. They are not
implemented until the Catalog lists their module, fixtures, verifier, and
evidence status.

The normative wire schemas remain `saccade.observation/1` and
`saccade-extension-host/1`. "Truth Layer" names the behavior defined here; it
is not a third wire protocol.

Profiles are described in `PROFILE_ARCHITECTURE.md`. The Native Host uses the
active Profile to supply Agent behavior text and remove banned controls before
MCP exposure. Profile data does not change these v1 schemas or reinterpret
their fields.

## Purpose

The Truth Layer gives an Agent the smallest sufficient, revision-bound model
of the page a person can currently use. It is not a DOM export, accessibility
tree dump, screenshot interpretation, page database, or claim of complete
browser compositor access.

The Agent should be able to answer four questions without guessing:

1. What user-visible content and controls exist?
2. What can each control do now?
3. What state is safe to disclose?
4. What is missing, restricted, stale, or unverifiable?

DOM and accessibility metadata may describe an object. Current layout,
visibility, hit testing, focus, authorization, and revision state authorize an
action. A semantic match alone never authorizes native input.

## Topology and authority

Chrome or Edge runs the MV3 Extension. The browser launches
`com.nanlogic.saccade` through Native Messaging. The Host exposes a
per-session, owner-only local endpoint to MCP. No other perception or action
route is valid.

MCP uses the single `saccade_host_client` interface. Platform IPC selection is
internal to that client and cannot change tools, schemas, validation, or
behavior. MCP validates and forwards; it does not collect page state, resolve
targets, or dispatch input.

Authority is split deliberately:

- Extension: tab ACL, safe semantic projection, runtime object identity,
  revisioning, action preparation, and current-page hit testing.
- Host: session authority, request validation, token replay protection,
  last-moment revision checks, native input, settled receipts, bounded loops,
  audit metadata, and download verification.
- MCP: compact public tool schemas, Agent-view alias/envelope hydration, strict
  validation, and forwarding only. It does not resolve a page target or execute input.
- Agent: chooses only from disclosed objects, affordances, and opaque tokens.

Control-family modules own semantic interpretation, native execution,
reobservation, and control-specific verification. They do not read Profile
data. The Native Host applies Profile bans to the Agent projection and exposes
the Profile behavior through capabilities. The current v1 authorization,
token, revision, and protected-value behavior remains unchanged.

A control module may declare more than one finite operation strategy. The
current example is an ARIA select: a collapsed combobox may first advertise a
verified click-to-expand strategy, then expose option objects through the next
browser delta and accept the existing option-identity select strategy. Native
`<select>` does not advertise click-to-expand. Each operation still performs
its own complete closed loop with Catalog-declared primitives and verifiers.

## Browser authorization

The official Extension requests HTTP and HTTPS host access once in the
browser-controlled installation prompt. Browser permission is a technical
prerequisite, not the Agent disclosure boundary.

The disclosure boundary is a session-scoped tab ACL in
`chrome.storage.session`:

- A tab created by `tabs.open` is Agent-owned.
- A normal HTTP or HTTPS child tab opened by an Agent-owned tab inherits that
  ownership.
- Ownership survives navigation and redirects, and ends when the tab closes
  or the browser session ends.
- A pre-existing user tab is absent from `tabs.list` and cannot be observed
  until the user shares that exact tab from the Extension UI.
- User sharing survives navigation and ends on explicit revocation, tab close,
  or browser-session end.
- Sharing a user tab does not automatically share its child tabs.

Broad host permission must never enumerate, observe, or act on any other tab.
Internal browser pages, extension pages, unsupported schemes, and restricted
surfaces remain unavailable. Page content cannot add itself to the ACL.

## Observation envelope

An authorized top-level document emits `saccade.observation/1`. Every snapshot
binds:

- `browser_instance_id`, `tab_id`, and `document_id`;
- monotonically advancing `revision` and `viewport_revision`;
- observed and restricted frames;
- a compact list of disclosed objects;
- coverage, limitations, stream-gap state, and optional changes.

This complete snapshot is the Extension-to-Host evidence record. It is not
re-serialized wholesale for every Agent turn. The MCP process derives a
per-Agent `saccade.agent-view/1`: the first result for a document has
`mode=full`; later results have `mode=delta` and contain only appeared,
updated, and disappeared objects. Because v1 action tokens are refreshed with
the evidence revision, a delta may also carry an `authorities` list for
semantically unchanged actionable objects. Those opaque refreshes are not
semantic page changes.

`saccade.agent-view/1` may place common values in `object_defaults`. Matching
objects omit `frame_id`, `visibility=visible`, `transition=none`, and
`protected=false`; consumers apply the declared defaults while reconstructing
their Agent Browser. Non-default values remain on the object. The evidence-only
`kind` is omitted because the Agent `role` is the complete semantic type. This
is lossless response compaction over the unchanged Extension-to-Host snapshot.
Internal object identities are likewise projected as short, document-scoped
Agent aliases. Delta changes and authority refreshes use the same aliases. For
select, MCP resolves the chosen option alias to the internal identity before
the unchanged Host request is validated; stale or unknown aliases fail closed.

Navigation creates a new document identity and invalidates all earlier facts
and tokens. Object identity is runtime-only and held with `WeakMap`; it is not
a selector, stable locator, DOM path, or identifier the Agent can construct.

For one tab, the Host retires the previous document identity when a new one is
accepted. A delayed snapshot from a retired document cannot replace the current
snapshot, even if its per-document revision is numerically higher.

An observation is a claim about the Extension's current safe projection. It is
not a claim that canvas, WebGL, video, closed shadow roots, restricted frames,
or browser-owned documents have been semantically understood.

Browser-owned alert, confirm, permission, download, and chooser dialogs do not
become page objects. Capabilities mark browser-owned confirm dialogs as
restricted and require human confirmation. A page click that opens one remains
delivered/unverified until a later page observation proves the intended effect.

## Agent-facing object model

Every Host evidence object has:

- runtime `object_id`, `object_revision`, and `frame_id`;
- broad `kind` and a more specific `role`;
- document bounds, optional viewport bounds, and visibility;
- zero or one safe `name` and `description`;
- zero or one visible-content `text` value;
- an allowlisted safe-state map;
- current affordances and transition hint;
- optional opaque action token;
- `protected`, indicating that a human-only value path is required.

The derived Agent Browser object keeps `object_id`, `frame_id`, kind, role,
visibility, safe name/description/text, safe state, affordances, transition,
protection, and the current opaque action token. It omits `object_revision`,
document/viewport bounds, and loop-class tokens. Those fields remain local
revalidation evidence. The Agent acts through the opaque token and global view
revision; it cannot turn geometry into a coordinate action route.

The fields have distinct meanings:

- `name`: short page-authored identity, such as "Create account", "Email",
  or "Search". It is derived without reading a control value.
- `description`: short page-authored help or constraint text. It is never a
  substitute for a value.
- `text`: visible document content only. Controls, links, images, and fields
  use `name` instead of duplicating their label in `text`.
- `role`: what the object is for Agent reasoning.
- `affordances`: the only operations the Agent may request.
- `action_token`: opaque, single-use, document-and-revision-bound authority to
  request one advertised operation. It does not bypass Host revalidation.

Duplicate actionable controls with the same role and name may receive the
nearest bounded page-authored, non-control context as `description`. Context
extraction removes nested controls first and is disabled for protected or
non-actionable controls, so disambiguation cannot disclose their values.

Action tokens carry at least 128 bits of browser randomness. Browser, document,
and loop identities retain their independent longer entropy. Short Agent object
aliases are not authorities and cannot replace an action token.

The Agent never receives tag names, CSS selectors, XPath, DOM paths, event
handlers, arbitrary attributes, raw accessibility trees, or page-supplied
coordinates.

## Inclusion and compaction rules

The projection MUST include:

1. current actionable controls and links, including meaningful offscreen
   controls that can be scrolled into view;
2. disabled or otherwise unavailable controls when their existence explains
   the current workflow;
3. visible headings, paragraphs, list items, table cells, alerts, status
   messages, and other non-duplicated user-facing text;
4. meaningful images with a safe page-authored name;
5. frames, opaque surfaces, restricted documents, and matching limitations;
6. current choices belonging to a select control, without exposing submitted
   machine values.

The projection MUST omit:

- script, style, metadata, templates, hidden inputs, and browser bookkeeping;
- layout-only wrappers and duplicate ancestor text;
- unnamed decorative images and SVG containers;
- hidden or zero-size content that a person cannot currently use;
- control values, file paths, filenames from file inputs, selection ranges,
  clipboard data, cookies, storage, network payloads, and form submissions;
- page content outside the authorized tab.

After building this projection, the Native Host applies the active Profile's
`ban` list. It matches each rule's `control` against the full semantic name. A
rule without `condition` removes the matching control. A rule with `condition`
removes it only when the normalized semantic name and description contain the
condition. Matching folds case and whitespace. The Host also removes the
control's change entries, object limitations, and action token from the Agent
surface.

Offscreen is not hidden: an offscreen object may be disclosed and later
scrolled into view. Hidden, zero-size, detached, or non-rendered objects receive
no action token.

The Extension caps objects, frames, and total disclosed text. Reaching a cap
sets `coverage.truncated=true` and emits a `truncated` limitation. It never
silently presents a partial snapshot as complete.

## Safe semantic derivation

For controls, links, and images, `name` is derived in this order from safe,
page-authored sources:

1. `aria-label`;
2. visible text referenced by `aria-labelledby`;
3. associated HTML label text;
4. visible control text or image `alt`;
5. `title` when no stronger name exists.

`description` may use visible text referenced by `aria-describedby`, then a
page-authored placeholder or title that was not already used as the name.
When two or more buttons or links have the same generic name, `description`
may instead contain a short, visible, non-editable label from the nearest
bounded action group. This disambiguates repeated actions such as file-row
management without exposing a locator or reading any input value. Local file
input names and paths remain forbidden; a server-rendered public upload name is
ordinary visible page content.

Derivation MUST NOT read `value`, `defaultValue`, selected text from an editable
control, password-manager state, or editable `textContent`. Accessible metadata
is descriptive evidence, not proof of visibility or actionability.

For a visible `role=dialog` or `aria-modal=true` container, its visible
page-authored accessible name is projected as a heading. This does not export
the dialog subtree, input values, or a new wire role. An unlabeled dialog does
not receive a guessed title.

For an explicit ARIA widget role, that role takes precedence over a native
anchor fallback. Visible-text name fallback excludes descendants marked
`aria-hidden=true`; state words hidden from accessibility remain state, not part
of the control name.

Names and descriptions are whitespace-normalized and length-bounded. A
control may remain unnamed; the Agent must not invent a label from geometry or
neighbor proximity.

## Control truth surface

The following table is the normative v1 Agent surface. "State" lists the only
control-specific state that may be disclosed in addition to common geometry,
visibility, transition, and authorization fields.

| Page object | Agent `role` | Safe disclosure | State | Affordances |
| --- | --- | --- | --- | --- |
| Button, submit, reset, ARIA button | `button` | name, description | enabled, pressed, expanded | click, hover, focus |
| Link | `link` | name, description; no destination secret or query data | current, expanded | click, hover, focus |
| Text-like input | `text_field` or `search_field` | name, description; never contents | has_value, enabled, required, readonly, invalid | click, focus, type |
| Password, OTP, payment-secret input | matching field role with `protected=true` | safe name only; never contents or dynamic description | has_value, enabled, required, readonly, invalid | editable token is accepted only by human-only protected fill; direct Agent type fails |
| Textarea | `text_area` | name, description; never contents | has_value, enabled, required, readonly, invalid | click, focus, type |
| Contenteditable | `content_editable` | safe external name; never editable contents | has_value, readonly | click, focus, type |
| Checkbox | `checkbox` | name, description | checked, enabled, required, invalid | click, hover, focus |
| Radio | `radio` | name, description | checked, enabled, required, invalid | click, hover, focus |
| ARIA switch | `switch` | name, description | checked, enabled | click, hover, focus |
| Select/combobox | `select` | name, description | has_value, enabled, required, invalid, expanded | click, focus, select |
| Option | `option` | page-authored option name, never submitted `value` | selected, enabled | none; selected through its owning select token |
| File input or unambiguous visible file-chooser trigger | `file_input` | name; never local path or filename | has_value, enabled, required | upload through the dedicated native chooser flow |
| Range/slider | `slider` | name, description; no current numeric value in v1 | enabled, required | focus; unsupported manipulation is explicit |
| Number/spin button | `spin_button` | name, description; never contents | has_value, enabled, required, readonly, invalid | click, focus, type |
| Tab/menu item | `tab` or `menu_item` | name, description | selected or expanded, enabled | click, hover, focus |
| Associated label | `label` | name | none | click only when bound to a current control |
| Generic audited click target | `generic_control` | safe name when available | enabled | click, hover, focus |
| Reflex target | `reflex_target` | safe name when available | reflex_target, reflex_occurrence | click, hover |
| Heading | `heading` | visible text | level | none |
| Paragraph/list item/table cell | matching structural role | visible text | none | none |
| Alert/status | `alert` or `status` | visible text | busy when declared | none |
| Image/SVG | `image` | page-authored name only; no pixels | none | none unless it is independently a control |
| Same-origin frame | `frame` | frame name when safe | frame status | no direct frame click; descendants carry actions |
| Cross-origin/restricted frame | `frame` | no contents | restricted status | none; emit limitation |
| Canvas/WebGL/video | `opaque_surface` | safe external name only | none | none; emit matching limitation |
| Built-in PDF | `restricted_document` | document presence and bounds only | restricted status | explicit confirmed download/open flow only |

An affordance is omitted unless the current implementation can validate and
execute it through the single registered-input route. Unsupported controls remain
observable when useful, but are not made actionable by guessing.

An image with a safe name may opt into the audited
`data-saccade-image-identity` bridge. Saccade exposes the bounded page-authored
identity as `description` prefixed by `Semantic identity:`. This proves the
application's semantic declaration. It does not hash, inspect, describe, or
compare pixels and does not expose `src` or `currentSrc`.

## Safe state allowlist

The v1 state map may contain only:

`has_value`, `checked`, `enabled`, `selected`, `expanded`, `required`,
`readonly`, `pressed`, `current`, `invalid`, `busy`, `modal`, `level`,
`reflex_target`, and `reflex_occurrence`.

Boolean state uses the strings `true` and `false`. Enumerated ARIA states use
their normalized public token. `level` is a bounded positive integer string.
No extension or adapter may introduce an unreviewed state key. Keys containing
`value`, `text`, `raw`, `password`, `otp`, `secret`, or `content` are forbidden,
except the boolean key `has_value`.

`has_value` reveals only whether a field is empty. It never reveals length,
format, prefix, suffix, validation message containing the value, or source.

## Protected values

Editable controls never expose their contents in observations, changes,
receipts, logs, diagnostics, or artifacts. Values intentionally supplied by an
Agent may exist only in the immediate fixed action payload required to type
them. A file-selection path follows the same immediate-payload boundary: it is
validated as an absolute accessible regular non-symlink file, consumed by the
finite native chooser primitive, and omitted from Extension messages,
observations, receipts, logs, diagnostics, and evidence.

Passwords, one-time codes, payment secrets, and other locally protected values
must use the Extension's human-only protected-value UI. The value travels
directly to the Host input path and never enters MCP, an observation, a receipt,
or an audit record. The Agent sees only a safe field name, `protected=true`, and
allowlisted boolean state such as `has_value`; dynamic descriptions are omitted.

## Action transaction

MCP supplies a current action token and fixed operation fields. The adapter may
resolve that token only inside the current views already emitted to that Agent,
then locally restores the complete browser, tab, document, and basis-revision
envelope. It cannot invent or refresh authority. An absent, ambiguous, stale,
or cross-document token set fails before Host forwarding. The Host independently
validates the complete hydrated request. The
transaction is:

```text
authorized observation
  -> Agent action request
  -> Extension prepared action
  -> Host identity/revision/token/affordance revalidation
  -> Registry-selected input backend
  -> settled fresh observation
  -> action receipt
```

The Extension scrolls the target into view and prepares current screen geometry,
visibility, topmost hit-test state, and focus state. The Host rejects arbitrary
coordinates and unrestricted key sequences, rechecks the current browser
instance, tab, document, revision, token, and affordance, rejects replay, then
dispatches input. The Catalog marks each control `software_preferred` or
`native_required`. The `native` backend uses OS input. The `soft` backend is
available only to finite Registry click and option-selection roles; click computes the current target
center inside the Extension and never accepts or discloses an Agent coordinate
or locator. Selection revalidates the owning control and opaque option identity,
then uses a bounded native-select or ARIA key sequence. The page collector, not the service worker's observation cache, is
the authority for the final document, revision, token, and target revalidation.
Normal MCP clients receive only `web.act`, and the Registry selects the backend;
backend choice is not an Agent planning decision. Explicit soft/native action
tools and the reflex-loop backend selector are available only under the local
development diagnostic flag and otherwise fail before Host dispatch.

A Host receipt binds before, prepared, and post-action revisions and includes
the complete post-action observation for verification and local evidence. The
MCP `saccade.agent-receipt/1` exposes the receipt status and the derived
Agent-view delta instead of repeating that snapshot. `AcceptedByOs` means the operating system accepted the
input request. `AcceptedBySoftware` means the audited Extension software
dispatch was accepted. Neither status by itself proves the user's intended
business result.
A postcondition is verified only to the level explicitly represented by the
fresh observation.
For a button whose observation declares `deferred_content_possible`, a newly
appeared visible heading, alert, or status is a verified semantic effect. Form
submit buttons, `aria-haspopup=dialog`, and `aria-controls` may declare this
transition. Unrelated object churn, new table cells alone, or input acceptance
does not verify the button.

Profiles cannot change those meanings. The Host checks that an action token
still occurs in its current Profile-filtered observation before asking the
Extension to prepare the action.

The Runtime also maintains a separate user-local `saccade.input-policy/1` log.
Rules are keyed by normalized page path, semantic role, and safe control name.
A verified software receipt records that software worked; an unverified or
visibly unchanged software receipt records that a future fresh action should
use native input. There is no same-action fallback or token reuse. A user or
Agent can remember native input for a current software-preferred control. The
log stores no query, fragment, credentials, editable value, protected value,
locator, or coordinate. It cannot weaken `native_required`, and a diagnostic
software override cannot bypass a learned native rule.

Under the v1 contract, browser-session end, tab ACL revocation,
browser-instance mismatch, cross-tab use, navigation, token replay, stale
revision, detached identity, unsupported affordance, hidden or covered target,
lost focus, uncertain geometry, stream gap, or ambiguous frame composition
fails closed. Profiles do not alter these closed-loop checks.

## Changes and waiting

Full Extension-to-Host snapshots are always valid. MCP retains the last full
snapshot for each tab in that Agent process and computes semantic changes after
Profile filtering. It ignores action-token, loop-token, object-revision, and
geometry-only rotation when deciding whether a human-visible object changed.
Visibility and semantic responsive-layout changes remain observable. After any gap,
navigation, MCP restart, missed base, or a sufficiently large change set, the
next Agent response is `mode=full`. Otherwise it is `mode=delta`. A client can
reconstruct the current Agent Browser by applying `changes` and then opaque
`authorities` to its previous view.

`tabs.open` does not return success until the collector has produced the first
authorized observation. Dynamic content may legitimately arrive after that
first snapshot. `web.observe` therefore accepts `after_revision` plus a bounded
`timeout_ms`: the Runtime waits on the browser-pushed observation stream and
returns only after a newer revision exists. Agents and clients must use this
local wait instead of polling unchanged truth through repeated model tool
calls.
When `after_revision` is absent, observe returns the current Agent view
immediately; a supplied `timeout_ms` is ignored by the MCP adapter rather than
turning a harmless read into a failed tool call.

The Extension injects and configures the collector once an authorized HTTP(S)
document has committed and is loading. It MUST NOT require browser
`status=complete`, because third-party resources may remain pending indefinitely.
Concurrent load/update notifications are deduplicated; navigation still clears
the old session before the new document is authorized. `collect()` withholds
the first actionable observation while `readyState=loading` and publishes it at
`DOMContentLoaded`; later resources arrive through ordinary deltas.

DOM insertion, removal, safe attribute changes, visible text changes, scroll,
resize, focus, and form state changes schedule observation refresh. Content not
yet created is never invented. A trigger may declare
`deferred_content_possible`.
CSS `transitionend` and `animationend` also schedule refresh. An inserted dialog
at opacity zero remains hidden evidence; only the post-transition observation
may disclose its now-visible title. Deferred-content action settlement is
bounded to 750 ms and still requires the ordinary semantic verifier.

## Local form plan

`saccade.web.form.fill` accepts between one and 32 current control-token
operations. MCP proves that every token occurs in one current Agent-view
document revision and hydrates that envelope as described above. The allowed plan surface is
text-like editable `type`, select-by-option-object identity, and the explicit
`check` intent. `check` maps to the existing click transaction only after the
Runtime proves the target is a checkbox, radio, or switch. Protected controls,
file inputs, submit buttons,
navigation, repeated targets, and arbitrary operations are rejected before the
first side effect.

The Host resolves all initial tokens to runtime object identities, then runs
each control through the ordinary Registry-selected closed loop. After each
verified step it obtains the next fresh observation and refreshes the remaining
target by the same document-local object identity, role, and safe name. A
disappeared, renamed, retyped, protected, unverified, navigated, or otherwise
invalid target stops the plan. There is no rollback and no same-action backend
fallback. The MCP result contains value-free step summaries and one final
Agent-view update; immediate editable payloads do not enter receipts or views.
Step order, role, dispatch status, postcondition, and settlement remain visible;
the result does not echo the request's control name or operation.

## Local reflex loop

The one audited local-loop exception begins from a current reflex-target action
token. It is fixed to one browser instance, tab, document, operation, and
audited target class. `saccade.web.reflex.run` keeps repeated observation and
action transactions local after one MCP request. Each occurrence still receives
a fresh token and is prepared and revalidated before either registered backend
dispatches it.

The Host rejects repeated occurrences, stale revisions, changed identities,
hidden targets, uncertain geometry, or permission loss. A stale target is
reobserved; it is never replayed. The loop is bounded by the MCP schema (currently
60 seconds and 10,000 requested actions). Reports contain count, failures,
stale retries, backend, causal occurrence transitions, and p50/p95/max
observation-to-receipt latency.

MouseAccuracy supplies an audited DOM semantic bridge over its canvas game.
Only `.target:not(.hit)` is actionable. Its safe `reflex_occurrence` is the
visible score, and verification requires that score to advance within the same
loop class. A non-actionable loop-status object carries that score so a receipt
does not wait for the next target to spawn. Target geometry, disappearance,
animation, or a revision change alone cannot verify a hit. Arbitrary
Canvas/WebGL remains opaque.

This loop is a bounded implementation feature, not a general page-script,
selector, coordinate, or detector protocol.

## Frames and opaque surfaces

Safely composable same-origin frames contribute normal descendants with frame
identity. Frames that cannot be safely composed report `restricted_frame` or
`ambiguous_frame_transform`. Open shadow roots contribute normal descendants.
A closed shadow root is never traversed; until an early browser-side hook can
reliably establish its presence, current coverage must not claim that it always
emits `closed_shadow_root`. Canvas, WebGL, and video report opaque surfaces. The
built-in PDF viewer reports a restricted document.

Missing access is a limitation, never silent completeness. The Agent must not
infer controls inside an opaque or restricted surface from surrounding text.

## Downloads and PDF

Downloads are bound to an authorized tab and current object token and use the
browser download manager. A top-level PDF requires explicit local confirmation.
The Host accepts completed files only inside the owner Downloads directory,
records size and SHA-256, and may ask the operating system to open the file.
File contents are not exposed to MCP. PDF parsing and filling are outside v1.

File selection is not a download route. A cataloged `file_input` may accept one
Agent-supplied local file through the native operating-system chooser. A
verified `has_file` postcondition means a real file-input change accepted a
non-empty selection. It does not by itself claim that a remote server finished
receiving or persisting the file; that requires a separate current page effect
such as a new file row surviving a fresh server-loaded document. Visible
buttons that create an ephemeral file input are eligible only when their safe
name unambiguously describes choosing or uploading a file, and verification
still requires the real input change event.

## Conformance and release gate

Rust types and canonical fixtures in `crates/saccade_protocol` and
`tests/protocol` are the wire-format source of truth. The Extension, Host, MCP,
and fixtures must agree on every required field, enum, limit, and rejection.

Conformance must prove at least:

- compact control roles and safe names are emitted without control values;
- hidden, zero-size, stale, covered, detached, or unauthorized targets cannot
  produce successful native actions;
- protected values never cross the MCP or observation boundary;
- navigation and revision changes invalidate earlier tokens;
- tokens are single-use and cross-tab/browser reuse fails;
- frame, opaque-surface, truncation, and stream gaps are explicit;
- receipts contain the settled post-action observation;
- the ordinary native mouse gate verifies zero misses across standard target
  sizes and horizontal positions in a controlled unobstructed browser window;
- unknown fields, roles, states, operations, and protocol versions fail closed.

Extension `elementFromPoint` establishes DOM-level topmost state. The v1 wire
format does not carry OS-window ownership, so an always-on-top desktop overlay
cannot be preflighted by the Extension. If one intercepts native input, the
required semantic postcondition remains unverified; it is never reported as a
successful action.

The development manifest carries a fixed public key. Official Host manifests
allow only approved development/store Extension identities. Consumer macOS is
a signed, notarized, stapled DMG containing `Saccade.app`; PKG is enterprise
only. Windows Setup and binaries require Authenticode signing and native-host
registration. Source commit, SBOM, release manifest, and artifact hashes
accompany a release. A platform is not supported until its native integration
and clean-machine installation matrices pass.
