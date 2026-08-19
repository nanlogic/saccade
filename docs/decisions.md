# Architecture decisions

Entries are chronological records. The 2026-08-02 Truth Layer decision
supersedes every earlier statement that made action execution, action tokens,
input backends, or closed-loop receipts part of the default product. The
2026-08-10 setup decision supersedes earlier DMG, visible app, and Windows Setup
delivery plans. Those earlier entries remain only as migration history.

## 2026-08-18 — Eager Snapshot, delta transport, and exact-tab recovery

Accepted: authorizing a document immediately starts its Collector and produces
one complete Snapshot without waiting for an Agent read. Every later continuous
revision is transported Extension → Host as a source-compiled delta rather than
another full page. Runtime materializes one current complete Truth and retains
only a bounded compact revision journal; this is not a permanent event store.

Accepted: every recovery operation is scoped to one required `tab_id`. A
transport continuity failure makes only that tab temporarily unavailable and
requests one fresh Snapshot from its Collector. Separately, an Agent that loses
its own folded cache may use `truth.read({tab_id, resync:true})` to reset only
its cursor for only that tab. No API returns or resets Truth for all tabs.

Accepted: routine delivery remains automatic full-once then delta-only.
`resync` is an explicit repair operation, not a model-selected projection mode
or permission to poll repeated full pages.

## 2026-08-10 — Store Extension plus explicit npm setup

Accepted: the first public release has two user-facing components: the Saccade
Extension from the Chrome Web Store or Edge Add-ons and
`npx -y @saccade/setup`. The explicit command installs a headless
platform-specific Runtime, user-level Native Messaging manifests, and local
MCP entries for supported Codex and Claude clients.

Accepted: setup uses no npm `postinstall` hook, visible Runtime application,
DMG, Windows Setup, Accessibility permission, or default Reference Actuator.
The internal macOS app wrapper remains development-only. Cloud-only Agent
sessions are incompatible until a separately approved remote architecture
exists.

## 2026-08-09 — Dynamic geometry is public Truth

Accepted: every projected object exposes its current `document_bounds` and
`viewport_bounds` in CSS pixels. Bounds are relative to the object's frame
document and frame viewport respectively. Stable identity does not depend on
geometry, but position or size changes are first-class `updated` Truth changes
on that identity.

Accepted: scroll, resize, layout, transition, animation, and observed-element
resize signals keep geometry current. Visual churn is rendering-frame bounded;
the Host keeps the newest complete state and Agent clients fold deltas into
their cached view rather than asking the model to poll or replay intermediate
animation frames.

Accepted: geometry remains value-free observation. Password, SSN, and EIN
objects expose their safe bounds and `has_value` state while their contents
remain unavailable. Default Truth still omits locators, DOM paths, action
tokens, and authority to issue arbitrary-coordinate actions.

This decision supersedes every earlier statement that exact bounds were
Host-only, omitted from Agent views, or categorically unavailable to Agents.
Those older statements apply only to arbitrary-coordinate action inputs and
historical Reference Actuator evidence.

## 2026-08-09 — Reflex results stay observable after a bounded local run

Accepted: visible leaf text in generic layout containers projects through the
existing non-actionable `text` role when it is outside editable controls,
images, structural objects, and dialogs. This makes score, accuracy, combo, and
result details observable without a result-page schema or site-specific result
selector. The existing structural byte and object limits still apply.

Accepted: a reflex control may derive its safe occurrence from an authored
`data-saccade-reflex-occurrence` value or the approved MouseAccuracy bridge's
visible score. MouseAccuracy game mutations are frame-batched before semantic
compilation so one fast UI transition cannot overrun Native Messaging with
intermediate full snapshots. The Extension still emits every distinct compiled
target or score state, and a post-run revision-bounded read remains the required
verification path.

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

Superseded on 2026-08-10: the earlier plan called for a signed/notarized macOS
DMG and a signed Windows Setup. Neither is part of the first public release.

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
Agents do not provide locators or arbitrary-coordinate action inputs. Public
geometry is now governed by the 2026-08-09 Dynamic geometry decision.

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

Superseded on 2026-08-11 for ordinary-Chrome candidate activation: directory
versioning alone cannot prove which MV3 Worker and Collector are live. The
candidate-identity handshake decision below is now required.

## 2026-07-29: Browser-owned confirmation remains outside Extension Truth

Superseded on 2026-08-06: `saccade.system.capabilities` no longer publishes a
`browser_owned_confirm` policy. Chrome and Edge still do not expose these
dialogs to the page Extension as revision-bound objects, but Saccade MCP does
not classify them or prescribe a confirmation rule. The calling Agent owns
that decision.

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

## 2026-07-30: Agent Browser views are incremental; form loops are locally orchestrated

Accepted: complete `saccade.observation/1` snapshots remain the
Extension-to-Host evidence and verification boundary. They are no longer the
shape repeated to an Agent after every control action. Each MCP process keeps a
per-tab Agent Browser base. Its first `saccade.agent-view/1` is full; later
views contain semantic appeared, updated, and disappeared objects plus opaque
authority refreshes. Navigation, gaps, missing bases, and large changes produce
a new full view. This changes neither `saccade.observation/1` nor
`saccade-extension-host/1`.

Superseded on 2026-08-09: exact bounds are public Agent Truth and geometry
changes produce object updates. Per-object internal evidence revisions,
loop-class tokens, and action authority remain Host-only. Public bounds do not
create an arbitrary-coordinate action surface.

Accepted: `saccade.web.form.fill` is one bounded orchestration tool, not one
tool per control and not a second execution route. It preflights the entire
initial plan, excludes protected/file/submit/navigation operations, then runs
each supported form control through its existing Registry module and closed
loop. Later controls are refreshed locally by runtime object identity instead
of making the Agent observe and reason again. The Agent receives value-free
step receipts and one final view update.

Accepted: Host receipts retain the full settled post-action observation for
local audit and verification. MCP returns a compact Agent receipt and does not
duplicate a full JSON value as both text content and structured content.
`tabs.open` waits for the first authorized collector observation before it
reports `observation_ready=true`.

Accepted: settlement may return before the legacy 300/750 ms quiet window only
after the registered verifier already succeeds on a fresh observation and that
verified revision remains quiet for a bounded 25 ms (1 ms for the reflex
policy). A fresh focus-only revision is insufficient. A form plan may locally
retry a refreshed target only for an explicitly recognized pre-dispatch stale
failure; uncertain or post-dispatch failures are never retried.

Development proof: the Selenium official `web-form.html` run
`web-form-agent-compact-3x` passed three of three tasks and 18/18 receipts with
no editable-value disclosure. Median task time was 2.391 seconds and median
model-facing output was 4,863 tokens. The out-of-band Playwright MCP best case,
given selectors with snapshots disabled, measured 1.327 seconds and 421 tokens;
the comparison is retained as a boundary, not rewritten as a Saccade win.

