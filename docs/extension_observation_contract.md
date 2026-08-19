# Extension Truth Layer contract

Status: normative for `saccade.observation/1`.

## Boundary

The authorized Extension is the only webpage compiler. It continuously reads
browser-visible semantic state and sends complete current evidence plus
source-computed changes through the single Native Messaging route. The Host
stores and forwards that truth; MCP compacts and aliases it. Neither Host nor
Agent reparses HTML or diffs snapshots to discover meaning.

The collector stays dormant until the tab ACL authorizes the document. A
long-lived Extension Port carries observations. Navigation, reconnect,
document replacement, or a revision gap resets the stream and requires a new
full view.

The Native Messaging `hello` carries the live Extension's content-addressed
candidate identity and a bounded lifecycle wake description: `chrome` or
`edge`, a development boolean, and that Extension's own `popup.html` URL. The
Host rejects any other browser family, scheme, Extension-ID form, or internal
path before persisting it owner-only. When the Host has an installer-pinned expected identity,
it rejects a missing or different identity and remains unready. The Service
Worker likewise rejects an authorized page Collector whose loaded candidate
does not match its own. Replacing files on disk is therefore not activation
evidence, and ordinary browser restart is not accepted as unpacked-Extension
activation evidence. A pre-handshake development Worker requires one explicit
Chrome Extensions Reload; later candidates self-reload on reconnect. This adds
bounded identity fields without changing
`saccade-extension-host/1` or the observation schema.

An already-open page can retain a static Collector from the previous candidate
after the Worker updates. Background reconnect reports that stale state without
refreshing user pages. When the user explicitly shares that exact tab, the
Extension may perform one ordinary tab reload, wait boundedly for the current
Collector, and then finish authorization automatically. It does not inject a
script, bypass the candidate check, or create another browser route.

## Object projection

A projected object may expose:

- stable document-local object identity;
- role and accessible name;
- safe role-specific state;
- semantic affordances;
- for a current link, an optional resolved HTTP(S) `navigation_target`;
- current document- and viewport-relative geometry;
- frame and semantic provenance;
- truthful limitations.

Every emitted object includes `document_bounds`; rendered objects also include
`viewport_bounds`. Both are CSS-pixel rectangles in the object's own frame:
the former is relative to that frame document and the latter to that frame's
current viewport. Stable identity never depends on either rectangle. The
Extension emits an `updated` change when a current object's position or size
changes, including scroll-, resize-, layout-, transition-, and
animation-driven movement.

It must not expose locators, DOM paths, editable contents, protected values,
cookies, browser storage, or arbitrary-coordinate action authority. The
Extension's protected content set is deliberately narrow: password, SSN, and
EIN fields, plus SSN/EIN-shaped text values masked before emission. Protected
objects retain geometry and value-free state. Default MCP additionally removes
optional action tokens and internal authorities, but preserves geometry.
Profile bans are applied before projection and cannot alter recognition
semantics; `PROFILE_ARCHITECTURE.md` remains normative for that boundary.

`navigation_target` is semantic page state, not a locator or execution token.
The Collector resolves it against the document base URL, emits only HTTP(S),
rejects embedded URL credentials, and bounds it to the same 8192-byte
navigation limit as `tabs.open`. It is
legal only on `role: link` with `transition: navigation_possible`. A changed
link target updates the same stable object. Unsupported schemes remain absent.

Control modules are indivisible semantic modules: each recognizes one control
family and consistently projects its role, name, safe state, affordances, and
limitations across supported native HTML, ARIA, and framework lifecycles. A
finite affordance may be consumed by `saccade.act`, but selectors, coordinates,
editable values, and protected values never enter Truth. Runtime binds the
action to current object, document, and revision identity; Extension accepts
only the registered software primitive.
MCP normally compiles a sole current `click`, `type`, or `select` affordance
directly from that object. A supplied text payload implies `type`, and an
`option_object_id` implies `select`; only a genuinely ambiguous object needs an
explicit operation. This is a Runtime projection rule and does not add fields,
authority, or bytes to the Extension observation.

## Full and delta views

Authorization/configuration eagerly schedules collection. The first
Extension→Host message for a document is a complete Snapshot. Later Native
Messaging messages carry only Extension-compiled `appeared`, `updated`, and
`disappeared` identities, complete current values for appeared/updated objects,
and refreshed opaque authorities for unchanged actionable objects, together
with document, viewport, and semantic revisions. The Service Worker retains
only readiness/document/revision metadata, not a second full page copy. Stable
aliases remain stable within one
document. Dynamic replacement receives new internal identity and is reported
as disappearance plus appearance; it is never silently treated as the old
object.

