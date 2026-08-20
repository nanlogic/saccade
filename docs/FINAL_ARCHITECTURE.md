# Saccade final architecture

Status: accepted direction, 2026-08-02.

## Permanent product objective

> Saccade is a live semantic Truth Layer for the web. The Extension continuously
> compiles an authorized page, publishes a full semantic view, pushes meaningful
> deltas, and offers bounded object-addressed software input. The Agent client
> owns execution whenever that software route cannot prove a safe result.

Every core change preserves fast interaction, low model-token cost, easy
maintenance and extension, trustworthy observation, and model independence.
Saccade is not a browser-testing framework, coordinate clicker, input backend,
or model-specific plugin.

## Product responsibility boundary

Core Saccade owns page semantics, stable document-local identity, full→delta
compilation, iframe and open Shadow DOM composition, Profile filtering, honest
opaque/restricted boundaries, and observation of the page transition after an
external action.

Core Saccade may dispatch only Registry-approved, object-addressed software
click, select, and type operations through `saccade.act`. It accepts no
selector or coordinate and never escalates this route to native input,
Accessibility, Playwright, CDP, or screenshots. A bounded software attempt that
provably leaves its target unchanged may hand execution to the Agent client.
If an action's page revision advanced only because other same-document objects
changed, Runtime may rebind the untouched target to the latest opaque authority
using the retained source journal. Any target/option change, missing base, or
document replacement still rejects the stale request.
The normal public call does not require the Agent to restate a control's
already-projected semantics. Runtime compiles a sole current `click`, `type`,
or `select` affordance into the operation; a text payload implies `type`, and
`option_object_id` implies `select`. An explicit operation remains available
only for compatibility or a genuinely multi-affordance object. Inference never
widens the Registry or substitutes a missing affordance.
The same tool may accept one preplanned batch of independent editable, select,
checkbox, radio, and switch operations. Runtime validates the complete plan,
rebases every step to current Truth, verifies every transition, and returns one
final delta. Submit, navigation, upload, and other material actions cannot be
hidden inside that batch.
The optional Reference Actuator retains its separate native-input policy.

## The single route

```text
authorized Chrome/Edge tab
  → Extension compiler
  → Native Messaging Host
  → owner-only local IPC
  → MCP adapter
  → Agent
```

The default route transports Truth and finite `saccade.act` software commands.
It does not request Accessibility or expose a native-input backend, selector,
locator, screenshot, or arbitrary-coordinate action surface. There is no
Playwright, CDP, embedded-browser, vision, or hidden execution fallback.

The development tree may wrap the Native Host, Truth state, and MCP adapter in
`Saccade Dev Runtime.app` for signing and Native Messaging tests. This wrapper
is internal tooling, not a public component or distribution format. The public
Runtime is headless and has no webpage-control responsibility. On the macOS
Preview, a disconnected `tabs.open` may invoke one fixed-purpose lifecycle
wake: the MCP adapter opens only the validated Saccade Extension
`popup.html` URL in the last connected Chrome/Edge family. It cannot accept a
page URL, selector, coordinate, or script. After the Extension reconnects, the
requested HTTP(S) URL still travels through the normal Native Host command and
is created by the Extension.

`tabs.open` creates and authorizes a tab in the managed Chrome/Edge instance.
It selects a focused ordinary browser window explicitly. If the connected
browser instance is alive without any ordinary window, the Extension creates
one and opens the Agent-owned tab there; this routine lifecycle state never
requires an Agent to switch browser routes or ask the user to restart.
If Chromium terminated Native Messaging after the last window closed, the
bounded lifecycle wake above starts the same Extension route before retrying
the request. It is not a second navigation or execution route.
`tabs.open` also carries a provisioned claim for Agent clients whose own
web-act or computer-use tool can act only in tabs that client created itself.
`claim: "arm"` records one session-only intent bound to the requested origin and
creates, reads, and authorizes nothing. The Agent client then creates the tab
with its own tooling, and `claim: "confirm"` names the returned `claim_id`
together with the exact `tab_id` the client received. Saccade never hands that
identity back or infers it. Only the first new tab that appears on the armed
origin inside the claim window can be confirmed; the Extension enumerates no
other tab, and any mismatched token, identity, origin, or expiry fails with one
uniform message and consumes the claim. A confirmed tab is Agent On for that
Native Host session with `provenance: agent_client`, and is revoked on Stop
sharing, tab close or removal, Host disconnect, and browser startup. Ordinary
user tabs remain Agent Off; the claim can never authorize more than the single
tab it latched.
Tabs created by a user or webpage never inherit Agent On from an Agent-owned
`openerTabId`; every new context requires its own `tabs.open`, exact claim, or
explicit user share.

