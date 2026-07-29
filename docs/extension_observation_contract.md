# Saccade Truth Layer contract

This is the only production contract for browser authorization, observation,
action preparation, native input, receipts, downloads, and MCP exposure.

The current implementation covers eleven Registry controls: button, link,
text field, search field, textarea, contenteditable, spin button, checkbox,
select, reflex target, and file input, plus select-option observation. Other
roles in this contract define the intended Truth Layer surface. They are not
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
- MCP: public tool schemas and strict forwarding only.
- Agent: chooses only from disclosed objects, affordances, and opaque tokens.

Control-family modules own semantic interpretation, native execution,
reobservation, and control-specific verification. They do not read Profile
data. The Native Host applies Profile bans to the Agent projection and exposes
the Profile behavior through capabilities. The current v1 authorization,
token, revision, and protected-value behavior remains unchanged.

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

Navigation creates a new document identity and invalidates all earlier facts
and tokens. Object identity is runtime-only and held with `WeakMap`; it is not
a selector, stable locator, DOM path, or identifier the Agent can construct.

An observation is a claim about the Extension's current safe projection. It is
not a claim that canvas, WebGL, video, closed shadow roots, restricted frames,
or browser-owned documents have been semantically understood.

## Agent-facing object model

Every disclosed object has:

- runtime `object_id`, `object_revision`, and `frame_id`;
- broad `kind` and a more specific `role`;
- document bounds, optional viewport bounds, and visibility;
- zero or one safe `name` and `description`;
- zero or one visible-content `text` value;
- an allowlisted safe-state map;
- current affordances and transition hint;
- optional opaque action token;
- `protected`, indicating that a human-only value path is required.

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
execute it through the single native-input route. Unsupported controls remain
observable when useful, but are not made actionable by guessing.

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

MCP supplies only a current action token and a fixed operation payload. The
transaction is:

```text
authorized observation
  -> Agent action request
  -> Extension prepared action
  -> Host identity/revision/token/affordance revalidation
  -> registered input backend
  -> settled fresh observation
  -> action receipt
```

The Extension scrolls the target into view and prepares current screen geometry,
visibility, topmost hit-test state, and focus state. The Host rejects arbitrary
coordinates and unrestricted key sequences, rechecks the current browser
instance, tab, document, revision, token, and affordance, rejects replay, then
dispatches input. The default `native` backend uses OS input. The `soft` backend
is available only to an audited `reflex_target`; it computes the current target
center inside the Extension and never accepts or discloses an Agent coordinate
or locator.

A receipt binds before, prepared, and post-action revisions and includes the
post-action observation. `AcceptedByOs` means the operating system accepted the
input request. `AcceptedBySoftware` means the audited Extension software-pointer
dispatch was accepted. Neither status by itself proves the user's intended
business result.
A postcondition is verified only to the level explicitly represented by the
fresh observation.

Profiles cannot change those meanings. The Host checks that an action token
still occurs in its current Profile-filtered observation before asking the
Extension to prepare the action.

Under the v1 contract, browser-session end, tab ACL revocation,
browser-instance mismatch, cross-tab use, navigation, token replay, stale
revision, detached identity, unsupported affordance, hidden or covered target,
lost focus, uncertain geometry, stream gap, or ambiguous frame composition
fails closed. Profiles do not alter these closed-loop checks.

## Changes and waiting

Full snapshots are always valid. A change list is an optimization and must not
be required to reconstruct truth after a gap. After any gap, navigation, Host
restart, or missed revision, the next response is a full snapshot with no
deltas. The Agent may wait for a revision newer than a supplied revision; it
must not poll by inventing delay loops outside the bounded MCP wait.

DOM insertion, removal, safe attribute changes, visible text changes, scroll,
resize, focus, and form state changes schedule observation refresh. Content not
yet created is never invented. A trigger may declare
`deferred_content_possible`.

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