## 2026-07-30: Compact Agent defaults and evidence-driven select settlement

Accepted: `saccade.agent-view/1` declares common object defaults once and omits
matching per-object `frame_id`, visible state, no-transition state, and
non-protected state. Non-default values remain explicit, and the complete v1
Host observation is unchanged. The redundant evidence `kind` is omitted from
the Agent projection because semantic `role` already determines the object
type. Form step summaries no longer echo name and operation already present in
the request.

Accepted: macOS native select keeps a bounded 300 ms popup handoff but removes
the old 300 ms post-action sleep. The selected-option verifier and a fresh
observation are the only success authority. Windows retains its existing popup
handoff pending platform evidence, while also relying on the verifier rather
than a second fixed post-action wait.

Accepted: native editable input waits 100 ms after the real center click. On
macOS the Host then posts Unicode to the exact browser PID that launched that
Native Messaging Host. Native popup and file-dialog keys remain system HID
events for the OS surface opened by the preceding real click. A fresh-profile
Chrome regression showed that a global keyboard post could leave a caret in
the intended field while delivering no value; PID-bound delivery restored two
consecutive full control gates and the public Selenium form gate. There is no
retry, arbitrary process choice, or weakening of `has_value` verification.

## 2026-07-30: Collector readiness does not wait for all page resources

Accepted: an authorized HTTP(S) tab starts collector injection after document
commit while Chrome reports `loading`, with per-tab authorization deduplication.
The previous `complete` dependency failed on GameSpot because advertising and
other third-party resources kept the tab incomplete past the 15-second
Truth-Layer gate. Initial truth may grow through ordinary deltas as the page
continues rendering; navigation still retires the previous document.
Correction from the first reduction: injection and bounded provisional
observation begin during loading. Loading-state objects carry no affordances or
action authority, so pre-interactive document state cannot be executed. At
`DOMContentLoaded` the collector recompiles and publishes the current semantic
state and affordances as an ordinary browser-pushed revision. This prevents a
slow resource or redirect lifecycle from withholding the first Truth record
while preserving the non-actionable pre-interactive boundary.

## 2026-07-30: Agent object aliases remove repeated internal identity cost

Accepted: MCP replaces long internal object IDs with monotonically assigned
document-scoped aliases (`o1`, `o2`, ...). Full views, deltas, and authority
refreshes use the alias consistently. Select option aliases are translated back
inside MCP before Host validation; an unknown or prior-document alias is
rejected. Extension-to-Host identity and both v1 wire schemas are unchanged.

Accepted: per-revision action tokens use 128 bits of browser randomness instead
of 192 bits. They remain opaque, single-use, document/revision-bound, and
required in addition to the non-authorizing Agent alias. Browser, document, and
loop identities retain 192 bits.

## 2026-07-30: Browser-pushed revision wait replaces Agent polling

Accepted: `web.observe` optionally takes `after_revision` and a bounded
`timeout_ms`. The Host waits on its observation condition variable and returns
only after the authorized tab has a newer browser-pushed revision. The default
without `after_revision` remains an immediate read of current truth. This is a
local transport wait, not an LLM loop, page re-analysis, or invented delay.

The public ScrapingCourse Anti-Bot Challenge exposed the cost of client polling:
one cold run made 59 observe calls before dynamic truth appeared. With the
revision wait available to clients, changing pages can block locally and return
the next semantic delta without spending repeated tool results or model turns.

## 2026-07-30: Current public parity results stay task-specific

Recorded: the final Selenium official web-form candidate passed 3/3 in both
lanes. Saccade's median was 2.475 seconds and 2,779 model-facing tokens;
Playwright MCP's selector-best-case median was 1.325 seconds and 421 tokens.
This is a Saccade compaction improvement, not a speed or token win.

Recorded: on the public ScrapingCourse Anti-Bot Challenge, Saccade returned the
required truth in 2/2 runs (505 ms cold, 233 ms warm, about 414 tokens). The
official Playwright MCP lane returned 484 result characters but not the required
challenge text, so the independent-lane report is `SACCADE_ONLY`. This supports
only that exact reproducible page/task and does not authorize a general anti-bot
or CAPTCHA-bypass claim.

Recorded: GameSpot illustrates the remaining output problem. Both lanes passed;
Saccade was faster on the warm comparison but returned the full 237-object Truth
Layer at a 17,335-token median versus Playwright's custom text extraction at 394
tokens. Future compaction must preserve the Truth Layer and browser-pushed delta
model rather than hiding controls to improve a benchmark.

## 2026-07-30: Freeze the permanent product north star

Historical; superseded by the 2026-08-02 Truth Layer decision below:

> Saccade is a browser protocol that lets any Agent continuously understand a
> web page, receive browser-pushed changes, and operate it through verified
> closed loops.

The permanent product qualities are fast interaction, low model-token cost,
easy maintenance and extension, trustworthy execution, and model independence.
The browser-pushed Truth Layer and deltas define understanding; Catalog-backed
control modules define extensible execution vocabulary; fresh postconditions
define receipts; declarative Profiles define user behavior policy. MCP is the
current adapter and no protocol meaning may depend on one model, vendor,
prompt, or Agent framework.

Implementation details and coverage may change, but the positioning does not.
Saccade will not become a browser-testing framework, coordinate clicker, or
model-specific browser plugin. Future proposals are evaluated by whether they
improve Agent understanding, delta efficiency, verified execution, or the ease
of adding and maintaining closed-loop controls.

## 2026-07-31: Frames compose inside the proven root collector

Accepted: the authorized top-document collector directly traverses accessible
same-origin iframe documents and open shadow roots. Root observation keeps the
existing `collector.observation` message and never depends on a separate frame
topology service. This preserves the proven first-observation path and makes
frame coverage additive.

Descendant objects carry frame identity. Native preparation converts a target's
local box through its same-origin `frameElement` chain and rejects a covered or
ambiguous ancestor. Inaccessible frames report `restricted_frame`. Closed shadow
roots are not traversed and current coverage does not claim reliable detection.
No code or classifier was copied from the legacy repository.

Paired managed Chrome and Edge run `20260731T051006Z` produced two native
`accepted_by_os + verified` receipts per browser. The paired common
control/Profile regression passed in Chrome run `20260731T050149Z`; Edge had one
native-select miss in that paired run and passed the complete rerun
`20260731T050252Z`. The miss remains release evidence for native-select
reliability work rather than being discarded.