`tabs.close` is the matching lifecycle cleanup operation. It can close only a
tab recorded by the Extension as Agent-owned; it cannot close a user-shared or
otherwise user-owned tab. Tab closure revokes collection and discards retained
Host Truth for that tab. This bounded cleanup authority is not webpage input
or general browser execution.
An Agent may act with its own web-act or computer-use tool only if that tool
controls the same browser instance and tab. A separate embedded browser cannot
be mixed with Saccade truth; clients must report that combination as
incompatible rather than add a fallback route.

Link Truth may include a resolved, bounded HTTP(S) `navigation_target` without
embedded URL credentials.
This is current semantic state, not a DOM locator or coordinate authority. The
Agent may use the link object with `saccade.act` for same-context navigation,
or pass its target to `tabs.open` to inspect a separate source. Navigation that
opens a new context or downloads is handed to the Agent client explicitly.
Search snippets remain discovery evidence only. A recommendation or verified
claim requires opening and reading its relevant source page. Temporary search
tabs are closed after use, while useful supporting result pages are retained
for user inspection unless the user asks otherwise.

## Truth compiler and state

The Extension—not the model or MCP adapter—interprets DOM, ARIA, registered
control semantics, current rendered geometry, visibility, relationships, open
Shadow DOM, and accessible same-origin frames. It emits:

- one full document view;
- `appeared`, `updated`, and `disappeared` semantic objects;
- document, viewport, and semantic revisions;
- explicit stream gaps and resets;
- observed transition evidence.

Runtime identifies the root frame from the protocol relationship
`parent_frame_id: null`, not from the total frame count. A page with one or
more same-origin child frames therefore keeps one truthful root scope; a
`frame_scope: root` working-set query cannot accidentally filter out the whole
top document merely because child frames exist.

Rendered semantic evidence includes bounded visible leaf text from generic
layout containers when that text is outside editable controls, images, and
already-projected structural objects. This keeps scorecards and result metrics
observable without creating page-specific result schemas.

Each public object may contain role, accessible name, safe state, affordances,
stable document-local identity, current geometry, provenance, and limitations.
`document_bounds` and `viewport_bounds` are CSS-pixel rectangles in the
object's frame document and frame viewport coordinate spaces. Movement,
resizing, scrolling, and rendered animation update geometry on the same object
identity and produce pushed Truth changes. Geometry is observation, not action
authority: the API still contains no locator, DOM path, arbitrary-coordinate
action, editable value, protected value, cookie, browser storage, or default
execution authority. Profiles are strict Runtime inputs with Agent-facing
behavior and bounded filtering policy. Profile `ban` filtering happens after
canonical control recognition and before the Agent projection. A filtered
control and its action authority are both absent. Profile policy cannot change
recognition or reveal editable values, protected values, cookies, browser
storage, locators, or arbitrary execution authority.

The Extension is the only product safety/redaction gate. It protects password,
SSN, and EIN fields and masks SSN/EIN-shaped text before emission. MCP adds no
data taxonomy, confirmation policy, or action gate; the Agent client and its
LLM own all decisions beyond that Extension boundary.

For each authorized document, the Extension sends one eager complete Snapshot
as soon as its Collector is ready. Later revisions cross Native Messaging as
source-compiled deltas only: complete values for appeared/updated identities,
disappearances, and refreshed opaque authorities for unchanged actionable
objects. The Host materializes those messages into one complete current
observation plus a bounded compact revision journal for recovery; neither side
retains one full page per revision.
MCP applies document-scoped aliases and response compaction while
preserving current geometry; it does not infer page meaning by comparing
snapshots. High-frequency geometry is frame-bounded and clients fold deltas to
the latest object state instead of replaying every intermediate animation
position through the model. A missing base revision or a stream discontinuity
produces a full reset rather than a fabricated delta. Canvas and WebGL remain
bounded opaque surfaces unless an approved application semantic bridge
publishes revalidatable objects.

## Public MCP API

Default MCP exposes exactly:

- `saccade.system.capabilities`
- `saccade.tabs.list`
- `saccade.tabs.open`
- `saccade.tabs.close`
- `saccade.truth.read`
- `saccade.act`