The Host keeps one current full observation and at most 256 compact journal
entries containing revision metadata and source-declared changed identities;
it does not retain 256 full pages. `truth.read(after_revision)` waits locally
and folds only source-declared changes after that revision. If history cannot
prove continuity, the Host discards that tab's materialized state and requests
one complete Snapshot from the Collector for that exact tab. Deltas for that
tab are ignored until the reset arrives. Other tabs are unaffected. Resource subscribers receive only an
updated URI notification and then read the same full/delta stream; notifications
do not repeat the page.

The Agent may choose `live` or `economy` delivery on each MCP Truth read. This
choice is downstream of the Extension and never changes collection semantics:
`live` exposes the next push immediately, while `economy` lets MCP coalesce a
bounded 150 ms burst and return the latest folded delta. Both preserve the same
objects, safe state, current geometry, identities, gaps, and source revisions.
The product does not force a mode or encode model/vendor policy into either one.

Downstream MCP exposes one automatic cursor rather than model-selected views.
An initial read may include a bounded semantic query over labels, roles,
affordances, visibility, and root/all frame scope. Runtime returns a
`working_set` of at most 32 stable objects plus frame summaries and keeps the
complete observation locally. The Extension still emits the same complete
Snapshot and deltas; it does not run the query or filter collection.
Runtime may wait through a bounded hydration interval for `min_objects` and may
acknowledge older queued ambient pages when a new working set is projected from
the latest canonical observation. `visible_only: false` includes rendered
offscreen objects but excludes hidden and unknown objects. None of these rules
changes Extension collection or either wire schema.
For a dynamic follow-up, the query may include the preceding action receipt's
`after_revision`. MCP treats that revision as a lower bound on canonical Truth:
it projects immediately if its exact-tab cursor already holds that revision or
newer, and otherwise performs one local blocking wait before projection. This
keeps the query revision-bound without transferring an intermediate delta or
forcing a second unbounded query.
Text query words are conjunctive over safe projected name, text, and
description fields. For a control, MCP may also match the bounded nearest
preceding headings already present in canonical Truth. This association is
computed locally from stable frame identity and document geometry; no
page-side query or selector is introduced.
An MCP-only `text_any` query may provide several such phrases. Each phrase is
conjunctive and phrases are alternatives. Runtime returns immediately after the
declared `min_objects` match count is reached, so unrelated continuing page
churn does not turn hydration into a page-idle wait. The complete Extension and
Host Truth remains unchanged and local.
If the count is not met before the bounded hydration timeout, Runtime returns
`settled:false`; a slowly hydrated control is not discarded merely because the
page had a short quiet interval.
ASCII query words use word boundaries (`Male` does not match `Female`), while
punctuation-bearing and non-ASCII terms retain substring matching.
The first read of a bounded document is full. If that full projection exceeds
the response budget, Runtime automatically returns a compact catalog covering
every projected semantic object by stable `object_id`, role, bounded label
preview, affordances, and visibility. The Agent may dereference up to 64
relevant identities against that exact `document_id` and `basis_revision`;
details do not advance the cursor. This is an MCP delivery projection only:
the Extension and Host retain canonical complete Truth. Subsequent ordinary
reads are deltas from the last view delivered to that Agent session. Document
replacement, a stream gap, or an unavailable base forces a new full-or-catalog
reset. An Agent whose own cache is wrong may call
`truth.read({tab_id, resync:true})`; this resets only that Agent/tab delivery
cursor. The API always requires `tab_id` and has no all-tabs Truth or resync
operation.
Browser-lifetime ACL cleanup is completed before the first Native Host hello,
using session-scoped Extension storage to distinguish a browser restart from a
Service Worker reload. A delayed browser `onStartup` notification therefore
cannot revoke authority granted after Host readiness.
`saccade.act` folds its post-action observation through the same cursor. Its
inline transition is action-scoped: target verification is compact, same-frame
structural appearance/disappearance is returned, and unrelated updates or
frame metadata are queued for the ordinary Truth cursor. The Extension
continues to compile complete current Truth;
Before dispatch, Runtime may rebase an object-addressed request across
same-document revisions only when the retained source journal proves neither
the target nor selected option changed. Missing history, document replacement,
or any target change remains stale and fails closed. The Extension independently
requires the opaque action token to remain current; it reuses that token across
geometry-only or unrelated page changes only when the target's non-geometry
semantic contract is identical.
after the initial Snapshot it transports only the compiled delta. This delivery
rule changes neither Profile filtering, object identity,
geometry, nor canonical observation semantics.

MCP compacts each `updated` Agent change as a recursive JSON merge patch over
the prior Agent object. A `null` patch value removes a field. `appeared` still
carries the complete projected object and `disappeared` carries its stable ID.
This downstream compaction does not change the Extension's source delta or the
Host's canonical materialized observation.