Recorded public parity: W3C WAI-ARIA comparison run `20260731T050337Z` matched
Saccade and the out-of-band Playwright oracle for radio, switch, tab, and menu
item in both Chrome and Edge. The Selenium official web-form Chrome benchmark
passed 3/3 in both lanes: Saccade median 2.486 seconds and 2,776 model-facing
tokens; Playwright selector-best-case median 1.369 seconds and 421 tokens.
Saccade was 1.816x the task time and 6.594x the token count in this single-shot
form benchmark; no speed or token-win claim is authorized from it.

## 2026-07-31: Input backend selection is automatic on the Agent surface

Accepted: ordinary MCP discovery exposes one action transaction, `web.act`.
The Registry defaults finite click roles to software and keeps editable,
selection, and file operations native-required. A receipt-backed local rule may
strengthen one page/control to native on its next fresh token. The model does
not select the backend.

Explicit `web.act_native`, `web.act_soft`, and the reflex-loop backend selector
are now local development diagnostics. They are absent from normal tool
discovery, rejected when the diagnostic flag is off, and cannot bypass a
learned native rule or weaken a Catalog native requirement. Managed probes set
the flag explicitly so both backend implementations remain independently
testable without expanding the production Agent surface.

Paired managed Chrome/Edge run `20260731T052312Z` produced seven verified
software receipts and eight verified native receipts per browser through
ordinary Registry selection. Selenium official web-form run
`20260731T052600Z` passed 3/3 with nine software and nine native receipts. The
preceding `20260731T052500Z` run stopped on native select and is retained as a
separate reliability failure.

## 2026-07-31: Fair Agent comparisons start from the unknown page

Accepted: the primary Saccade/Playwright comparison gives the same model only
the same natural-language task and an isolated browser MCP lane. Navigation,
initial semantic discovery, planning, actions, verification, failed calls,
elapsed time, and model usage all count. Neither lane receives selectors,
coordinates, DOM queries, screenshots, site-specific execution code, or state
from the other lane. The older selector oracle remains a narrow implementation
baseline and cannot support Agent-efficiency claims.

Select is now `software_preferred`. The finite Extension primitive revalidates
the control token and option identity, supports native select and registered
ARIA listbox/combobox behavior, and still requires the normal fresh
selected-option verifier. Editable Unicode remains OS input after a token-bound
Extension focus handoff. Local history may still strengthen a select to native;
Profile cannot change either loop.

Managed Chrome run `20260731T121439Z` and Edge run `20260731T122553Z`
each passed the complete cataloged-control and Profile gate on this candidate,
including native select and ARIA listbox/combobox option-identity receipts.

Two order-reversed runs on Selenium's official web form passed in both lanes.
The complete result and limitations are recorded in
`docs/reports/2026-07-31-fair-agent-playwright-comparison.md`.

## 2026-08-01: Agent actions carry intent, not repeated envelopes

Accepted: the public MCP action surface requires a current opaque action token
and operation-specific intent fields. The adapter may resolve that token only
inside the current views it already emitted to this Agent, then restores the
complete browser, tab, document, and basis-revision envelope. Form-plan tokens
must all occur in one current document revision. It then forwards the unchanged
full Host action request. Missing, ambiguous, stale, or cross-document context
fails closed; Host and Extension identity, token, replay, preparation,
native-input, and postcondition checks are unchanged.

Form plans expose `type`, `select`, and `check`. `check` is deliberately limited
to checkbox, radio, and switch roles by the Runtime; Submit and navigation stay
separate `web.act` clicks. This removes model-visible wire bookkeeping and the
generic form-plan click ambiguity without changing either v1 wire schema or a
control closed loop.

For the read-only observation tool, `timeout_ms` without `after_revision` is
normalized to an immediate current-view read. It cannot refresh or authorize an
action and avoids a model round trip for a harmless redundant wait hint.

## 2026-08-01: Visible dialog titles close deferred button effects

Accepted: a visible dialog's page-authored accessible name is projected as a
heading, including when its labelled title uses a non-heading wrapper. No dialog
subtree, editable value, selector, guessed title, or new v1 role is exposed.

Buttons that semantically announce deferred content—form submit,
`aria-haspopup=dialog`, or `aria-controls`—declare
`deferred_content_possible`. Their button-effect verifier may accept a newly
appeared visible heading, alert, or status. New table cells or unrelated object
churn are insufficient. The focused fixture is
`fixtures/controls/dialog_effect.html`; the motivating external gate is DemoQA's
React student-registration modal.

The motivating modal is inserted while its framework fade class computes
`opacity=0`. The collector therefore listens for CSS `transitionend` and
`animationend` and publishes a fresh observation after the visual lifecycle;
it does not weaken the opacity-zero visibility rejection. Deferred button
settlement is bounded to 750 ms so the ordinary verifier can consume that fresh
evidence.

Final same-candidate local gate `20260801T081531Z` passed controls, focused
dialog effect, Profile, and stale authority checks in Chrome and Edge. Two
order-reversed public DemoQA React comparisons passed in
both Saccade and Playwright lanes; metrics and all retained failures are in
`docs/reports/2026-08-01-modern-react-agent-comparison.md`. Catalog status stays
`implementation`.

## 2026-08-01: Dynamic choices expand inside the select module

Accepted: a control module may declare multiple finite operation strategies
without splitting the semantic control. A collapsed ARIA combobox with
`aria-expanded=false` advertises click; its Catalog strategy uses
`primary_click` and `expanded_transition`. The verified receipt carries the
new option delta, after which the existing option-identity select strategy runs
unchanged. Native select does not advertise this expand action.

The motivating Angular Material page required no URL, framework, selector,
special-wait, or execution branch. The same investigation generalized duplicate
control context to every currently actionable control family while continuing
to remove nested controls and exclude protected values. An accepted but
unverified software receipt now states that the local policy already learned
native and requires a fresh authority before another action.

Public stability evidence is source-diverse rather than repetition-based.
Fixture evidence remains regression-only; external status requires two
independent traceable public sources per control and browser. Unknown-page
Saccade/Playwright runs isolate local policy, use Chrome in both lanes, wait for
MCP readiness, reverse order, retain failures, and redact editable values in
both raw and nested tool transcripts. Results and current gaps are recorded in
`docs/reports/2026-08-01-cross-site-stability-and-fair-agent.md`.

## 2026-08-01: The Extension is the Truth Layer compiler

Accepted: page interpretation belongs to the Extension. It continuously
projects DOM, ARIA, visibility, state, relationships, and registered control
semantics into the Truth Layer, and computes appeared, updated, and disappeared
objects at that source. An Agent does not receive a complete page and identify
a form from it. MCP does not compare observations to infer page meaning.

The unchanged `saccade.observation/1` v1 evidence record still carries complete
current objects for local prepare, revalidation, verification, and recovery,
plus its Extension-compiled `changes`. The Host retains current truth; MCP only
applies Agent aliases, response compaction, authority refreshes, and envelope
hydration. The first Agent view is complete and subsequent views are source
deltas. Form fill is a bounded operation over controls already compiled into
that Truth Layer, not a second form-recognition architecture.