The public tool descriptions deliberately identify capabilities, tab open,
and Truth read as the discovery, browser-navigation, and page-reading route for
web research. Clients that defer or lazily index MCP tools must discover
Saccade before choosing another web tool. A registered timeout is an unhealthy
route, not an absent one; after bounded retry/reconnect, the browser task stops
instead of silently falling back.

`tabs.list` labels each authorized tab as `agent` or `user_shared`, and reports
`provenance` as `saccade_tabs_open`, `agent_client`, or `user_shared`. Agents
receive an MCP-session projection: a session sees Agent-owned tabs it opened or
claimed itself plus every explicitly `user_shared` tab, but never Agent-owned
tabs belonging to another concurrent MCP process. Truth read, action, and close
are restricted to that same projection. The Extension ACL remains the
browser-session authority; this downstream scope prevents independent tasks on
one machine from selecting or closing each other's temporary tabs. Agents
close Agent-owned tabs opened only for temporary research when the task ends.
They retain result pages the user may inspect or continue, pages with unsaved
or in-progress work, and tabs the user explicitly asks to keep. They never
close `user_shared` tabs through Saccade.

Tab ownership survives Service Worker replacement, development Reload, and
Extension update, because those events do not end the browser session or close
its tabs. The Extension clears the value-free tab-identity ACL on browser
startup before its first Host hello, using session-scoped storage to distinguish
a browser restart from a Service Worker reload. A delayed `onStartup` event
therefore cannot revoke a tab granted after Host readiness.

Capabilities use `saccade.capabilities/6`, declare `product: truth_layer`,
push/resource support, and retain `execution_owner: agent_client` for final
decision and external-action ownership. The machine-readable
`execution_contract` declares `saccade.act` as the preferred bounded software
route and its safe-handoff condition. Capabilities do not expose native input
or Accessibility state. They also report the live Extension
candidate identity and, when a development installer has pinned one, the
expected candidate identity. A mismatch is disconnected/unready state, never
accepted candidate evidence.

Fair benchmark processes may explicitly ignore user-local learned input policy
so a remembered native preference cannot contaminate an engine comparison.
That override is driver-only, is declared in capabilities evidence, and never
modifies or deletes the user's policy file.

`truth.read` requires one exact `tab_id` and is an automatic per-Agent cursor.
When a task already identifies useful labels, roles, or affordances, the first
read may carry a bounded semantic `query`. Runtime selects at most 32 matching
objects and returns a `working_set` with frame summaries while retaining the
complete canonical observation locally. This is an indexed Truth projection,
not a selector, DOM query, execution route, or discarded page region.
The query can wait for a declared minimum match count during bounded page
hydration. Its default visibility policy includes rendered offscreen controls
but excludes hidden and unknown objects. A later exact-label query may carry
`after_revision` from the preceding action receipt. Runtime treats it as a
canonical lower bound: when that revision is already current it projects
immediately, otherwise it waits locally for a newer revision. The bounded
working set folds any older queued ambient pages, so the Agent never needs a
failing revision read followed by an unbounded retry.
Text matching requires every whitespace-separated query word across the safe
name, text, and description fields. For controls, Runtime also searches the
bounded nearest preceding heading context already present in canonical Truth,
allowing an Agent to ask for a named section/example without transferring every
same-role control or introducing a selector.
For a task that names several controls, `text_any` accepts one bounded label or
placeholder phrase per target. Words remain conjunctive within each phrase and
the phrases are alternatives. Before filling the remaining response budget in
document order, Runtime reserves one distinct match for each phrase that has a
match. A noisy early phrase therefore cannot consume `max_objects` and hide a
later named target that is already present in canonical Truth. The response
also reports one match count per phrase. ASCII words use semantic word boundaries, so a
query for `Male` cannot match `Female`; punctuation-bearing and non-ASCII terms
retain bounded substring matching. Runtime can therefore return named controls without
also transferring every unrelated object that shares their roles. `min_objects`
is the explicit hydration boundary: once that many matches exist, unrelated
animation, advertisement, or iframe churn cannot delay the working set. If no
matching revision arrives before the bounded timeout, Runtime returns the
partial set with `settled:false`; slowly hydrated controls therefore retain the
declared opportunity to appear.
Its first read of a document is either one bounded full view or, when that full
view would exceed the MCP response budget, one automatic compact catalog. The
catalog covers every projected semantic object and carries its stable
`object_id`, role, bounded label preview, affordances, and visibility. It is
not a partial region and does not discard the canonical full observation. The
Agent dereferences only relevant identities by passing `object_ids`, the exact
`document_id`, and `basis_revision`; Runtime returns their current full object
records without advancing the delivery cursor. Every later ordinary read
returns only the revision-bounded delta since that MCP session's last delivered
revision. A document transition, stream gap, or lost base automatically returns
a new full-or-catalog reset. If an Agent loses or corrupts its own folded cache,
it may pass `resync: true` once with that exact `tab_id`; this resets only that
Agent session's cursor for that tab. There is no all-tabs Truth read or resync.
With `after_revision`, the Runtime waits locally for a newer revision; when it
is combined with a semantic query, an already-current equal or newer canonical
revision is sufficient. The model does not poll the page. Truth resources use
`saccade://tabs/{tab_id}/truth`; subscribe/unsubscribe and unsolicited
`notifications/resources/updated` carry the same Extension-produced stream.

