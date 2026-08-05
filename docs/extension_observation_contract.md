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

## Object projection

A projected object may expose:

- stable document-local object identity;
- role and accessible name;
- safe role-specific state;
- semantic affordances;
- frame and semantic provenance;
- truthful limitations.

It must not expose locators, DOM paths, editable contents, protected values,
cookies, browser storage, or arbitrary coordinates. Default MCP additionally
removes optional action tokens and internal authorities. Profile bans are
applied before projection and cannot alter recognition semantics;
`PROFILE_ARCHITECTURE.md` remains normative for that boundary.

Control modules are indivisible semantic modules: each recognizes one control
family and consistently projects its role, name, safe state, affordances, and
limitations across supported native HTML, ARIA, and framework lifecycles.
Execution primitives and verifiers are not part of this contract.

## Full and delta views

The first view of a document is full. Later views carry only Extension-compiled
`appeared`, `updated`, and `disappeared` objects, together with document,
viewport, and semantic revisions. Stable aliases remain stable within one
document. Dynamic replacement receives new internal identity and is reported
as disappearance plus appearance; it is never silently treated as the old
object.

The Host keeps bounded history. `truth.read(after_revision)` waits locally and
folds only source-declared changes after that revision. If history cannot prove
continuity it returns a full gap reset. Resource subscribers receive only an
updated URI notification and then read the same full/delta stream; notifications
do not repeat the page.

## Structure and visibility

The top collector composes accessible same-origin iframe documents and open
shadow roots. Descendants retain frame or shadow provenance. Cross-origin or
otherwise inaccessible frames and closed shadow roots are reported as limited
or opaque rather than guessed.

Visibility follows rendered semantic availability, including lifecycle events
that finish transitions or animations. Mutation, relevant attribute, viewport,
focus, frame, and registered semantic-bridge changes schedule compilation.
Canvas/WebGL surfaces remain opaque unless an approved bridge supplies stable,
revalidatable semantic objects and changes.

## External execution observation

Codex, Claude, or another Agent acts with its own tool in the same authorized
browser tab. Saccade does not prepare, dispatch, or accept that action. It only
observes the resulting browser state and pushes the corresponding semantic
transition. If the Agent's tool cannot control the same browser instance, the
integration is incompatible.

The optional `reference-actuator-mcp` may consume internal revision-bound
authority for regression and compatibility testing. That interface is
`saccade.reference.*`, loads native permissions lazily, and marks every receipt
with `reference_actuator` provenance. It is outside the default Truth API.

## Required tests

Extension tests cover all catalogued role/name/state/affordance projections,
Profile bans, full→delta, dynamic replacement, same-origin iframe, open Shadow
DOM, delayed render, and stream gaps. MCP tests prove the default four-tool
surface, absence of action authority, blocking revision reads, and unsolicited
resource updates. Default installation must pass without Accessibility.

The local Chrome and Edge gate covers the machine inventory but is not public
web compatibility evidence. Source-diverse public cases must retain truthful
limitations and failures; they may not be made to pass with site-specific
selectors or an execution fallback.