The Host also retains a bounded per-document sequence of Extension records.
This is necessary because action settlement may advance through a semantic
revision and then an authority-only revision before returning. The Host folds
only Extension-declared touched identities since the Agent's known revision;
it never recomputes semantic meaning. A missing base produces a full gap reset.

## 2026-08-01: Static compiler and subscribable Agent Browser

Accepted: the ordered Extension compiler bundle is a static isolated-world
content script and stays dormant until the tab ACL authorizes it. Programmatic
multi-file injection is removed. Authorized collectors use a long-lived Port to
the service worker, which retains the one Native Messaging route. A revision
jump clears delta history and forces a full gap reset.

The current Agent Browser is a subscribable MCP Resource at
`saccade://tabs/{tab_id}/truth`. Resource update notifications contain only the
URI, and clients read the resource for a full or delta view. The existing
blocking `web.observe(after_revision)` remains a compatibility surface. Both
consume the same Extension-compiled event history; neither polls or interprets
the webpage.

## 2026-08-02: Truth Layer is the product; execution belongs to the Agent

Accepted, superseding the default execution decisions above: Saccade's public
product is the continuously compiled semantic Truth Layer. Default MCP exposes
only capabilities, tab list/open, and `truth.read`; `web.observe`, `web.act`,
form fill, reflex, input-policy, and diagnostic action tools have no public
compatibility period. Capabilities advance to `saccade.capabilities/5` with
`product: truth_layer` and `execution_owner: agent_client`.

Codex, Claude, or another client acts through its own tool in the same
authorized Chrome/Edge tab. Saccade reports the resulting semantic transition,
not input acceptance. The old execution engine remains only as the explicitly
started `reference-actuator-mcp`, under `saccade.reference.*`, with a separate
execution catalog, lazy Accessibility/input-policy use, and reference
provenance on every receipt. Historical decisions and evidence remain as the
record of that implementation but no longer define the default product API.

The current machine scope is 34 protocol roles, 12 reusable variants, and 6
structural/push boundaries. Fifteen optional Reference Actuator families are a
separate subset. Passing the local Chrome and Edge pushed-delta gates proves
the framework and projection path, not universal public-web compatibility.

The primary Playwright comparison must start from the same unknown URL and
natural-language task. Saccade uses Truth plus the Agent client's own web-act
tool in the same tab, with no Reference Actuator; the other lane uses official
Playwright MCP without prepared scripts or selectors. Completion, discovery
time, transfer/token cost, delta latency, re-observation, stale recovery, tool
calls, total time, and failures all count. No current evidence supports a
blanket superiority claim.

## 2026-08-05: Semantic collection is independent of rendering frames

Accepted: DOM, ARIA, input, change, focus, transition, and animation signals
schedule one microtask-batched compilation. Scroll and resize remain bounded by
`requestAnimationFrame` because they describe visual/viewport churn. Semantic
Truth must not wait for a paint frame: background or occluded tabs may throttle
rendering while their DOM continues to change.

The latency completeness fixture uses a distinct stable object for each
sequential marker. Host/MCP reads intentionally fold retained Extension
revisions into current Truth; overwriting the same object repeatedly and then
requiring every obsolete intermediate value would test an event-log contract
that Saccade does not have. The probe retains delivery batches so consumer-side
folding and scheduling tails remain visible rather than being misclassified as
Collector omissions.

Same-tab Agent testing uses ordinary Chrome with both Saccade and the Agent
client's execution extension installed. `dev.sh attach` prepares the Host and
fixture without launching the isolated managed browser. Starting `dev.sh up`
at the same time is an incompatible test setup because the owner-only Native
Host session cannot switch browser instances.

Codex MCP configuration is installed or restored only by explicit `dev.sh mcp`
commands. Runtime, browser, and test lifecycle commands never mutate the live
client configuration: Codex owns the MCP child-process transport for the task,
and a configuration reload can destroy that transport even though Saccade's
Host client already rereads the owner grant and reconnects local IPC per call.

## 2026-08-05: MCP survives temporary Native Host absence

Accepted: the MCP adapter must initialize even when the Native Host grant or
socket is temporarily absent. It loads the startup Profile from the local
Runtime directory and uses only a short bounded capability probe during MCP
initialization, keeping the client-owned MCP process alive for later recovery.

Each Host call rereads the owner-only grant, opens a fresh local IPC connection,
and retries only transport-unavailable failures within that call's existing
timeout. A recreated socket or rotated capability therefore does not require a
new MCP process. Permission failures and invalid protocol messages still fail
closed and are never retried. This changes neither wire schema nor the
Extension's exclusive ownership of webpage semantics.

## 2026-08-05: Same-page execution uses one plan and one delta verification

Accepted: MCP initialization instructs every Agent client to read one complete
Truth view for the current page, consecutively perform ordinary reversible
operations that are already determined, and then issue one revision-bounded
Truth read. It replans only after an operation failure, a material page
boundary, or a semantic delta that invalidates an assumption. Repeated full
reads and model polling between predetermined fields are rejected as avoidable
latency.

This is an Agent orchestration rule, not an execution transaction inside
Saccade. The Agent client's same-tab tool still owns every operation and
Saccade remains the passive, browser-pushed verification source.

## 2026-08-05: Autonomous completion is the default Profile behavior

Accepted: the default Profile continuously completes ordinary reversible work
until a genuine human authorization boundary actually blocks progress. Merely
seeing a protected field or irreversible final action is not a stopping event:
the Agent skips it, completes and verifies all other safe independent work,
then asks one concrete question at the final blocker. The target experience is
faster than manual completion, not repeated handoffs to the user.

This default does not authorize passwords, OTPs, CAPTCHAs, protected identity,
tax or banking data, payment, publication, submission for review, contract
acceptance, or material deletion. It changes orchestration, not the Truth wire
schemas, execution ownership, or protected-data boundary.

## 2026-08-05: Placeholder provenance and impossible revision recovery

Accepted: editable placeholders remain useful bounded context but are emitted
as `Placeholder: …`, never as an unlabeled description that an Agent could
mistake for the current value. Current values remain undisclosed; `has_value`
continues to describe only live state.

Accepted: if `truth.read(after_revision)` supplies a revision newer than the
current document and therefore cannot be a valid consumed basis, Host returns
the current observation immediately as a full gap reset. It does not wait for
the document to catch an impossible revision. MCP instructions require clients
to retain one full view, fold deltas into it, and treat objects omitted from a
delta as unchanged rather than absent.

## 2026-08-05: Dialog confirmations remain observable without authored ARIA