Within an Agent delta, `appeared` carries one complete new object,
`disappeared` carries its stable `object_id`, and `updated` carries that ID plus
a recursive JSON merge patch. The patch contains only changed fields; `null`
removes a prior field. Geometry-only churn therefore sends changed coordinates
instead of retransmitting names, roles, state, and affordances for every object.

Each `truth.read` call may select `delivery_mode: live` or
`delivery_mode: economy`; omission preserves the compatible `live` default.
`live` returns the next pushed revision immediately for latency-sensitive work.
`economy` adds a bounded 150 ms MCP-local coalescing window and returns the
latest folded truthful delta for routine low-token work. Neither mode filters
objects, removes current geometry, changes Profile behavior, or constrains the
Agent's next choice. Capabilities advertise both modes so any LLM may select
per call without product policy deciding for it.

The public tool has no model-selected `view_mode`, `full`, `index`, or `region`
override. Full-versus-catalog selection is a deterministic Runtime size rule,
not an LLM guess. Object detail dereference addresses stable identities within
one exact document revision; it is not a page view mode. `resync` is recovery
for one named tab, not a selectable routine view mode.
Public MCP tool input schemas avoid top-level `oneOf`, `allOf`, and `anyOf`
composition because supported Agent registries do not accept it uniformly.
Cross-field constraints such as the mutually exclusive single-action and batch
forms of `saccade.act` remain strict Runtime validation and are described on the
tool itself. This compatibility shape does not weaken object, document,
revision, operation, or protected-value validation.
Canonical current Truth still exists inside the Extension and Host for
verification and automatic reset, but it is not repeatedly transferred to the
model during a continuous document session. `saccade.act` advances the same
Agent cursor. A verified single action or `all_verified` batch returns only its
compact semantic proof; unrelated structural, geometry, and frame churn stays
silently queued on the ordinary cursor. An unverified action may return
same-frame appeared/disappeared objects when they are its only useful evidence.
No canonical change is discarded.
An action batch returns the final `document_id`, `revision`, and
`next_basis_revision`; a later separate submit or navigation action can use
that basis without an intermediate Truth read.
MCP initialize carries only the compact route and full→delta invariant.
The first `system.capabilities` call returns Profile name and behavior once,
plus `profile_digest`, `runtime_version`, and `mcp_contract_hash`; the ban list
remains private. Agent sessions must restart after a contract hash change.

The wire protocols remain `saccade.observation/1` and
`saccade-extension-host/1`. Optional action-authority fields remain legal on
the internal wire for the Reference Actuator, but the default Agent projection
omits them.

The Extension `hello` includes a content-addressed candidate identity plus a
bounded browser-family lifecycle route (`chrome` or `edge`, development flag,
and its own `popup.html` URL). The Host validates and persists that route in
the owner-only Runtime directory. The
Service Worker accepts a page Collector only when both loaded the same
candidate. Development installation stamps the unpacked directory and pins the
same identity for the Native Host; `attach` succeeds only after the live
Service Worker reports it. A newly bootstrapped Worker checks the installed
candidate on reconnect and self-reloads when it changes. Chrome restart is the
not an activation proof for an unpacked Extension. A pre-handshake Worker that
cannot execute code it does not yet contain requires one explicit Reload from
Chrome's Extensions page; the Agent client may perform that development setup
operation with its own browser-settings tool.