The client-owned MCP adapter may start while the Native Host is temporarily
absent. It keeps its process alive, rereads the owner grant for each bounded
Host call, and reconnects after socket or capability rotation. Only unavailable
transport is retried; authorization and protocol failures fail closed. This
lifecycle recovery does not create a second browser route or cached page truth.
On macOS, `tabs.open` may wake a disconnected zero-window browser by opening
only the validated Extension `popup.html` in the recorded Chrome/Edge family.
The wake surface accepts no target URL or page action; after reconnect, the
HTTP(S) request is still sent through Extension → Native Host → owner-only IPC.
MCP tool metadata and initialization instructions identify Saccade as the
primary navigation and page-reading route, including for clients with deferred
tool discovery. If the registered route stays unavailable after bounded retry,
the Agent reports the blocker rather than substituting another browser.

## Tab ownership and cleanup

The Extension is authoritative for tab ownership. `tabs.list` marks authorized
tabs as `agent` when they were created by `tabs.open` or claimed by an Agent
client, or `user_shared` when the user explicitly shared an existing tab. The
`provenance` field distinguishes `saccade_tabs_open`, `agent_client`, and
`user_shared`. `tabs.close` accepts only an
Agent-owned tab identity. A request targeting a user-shared, user-owned,
unknown, or already-closed tab fails without closing anything.

Each MCP adapter additionally projects only Agent-owned tabs created or claimed
by that MCP process plus current `user_shared` tabs. Other concurrent MCP
sessions' Agent tabs are omitted and cannot be read, acted on, or closed through
that adapter. This is downstream task isolation; it does not alter the
Extension's browser-session ACL or either wire schema.

Authorization never propagates through `openerTabId`. A user- or page-created
child of an Agent-owned tab remains Agent Off until that exact child tab is
created through `tabs.open`, confirmed by a provisioned claim, or explicitly
shared by the user.

### Provisioned Agent-client tab claim

Some Agent clients can act only in tabs they created themselves. For those,
`tabs.open` accepts `claim: "arm"` and `claim: "confirm"` as modes of the same
tool; the claim adds no additional public tool and no protocol version change.

`claim: "arm"` takes only the target URL. The Extension stores one session-only
intent in Service Worker memory holding a fresh single-use `claim_id`, the
normalized origin, a 30 second expiry, and no tab identity. It is never
persisted, so a replaced worker or an ended Native Host session forces a re-arm.
Arming creates no tab, queries no tab, reads no tab, and authorizes no tab.

While a claim is armed, the Extension inspects only the `tabs.onCreated` and
`tabs.onUpdated` event payloads for tabs created after arming. A tab that is
already authorized is never a candidate, and a pre-existing tab can never be
latched. The first candidate whose settled URL is an HTTP(S) URL on the armed
origin is latched; a candidate that settles on any other origin is decided once
and dropped permanently. Latching authorizes nothing — it only records which
single tab a later confirm may name. Once one tab is latched, no second
candidate is considered.

`claim: "confirm"` carries the `claim_id`, the target URL, and the exact
`tab_id` the Agent client obtained from its own tooling. The claim is consumed
on every confirm attempt. Authorization requires all of: an unexpired claim, a
latched tab, a matching `claim_id`, a `tab_id` equal to the latched identity, a
requested origin equal to the armed origin, and a live tab still on that origin
and not user-shared. Any failure returns the single message `tab claim could not
be confirmed`, so a caller cannot use confirm as a tab-identity oracle or probe
claim state. On success the tab is recorded Agent-owned with
`provenance: agent_client`, the Collector is configured exactly as for any
authorized tab, and Truth flows normally.

A claimed tab is revoked on Stop sharing in the popup, `tabs.close`, tab
removal, Native Host session disconnect, and browser startup. Only claimed tabs
are revoked on Host disconnect; `user_shared` and `saccade_tabs_open` ownership
is unchanged by the claim in every respect. The claim adds no click, type, or
execute capability, and uses one generic Chrome/Edge codepath.

`tabs.open` does not depend on Chromium's implicit "current window" state. It
opens in an explicitly selected ordinary window, preferring the focused one.
When the connected browser has no ordinary window, the Extension creates one,
opens the requested URL, and records the resulting tab as Agent-owned before
replying. Browser-without-window is recoverable lifecycle state, not a reason
to request a restart or use another browser route. Chromium on macOS may
terminate the Native Host after the final normal window closes; a subsequent
`tabs.open` uses the fixed Extension wake surface described above, waits for
the same route to reconnect, and then creates the requested tab. If cleanup closes the only
tab in that recovered window, revocation and the close response are committed
before Chromium tears down the window. A named MV3 alarm supports transient
reconnects while the worker remains schedulable, but is not treated as a cold
zero-window wake guarantee.