Accepted: an authored `aria-live` container is projected as `status` even when
the author omits an explicit role. A visible dialog also contributes its
deepest otherwise-unmarked generic text leaves as bounded `text` objects. This
preserves dynamic success, failure, and explanatory messages from legacy or
underspecified widgets while avoiding a global scrape of arbitrary `div` and
`span` content.

The Collector continues to prefer authored paragraph, heading, alert, status,
list, and table semantics. Generic dialog leaves are deduplicated against those
objects, remain subject to the existing visibility and structural byte limits,
and receive no action authority. The rule is semantic and contains no site or
framework selector.

## 2026-08-06: Extension-only redaction; no MCP safety gate

Accepted: Saccade MCP does not classify data or actions, request confirmation,
or add a safety gate. Those decisions belong entirely to the calling LLM/Agent.
The Extension is the one product-enforced content boundary: password, SSN, and
EIN fields are protected, and SSN/EIN-shaped text is masked before observation
emission. Existing Agent-Off tabs remain unreadable. A user request to open a
known URL authorizes `tabs.open`; that new Agent-owned tab is Agent On without a
second popup step. This changes no wire version.

## 2026-08-10: Agent-selected Truth delivery and bounded reflex recovery

Accepted: `saccade.truth.read` exposes optional per-call `delivery_mode` values
`live` and `economy`, advertised through `saccade.capabilities/5`. `live`
preserves immediate pushed-revision delivery and remains the compatibility
default. `economy` uses a bounded 150 ms MCP-local coalescing window, then
returns the latest folded truthful delta. It does not filter the inventory,
remove current bounds, modify the Extension stream, or bind behavior to a model
or Profile. The LLM/Agent chooses freely on each call.

Accepted: the optional Reference Actuator treats dynamic prepared-action
invalidation as recoverable stale work rather than terminating the reflex run.
It waits for a newer Extension-pushed revision and retries within a 45 ms total
recovery budget. A missing newer revision still fails explicitly as
`recovery_exhausted`. This corrects the diagnostic loop without adding any
execution surface or latency policy to default Saccade Truth.

Accepted: Extension-dispatched soft reflex input is a semantic element action,
not a physical screen-coordinate action. It therefore requires current
document/object identity, opaque token, click affordance, visible non-zero
geometry, single use, and verified occurrence advancement, but does not apply
native-only viewport-coordinate, topmost, or browser-focus preparation gates.
Native reflex input retains every physical hit-testing check. This supersedes
older Reference Actuator language claiming identical physical preparation for
soft and native reflex clicks.

## 2026-08-11: Authenticated dogfood is Truth evidence, not a site integration

Accepted: a real authenticated workflow is valid engineering evidence when
Saccade supplies the authorized semantic Truth, the Agent client's own
same-tab browser tool owns execution, and success or failure is verified from
a newer observation. Complex forms, long pages, dialogs, permissions, and
server errors do not justify a site-specific module, selector, DOM path,
coordinate action route, or browser fallback.

CAPTCHA and restricted cross-origin frames remain explicit visibility or human
boundaries. Login/account mismatch, payment, publication, and other external
authorization states remain decisions or responses of the Agent, user, and
site; Saccade reports them without claiming completion. One-time URL secrets
may be supplied directly by the user to the Agent client for navigation, but
their secret query material is neither a Truth field nor a retained evidence
artifact. Single-browser authenticated dogfood does not promote a Control
Catalog row or compatibility claim to `publishable`.

## 2026-08-11: Live candidate identity is required after Extension installation

Accepted: an unpacked development Extension directory receives a reproducible
SHA-256 candidate identity derived from its shipped files. The loaded Service
Worker reports that identity in Native Messaging `hello`; every page Collector
reports the identity it loaded. Host rejects a missing or mismatched identity
when the installer pinned an expected candidate, and Worker refuses a stale
Collector. Disk replacement, Runtime restart, manifest version, and a
successful Host connection are insufficient by themselves.

Accepted: a Worker that already implements this contract checks the installed
candidate resource whenever it reconnects and calls its own supported
`chrome.runtime.reload()` path when the candidate changed. A Worker from before
this contract cannot retroactively execute the reload logic. Ordinary browser
restart does not prove unpacked-Extension activation; that legacy Worker needs
one explicit Reload from Chrome's Extensions page. The Agent client may perform
that development setting operation, but it is not a Saccade webpage execution
route. `attach` must fail explicitly until the live identity matches; it may
never label that state prepared or tested. The additive handshake and
capability fields retain
`saccade-extension-host/1` and `saccade.capabilities/5`.

## 2026-08-11: Zero-touch dogfood precedes setup hardening

Accepted: release work proves the product interaction before optimizing its
packaging. The next gate is a matrix of real objectives where the user gives
the task, the Agent performs ordinary webpage work through its own same-tab
tool, Saccade verifies every material result, and the user receives only the
completion result or one genuine human boundary. Findings from that matrix
then define the final `npx -y @saccade/setup` doctor and lifecycle requirements.

Accepted: the Extension popup is a small user-facing product surface, not a
diagnostic dump. It uses the existing blue-and-white Saccade icon, a white and
blue accessible palette, one current status, one concise explanation, one
contextual action, and quiet connection/session metadata. Development builds
show a restrained `DEV` badge derived from manifest identity; production
builds hide it. The popup does not add execution authority or new policy.

Accepted: the popup presents one authorization model for every tab. Internal
Agent-created and user-shared provenance may remain distinct for lifecycle
bookkeeping, but it is not a reliable statement about who most recently opened
or navigated a tab and therefore is not exposed as a user-facing restriction.
Every authorized tab shows `Stop sharing`; revocation removes both ACL
classifications, stops its Collector, and leaves the browser tab open. Closing
the tab remains an additional revocation path. This action revokes Saccade
Truth access only and does not claim authority over an Agent client's separate
browser execution tool.

Accepted: a Worker update may leave an already-open page with a static
Collector from the previous candidate. Background reconnect must not refresh
user pages. A later explicit `Share this tab` action first attempts ordinary
authorization; on a missing or stale Collector it performs one normal tab
reload, waits boundedly for the current candidate, and completes the requested
share. Saccade does not add `chrome.scripting`, accept a candidate mismatch, or
ask the user to repair this routine lifecycle state manually.

## 2026-08-12: Agent-owned temporary tabs have bounded cleanup

Accepted: dogfood showed that opening Agent-owned tabs without a matching
cleanup operation leaves the user's browser cluttered after otherwise complete
work. Default MCP therefore adds `saccade.tabs.close`, and `tabs.list` exposes
the Extension's `agent` versus `user_shared` ownership classification. The
capability schema advances to `saccade.capabilities/6`; the observation and
Native Messaging wire schemas remain unchanged.