Worker activation does not rewrite a static Collector already loaded in an
open page. Saccade never reloads such pages merely because an update occurred.
If the user later explicitly shares one of those tabs, the Extension recovers
the stale Collector with one ordinary tab reload, waits boundedly for the
current candidate, and completes authorization. This recovery adds no dynamic
script-injection permission or fallback compiler.

## Truth Catalog and Registry

`catalog/truth_inventory.json` is the canonical public Truth inventory. It
accounts for every protocol role, reusable control variant, structural
boundary, and its conformance gate. `catalog/controls.json` is the narrower
Reference Actuator module catalog; its 15 rows must never be presented as the
total Truth Layer surface. The core Registry owns semantic recognition and
projection consistency. Adding a role or variant must not add site-specific
selectors or execution policy.

The current machine inventory contains 34 protocol roles, 12 reusable
variants, and 6 structural/push boundaries. The 34 roles consist of 15
interactive roles, 17 additional semantic roles, `frame`, and reserved
`unknown`, which is forbidden from Agent output. Date/time/color inputs,
listbox/combobox implementations, and drag/drop reuse existing roles rather
than creating one protocol role per HTML element.

Common controls require same-candidate Chrome and Edge truth evidence before
becoming `publishable`. Fixtures are regression evidence, not proof of public
web compatibility.

## Evidence and comparison boundary

The complete local Chrome and Edge gate proves that the Extension → Host →
Runtime → MCP projection and pushed-delta framework works for the current
inventory. It does not prove universal compatibility with modern websites.

A fair Playwright engine comparison starts both lanes from the same generated
unknown URL and natural-language task. The Saccade lane uses Truth plus
`saccade.act`; the Playwright lane uses official Playwright MCP observation and
execution. Agent-client fallback compatibility is reported separately because
it introduces a client-specific executor. Record completion, discovery time,
initial bytes/tokens, delta latency, re-observation count, stale/replacement
recovery, tool calls, total time, and failures. Click latency alone is not a
product comparison.

## Reference Actuator

Historical execution code is retained as an optional development adapter:

```text
saccade-runtime reference-actuator-mcp
```

It exposes only `saccade.reference.*` tools and is never written into default
Codex or Claude MCP configuration. Its separate catalog owns native primitives,
backend policy, verifier rules, form fill, reflex loops, stale/replay checks,
and receipts. Native permissions and local input policy are loaded lazily only
after an explicit reference action request. Every returned execution artifact
has `reference_actuator` provenance and cannot establish default product
execution capability.

Sharing a source tree or executable with this explicitly selected diagnostic
subcommand does not make it part of the internal development wrapper, default
MCP, or the release architecture. Its permission failures are test-harness
failures, not Saccade product blockers.

The Reference Actuator's reflex loop treats dynamic preparation invalidation as
recoverable stale work. It waits for a newer browser-pushed revision and retries
within a 45 ms bounded recovery budget; exhaustion remains an explicit failure.
For Extension-dispatched semantic reflex clicks, current identity, token,
affordance, visibility, and occurrence advancement remain mandatory, while
physical screen-focus, topmost, and viewport-coordinate checks remain exclusive
to native OS input.
This diagnostic SLA and recovery behavior do not add execution to default MCP.

## Installation and verification

The first public release has two user-facing components: the Saccade browser
Extension and `npx -y @saccade/setup`. The explicit setup command installs the
headless local Runtime, user-level Native Messaging manifests, and local MCP
entries for supported Codex and Claude clients. It does not install a visible
application, use an npm `postinstall` hook, or request Accessibility. The
normative lifecycle and compatibility rules are in `SETUP_TARGET.md`.

Cloud-only Agent sessions cannot access the user's local Extension and Native
Host. They are incompatible with the first release; no remote relay or page
upload fallback is part of this architecture.

`dev.sh up`, `status`, `test`, and `down` exercise the Truth Layer without
Accessibility. `dev.sh test-actuator` explicitly exercises the optional
Reference Actuator and may require native-input permission.

The default Runtime has no generic `repair` command that requests
Accessibility. The only permission helper is explicitly named
`reference-actuator-repair` and is outside installation, status, MCP, and
product recovery flows.

The default dogfood loop is:

```text
tabs.open → truth.read/subscribe → saccade.act(object_id)
→ Extension software input → revision-bound semantic verification
→ explicit Agent-client handoff only when retry_safe is true
→ tabs.close when the Agent-owned tab was temporary
```

The public result distinguishes verified software input from an explicit safe
external handoff without exposing native backend receipts. Detailed OS/backend
receipts remain exclusive to Reference Actuator tests.