The ACL survives Service Worker replacement, development Reload, and Extension
update so ownership cannot disappear while the browser session and its tabs
continue. It is stored locally only as tab identities and provenance, then
cleared on the next browser startup before collection reconnects. Thus access
remains browser-session-scoped without treating a worker lifecycle as the
browser-session boundary.

Closing an Agent-owned tab removes its ACL entry and observation session; the
Host also discards its retained current view and history. This is bounded
session cleanup, not permission to execute inside a webpage or to close
arbitrary browser tabs. Agents should close temporary research tabs at task
completion while retaining user-facing results, unfinished work, and any tab
the user asked to keep.

## Structure and visibility

The top collector composes accessible same-origin iframe documents and open
shadow roots. Descendants retain frame or shadow provenance. Cross-origin or
otherwise inaccessible frames and closed shadow roots are reported as limited
or opaque rather than guessed.

Visibility follows rendered semantic availability, including lifecycle events
that finish transitions or animations. Mutation, relevant attribute, viewport,
focus, frame, and registered semantic-bridge changes schedule compilation.
DOM/ARIA semantic signals are microtask-batched and do not wait for a rendering
frame; scroll, resize, layout, transition, and animation geometry is
frame-bounded. Resize observation and active rendered-motion tracking keep the
Host's current object bounds fresh. The Agent client folds geometry deltas into
its cached view; omitted objects are unchanged.
Visible leaf text in generic layout containers is projected as bounded `text`
objects when it is not inside an editable control, named image, existing
structural object, or dialog projection. This covers rendered scorecards and
result metrics without adding site-specific selectors or exposing editable
contents. High-frequency reflex bridges may frame-bound their mutation batches
while preserving each semantic target or score transition.
Canvas/WebGL surfaces remain opaque unless an approved bridge supplies stable,
revalidatable semantic objects and changes.

## Software-first execution and external handoff

`saccade.act` is the preferred path for Registry-approved click, select, and
type. Preparation and dispatch are document-, revision-, token-, and
affordance-bound. Software preparation may defer scrolling until the
dispatch pass so its own geometry observation cannot stale the action.
Immediately actionable controls keep the zero-wait fast path. When a target is
animating, briefly covered, disabled, or not yet focused, the Collector waits
locally up to the existing action timeout, requires two consecutive stable
animation-frame geometries, then rechecks visible, topmost, focus, enabled,
document, identity, semantic authority, and token before dispatch. Any identity
or authority change fails stale; no replacement object is silently rebound.
Failures report `failure_stage`, `failure_code`, and `retry_safe`.
The default call supplies `object_id` plus any required payload and omits
`operation`. Runtime uses the current canonical object's advertised affordance
to compile the action, fails on zero applicable affordances, and requests an
explicit operation only when several applicable affordances remain.
For a known compatible role that is temporarily disabled, the caller may pass
the explicit operation to enter the same bounded local wait. During that wait
only the one-way enablement transition from false to true may preserve the
action basis; every other semantic or authority change fails stale.

An Agent may send already-planned independent form edits as one `saccade.act`
batch. The Runtime preflights the full object-ID plan, rejects protected or
unsupported roles before dispatch, refreshes each private action token and
revision locally, and returns value-free per-step verification plus one final
transition. Batches exclude submit/navigation buttons, links, uploads, and
arbitrary controls; they do not expand Extension authority.

If Truth verifies the target transition, the action is complete. If software
input leaves a bounded target state provably unchanged, the result may return
`external_execution_required` with `retry_safe: true`. Codex, Claude, or
another Agent then acts with its own tool in the same authorized browser tab
and uses Saccade Truth to verify the transition. A result that may already have
an unobservable side effect is never marked safe to repeat. If the Agent's tool
cannot control the same browser instance, the integration is incompatible.

The optional `reference-actuator-mcp` may consume internal revision-bound
authority for regression and compatibility testing. That interface is
`saccade.reference.*`, loads native permissions lazily, and marks every receipt
with `reference_actuator` provenance. It is outside the default Truth API.

## Required tests

Extension tests cover all catalogued role/name/state/affordance projections,
Profile bans, full→delta, dynamic replacement, same-origin iframe, open Shadow
DOM, delayed render, and stream gaps. MCP tests prove the default six-tool
surface, bounded object-addressed action authority, blocking revision reads,
and unsolicited resource updates. Lifecycle tests prove ownership labeling, Agent-owned close,
user-shared close rejection, and Host Truth disposal. Default installation
must pass without Accessibility.

The local Chrome and Edge gate covers the machine inventory but is not public
web compatibility evidence. Source-diverse public cases must retain truthful
limitations and failures; they may not be made to pass with site-specific
selectors or an execution fallback.