The Extension accepts close only for a tab in its Agent-owned ACL. It rejects
user-shared, user-owned, unknown, and already-closed tabs without closing
anything. Successful close removes the tab ACL/session and retained Host Truth.
This is bounded lifecycle cleanup, not webpage execution or general browser
control.

Agent behavior closes research-only Agent-owned tabs when a task finishes. It
retains result pages the user may inspect or continue, unsaved or in-progress
work, tabs the user explicitly asked to keep, and every user-shared tab. No
timer or heuristic closes tabs independently of the Agent's task context.

Accepted: Service Worker replacement, development Reload, and Extension update
do not define the end of a browser session. The value-free ACL therefore uses
local Extension storage so it survives those worker lifecycles. A
`chrome.storage.session` lifetime marker makes the first Worker initialization
of a new browser launch clear the persisted ACL before its Host hello; the
possibly later `onStartup` event only ensures connection and cannot clear grants
created after Host readiness.
This prevents an update from silently converting still-open Agent tabs into
unknown tabs while keeping authorization session-only.

## 2026-08-13: `tabs.open` recovers a connected browser with no window

Accepted: Native Messaging can remain connected while Chrome or Edge was
started with no ordinary window. Chromium's implicit-window `tabs.create`
then fails with `No current window`, even though Saccade itself is healthy.
The Extension now selects an ordinary window explicitly and, when none exists,
creates one with the requested URL before recording the resulting tab as
Agent-owned. This is lifecycle recovery inside the existing Extension route;
it adds no fallback browser and requires no Chrome or Agent-client restart.
Closing the only tab in that recovered window commits revocation and queues the
successful response before Chromium tears down the last window; the window
removal event immediately rotates the Native Host connection for the next
task. A named MV3 alarm preserves that same-route retry across worker
suspension. This alarm carries no page data and adds no execution authority.

## 2026-08-13: Deferred tool registration is not browser-route absence

Accepted: an Agent client may keep MCP tools in a deferred or lazy registry
instead of expanding them in the initial tool list. Before any web operation,
the Agent must search that registry for Saccade and call
`saccade.system.capabilities`. A registered timeout is unhealthy Saccade, not
an absent tool. After one retry and same-route repair, a continuing failure
blocks the browser task; generic web search and other browser tools are not
automatic fallbacks. Results obtained through another route are research, not
Saccade dogfood.

## 2026-08-13: Compatibility reports separate recognition from test stimulus

Accepted: the public Truth diagnostic reports `recognition_rate` independently
from `closed_loop_rate`. A target present with truthful role, name, geometry,
and no action authority is observation evidence even when the optional
Reference Actuator lacks permission or cannot complete a public-page
transition. A blocked stimulus never becomes a closed-loop pass, and neither
number substitutes for client-owned Codex or Claude execution evidence.

The Chrome and Edge candidate `0.3.21` recognized 12/12 declared public targets
in each browser, and the test-only actuator completed the same 7/12 transitions
in each browser. All temporary Agent-owned case tabs were closed through
`saccade.tabs.close`. Edge passed only after its required macOS system Native
Messaging manifest was installed; a user-level manifest that Edge ignores is
not release evidence.

## 2026-08-14: Zero-window cold recovery uses one fixed Extension wake surface

Accepted, correcting the final paragraph of the 2026-08-13 no-window decision:
macOS Chromium may terminate the Native Messaging Host and stop scheduling the
MV3 worker after its last normal window closes. A Worker timer or alarm is not
accepted as proof of cold recovery in that state.

The Extension `hello` now supplies only its browser family, development flag,
and own `popup.html` URL. Host validates and persists that finite description
owner-only. When and only when `tabs.open` finds the Extension route
disconnected, the macOS MCP adapter opens that internal Extension URL in the
recorded Chrome/Edge application and retries the same owner-IPC request. The
lifecycle wake cannot receive the requested HTTP(S) URL, a selector, a script,
or a coordinate. Target navigation and authorization remain Extension-owned;
webpage execution remains Agent-client-owned. This is not a Playwright, CDP,
vision, or browser-execution fallback.

## 2026-08-14: Progressive Truth views are advisory, not filtering policy

Accepted: `saccade.truth.read` keeps its five-tool API and compatible `auto`
full-then-delta default while adding explicit `full`, `index`, and `region`
views in the MCP projection. The Extension and Host still retain and transport
complete current Truth. Index exposes bounded role counts, safe anchors,
regions, and honest byte/token estimates; it never claims completeness for a
region or removes the ability to request full Truth.

Region reads bind to the index document identity and revision and fail stale
after a page transition. Region sizing and recommendation thresholds are
implementation tuning points, not frozen protocol semantics or mandatory LLM
behavior. The Agent remains free to request full, index, region, live, or
economy on each call. Wire schemas remain `saccade.observation/1` and
`saccade-extension-host/1`; capabilities remain `/6`.

## 2026-08-14: Link targets close research discovery without adding execution

Accepted: a current `role: link` object may expose an optional
`navigation_target` when the Extension can resolve its authored destination to
a bounded HTTP(S) URL. The field is semantic page state under the existing
Extension disclosure boundary. It is not a selector, DOM path, action token,
coordinate, or authority. Target changes produce ordinary object deltas;
non-HTTP(S), invalid, credential-bearing, or oversized targets remain absent.

The Agent follows a target through the existing `saccade.tabs.open` route.
Saccade does not click the element, navigate arbitrary existing tabs, or add a
sixth default tool. Search titles and snippets establish discovery only. The
Agent must open and read the relevant source before presenting it as verified
or recommending it. It closes transient search tabs after the task but keeps
useful supporting source/result pages open for user inspection.

## 2026-08-17: Provisioned Agent-client tab claim inside `saccade.tabs.open`

Accepted: an Agent client may obtain Agent On for exactly one tab it created
itself, through a two-step claim expressed as `claim: "arm"` and
`claim: "confirm"` modes of the existing `saccade.tabs.open` tool.
`docs/reports/2026-08-17-same-tab-handoff-blocker.md` established the need: some
clients can act only in tabs they created, so a Saccade-created tab is
unusable to them and the closed loop cannot complete. The alternative — making
new tabs Agent On by default, or letting Saccade adopt a named tab — was
rejected because either would turn a per-tab human consent boundary into an
ambient one.

Design constraints and why each was chosen:

- **Short-lived.** The intent expires 30 seconds after arming, so a forgotten
  claim cannot silently authorize a tab minutes later.
- **Origin-bound.** The origin is declared before the tab exists, so the claim
  cannot be redirected onto whatever page happens to open next.
- **Single-use.** Every confirm attempt consumes the claim, success or failure,
  which removes retry as an enumeration technique.
- **First-match-only.** Only the first new tab that settles on the armed origin
  latches; a second candidate cannot be claimed by the same intent. This caps
  the blast radius of a claim at exactly one tab.
- **Uniform failure.** Wrong token, wrong identity, wrong origin, and expiry all
  return one message, so confirm is not a `tab_id` oracle.
- **No scanning.** Only `tabs.onCreated`/`tabs.onUpdated` payloads for tabs
  created after arming are inspected. Saccade never enumerates tabs, never reads
  an Agent-Off tab, and never returns the claimed `tab_id` to the caller — the
  Agent must independently supply the identity its own tooling produced.
- **Session-only.** A claimed tab carries `provenance: agent_client` and is
  revoked on Stop sharing, tab close or removal, Native Host disconnect, and
  browser startup. Only claimed tabs are revoked on Host disconnect.

Ordinary user tabs remain Agent Off, and `user_shared` semantics, the Profile
boundary, and protected-value redaction are all unchanged. The claim adds no
click, type, or execute capability. It stays generic: no model, vendor, or
client name appears in the wire contract. There are still exactly five public
MCP tools, and wire schemas remain `saccade.observation/1` and
`saccade-extension-host/1`.

## 2026-08-18: Default execution is bounded software-first with truthful handoff

Accepted, superseding earlier Truth-only and five-tool statements: default MCP
exposes six tools, adding `saccade.act`. The Agent addresses only an `object_id`
from current Truth together with its document and revision basis. Runtime keeps
the action token private; Extension dispatches only Registry-approved software
click, select, or type; and success requires a declared revision-bound semantic
transition. The default route never escalates to native input, Accessibility,
coordinates, screenshots, selectors, Playwright, or CDP.

Software input is not universally accepted by web applications. When a bounded
target state is present before and after and is provably unchanged,
`saccade.act` may return `external_execution_required` with `retry_safe: true`.
The Agent client may then act in the same tab and verify through Saccade Truth.
Editable `has_value: true` to `true`, generic buttons, links, and any result with
possible unobservable side effects are never marked retry-safe. No site-specific
compatibility rule is permitted.

Software preparation defers its scroll until the synchronous dispatch pass.
This prevents a preparatory scroll revision from invalidating the same action,
while retaining document, revision, token, affordance, and current-target
checks. Reference Actuator policy remains separate and generation-blind; only
the public route may disregard an automatically learned native rule from an
older Extension candidate.

## 2026-08-18: Public Truth delivery is a mandatory full-to-delta cursor

Accepted, superseding the 2026-08-14 progressive-view decision for the public
MCP surface. The first `saccade.truth.read` for a tab document returns complete
Truth. Every later read in that MCP session returns only the revision-bounded
delta from the last delivered cursor. Document replacement, a stream gap, or
loss of the compact history base automatically returns a new full reset. The
public schema no longer accepts `view_mode`, `full`, `index`, or `region`; this
behavior is a Runtime invariant rather than a prompt recommendation or LLM
choice.

`saccade.act` folds its post-action observation through the same cursor and
returns any additional semantic transition not already represented by target
verification. A model therefore does not reread the page after an ordinary
action. Runtime retains one current complete observation plus at most 256
compact journal entries containing revision metadata and changed identities,
not 256 complete page snapshots. The Extension continues to compile complete
current Truth and holds only its current Collector state. Wire schemas remain
`saccade.observation/1` and `saccade-extension-host/1`.

## 2026-08-18: Oversized initial Truth uses an automatic stable-ID catalog

Accepted, refining the mandatory cursor without restoring model-selected view
modes. A bounded initial projection remains one complete full view. When that
serialized full view would exceed the MCP response budget, Runtime returns a
complete compact catalog of all projected semantic objects instead of multiple
full-detail pages. Each entry carries its stable Agent-facing `object_id`, role,
bounded label preview, affordances, and visibility. The Agent may request full
current records for at most 64 relevant identities using the exact catalog
`document_id` and `basis_revision`; a stale basis fails closed and a detail read
does not advance the cursor.

This decision changes only MCP delivery. Extension and Host continue to retain
canonical complete Truth, and later ordinary reads remain revision-bounded
deltas. Runtime, not the model, deterministically selects full versus catalog
from the response-size bound. There is still no public `view_mode`, region
selector, all-tabs read, or all-tabs resync, and the wire schemas remain
`saccade.observation/1` and `saccade-extension-host/1`.

## 2026-08-18: Agent-facing updated objects use recursive merge patches

Accepted. The Extension continues to emit its canonical changed-object delta,
and Host continues to materialize complete current Truth. MCP now compares the
prior and current Agent projections and sends only changed fields for an
`updated` identity. The Agent merges the patch recursively; `null` removes a
field. `appeared` still contains a complete object, while `disappeared` contains
the stable `object_id`.

This removes repeated role, label, state, affordance, and geometry fields when
only one nested value changed. It also drops public updates caused solely by
private action-token rotation. The change stays downstream of
`saccade.observation/1` and `saccade-extension-host/1`; no Collector, Registry,
Profile filter, stable identity, or canonical Truth semantics change.

## 2026-08-19: Public act batches independent ordinary form edits

Accepted. The sixth public tool remains `saccade.act`; no seventh tool or new
execution backend is added. An Agent that has already read one Truth view may
send up to 32 independent editable, select, checkbox, radio, and switch edits
in one call. Runtime preflights one document/revision plan, rejects duplicate,
protected, unsupported, submit, navigation, and upload targets, then refreshes
each private token and revision before software-only dispatch. Every step must
produce its declared semantic evidence. The result contains value-free step
receipts and one final revision-bound transition.

Fair comparison drivers set a private fresh-policy flag so local remembered
native preferences do not change one lane's engine behavior. Capabilities
record that override in evidence. Production calls cannot express the flag,
and the user's policy file is neither removed nor rewritten.

## 2026-08-19: Semantic working sets and action-scoped transitions

Accepted. The existing `saccade.truth.read` tool may carry a bounded semantic
query over text, roles, affordances, visibility, and root/all frame scope.
Runtime returns at most 32 complete Agent objects plus frame summaries as a
`working_set`, advances the same exact-tab cursor, and retains the complete
canonical observation locally. The query is not a DOM selector, does not run
inside the page, and adds no tool, browser, CDP, or execution route.

The task-oriented default includes rendered offscreen controls while excluding
hidden and unknown objects. `min_objects` plus an optional bounded timeout lets
Runtime absorb initial hydration before returning once. For dynamic controls,
an exact-label follow-up query is projected from the latest canonical
observation and acknowledges older queued ambient geometry pages; the model
does not drain them. Clicking a select may be verified by its `expanded` state,
while choosing an option remains verified by that option's `selected` state.
Every whitespace-separated text query word must occur in the safe name, text,
description, or bounded nearest-preceding heading projection. This lets a named
section/example disambiguate duplicate controls without adding a selector
language or changing Extension collection.

For `text_any`, Runtime selects one distinct match for every phrase that has a
match before filling the rest of `max_objects` in document order, and returns
the per-phrase match counts. This prevents one noisy phrase near the top of a
large real page from truncating later named targets. It changes only the
bounded Runtime projection; canonical Truth, Extension collection, cursor
semantics, and wire schema versions do not change.

A verified action receipt, including `all_verified` batches, no longer carries
same-frame structural churn or an ambient pending count. Those changes remain
queued but silent because the receipt is already complete proof. Unverified
actions retain same-frame appeared/disappeared evidence where it can explain
the side effect.
Every batch result also carries its final `document_id`, `revision`, and
`next_basis_revision`, eliminating a stale-basis recovery read before a
separate submit or navigation action.

Superseded on 2026-08-19: Profile behavior remains mandatory, but it is now
delivered once by `system.capabilities`; initialize carries only the compact
route/loop invariant.

`saccade.act` now separates causally useful receipt content from ambient page
churn. Verified target state remains in the compact receipt. Unverified
actions may return appeared or disappeared objects from their target frame;
unrelated updates and frame metadata are queued for the ordinary Truth cursor. This
changes only MCP delivery: Extension/Host Truth, stable identity, Profile
filtering, protected-value rules, and both wire schema versions remain intact.

## 2026-08-19: Public MCP schemas avoid top-level composition

Accepted. Claude's tool registry rejected the sixth public tool before its
first Saccade call because the `saccade.act` input schema used a top-level
`oneOf`. The public schema now exposes the same bounded fields without
top-level `oneOf`, `allOf`, or `anyOf`. Runtime remains authoritative for the
mutually exclusive single-action and batch forms and rejects missing, mixed,
or operation-incomplete requests exactly as before.

This is a Runtime/MCP compatibility fix only. It adds no tool or execution
route, does not change the six-tool API, Extension candidate, Profile boundary,
protected-value behavior, or either wire schema. A real Claude Opus 5 low
registration smoke and the complete three-task/two-order Claude matrix passed
after the installed Runtime was rebuilt.

## 2026-08-19: Root frame identity follows the frame relationship

Accepted after public IGN dogfood. Runtime previously treated a frame as root
only when the observation contained exactly one frame. IGN composes same-origin
child frames, so its actual top document was projected as `root:false` and a
correct `frame_scope: root` query returned zero matches even though canonical
Truth contained more than 180 root objects.

Runtime now selects the unique frame whose `parent_frame_id` is absent. Multiple
root candidates remain ambiguous and do not receive a fabricated default. The
change is generic and Runtime-only: no IGN selector, browser branch, Extension
candidate, protocol field, or wire-version change was added. A multi-frame unit
test and a real IGN root-only query prove the fix.

## 2026-08-19: Current Truth affordances compile directly into actions

Accepted. `saccade.act` no longer requires an Agent to restate an operation
already determined by the selected object's current Truth. When `operation` is
omitted, Runtime resolves the Agent-facing `object_id`, reads that exact
object's current affordances, and compiles its sole supported click, type, or
select operation. A text/value payload implies type and `option_object_id`
implies select. Explicit operation remains compatible and is required only
when several supported affordances remain.

Inference is fail-closed: it cannot create an affordance, bypass Registry
validation, use a stale alias, or add action authority. The Extension and both
wire schemas are unchanged, and the Truth response gains no duplicate action
field. A real Chrome probe verifies button, text-field, and select loops without
sending `operation` in any call.

## 2026-08-19: Dynamic semantic queries accept a revision lower bound

Accepted after the operation-inference A/B exposed the same failed call in five
of six successful lanes. Agents naturally combined `after_revision` from a
verified action receipt with a bounded query for newly revealed or replaced
controls, but Runtime rejected that combination and forced a second unbounded
query.

`truth.read` now accepts `query` with `after_revision`. If the exact-tab MCP
cursor already holds that revision or newer, Runtime projects the working set
directly from its canonical snapshot. Otherwise it waits locally for one newer
observation and then applies the same bounded query. This changes only the MCP
delivery projection: Extension collection, stable IDs, action authority,
wire-schema versions, and the six-tool surface remain unchanged.

## 2026-08-19: Compact MCP contract and local actionability waiting

Accepted. MCP initialize contains at most one compact route/loop invariant and
never embeds Profile behavior. `system.capabilities` delivers the active
Profile behavior once with `behavior_delivery: capabilities_once`, while the
Runtime exposes deterministic `runtime_version`, `mcp_contract_hash`, and
`profile_digest` identities. Setup release state and doctor pin the contract
hash, and a tool/schema change requires a new Agent session rather than a false
hot-refresh claim. The six public tools and both wire schema versions remain
unchanged.

Accepted. Registered software actions retain their immediate zero-wait path.
Only a transiently non-actionable or animating target enters a bounded
Collector-local wait using the existing timeout. Two stable animation-frame
geometries and fresh visible/topmost/focus/enabled/token/document checks are
required before dispatch. Semantic authority, identity, document, or token
replacement fails stale and never rebinds. Public results distinguish prepare,
dispatch, and verify failures with a stable code and truthful `retry_safe`.

Accepted. Fair benchmark reports separate control-plane, discovery,
steady-state, model-usage, stability, and infrastructure accounts. A 529,
rate limit, timeout, zero-tool run, or contract mismatch invalidates evidence
and can never score a lane loss. The long-horizon gate generates unknown oracle-checked
queues at lengths 1/5/10/25/50 for same-identity, replacement, and navigation
changes in both lane orders; no performance conclusion is authorized until the
complete matrix is valid.

The generated queue's unique success marker is emitted only by its independent
oracle after every correct action. Tool output containing every required marker
is therefore the completion ground truth for both lanes. The model's final
`completed` field is retained as a consistency diagnostic but cannot overturn
oracle proof or manufacture proof that is absent.

Accepted. An explicit compatible operation may enter the bounded local wait
when a known Registry role is temporarily disabled and therefore does not yet
advertise its affordance. Omitted operation still fails closed because Runtime
cannot infer absent authority. During that wait only `enabled: false → true`
may rebase; any other semantic, identity, token, or document change remains
stale.

Accepted. If a local MCP action cursor is missing after an earlier exact-tab
read, Runtime may restore it from one fresh Host snapshot only when the tab and
`document_id` match exactly. It does not carry aliases across navigation or
replacement. Compact action results may surface at most two bounded text/name
signals already present in public Truth, allowing an inline transition to prove
completion without another full or delta read. The compact result explicitly
marks `follow_up_read_required=false` whenever it already carries a transition.
Editable and protected values remain absent.
