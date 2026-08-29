# Node-only release conformance first pass

Date: 2026-08-27

Status: Chrome and same-candidate Edge engineering passes complete, including
live two-Agent isolation; the authenticated Steamworks handoff remains open

## Route

- MCP server: `saccade-node` 0.2.0
- Runtime: Node.js
- Extension: connected
- Rust: absent
- Native Host: absent
- Browser operations: Saccade only
- Retained secrets, editable values, screenshots, cookies, or storage: none

The initial capabilities response did not expose the attached browser family or
Extension candidate digest, so the first pass could not satisfy the final
frozen Chrome/Edge same-candidate claim even though the connected Extension
instance completed the work. The Node Broker now validates and reports both.
A live post-restart `saccade.capabilities/8` response proved the attached
browser was `chrome` and its exact candidate was
`af09e65c83d1ea38ebdf6abe88804cd056e2f57a0492c8de4f3365df1f1943d7`
version `0.4.0`. Edge later connected concurrently with Chrome using that exact
candidate and completed the core execution lanes recorded below.

## Passed

| Gate | Result |
| --- | --- |
| Same-origin iframe plus restricted iframe | 2 observed frames, 1 restricted frame; nested button observed |
| Open and closed Shadow DOM boundary | open-shadow button observed; opaque descendants did not leak |
| Semantic table projection | table, row, and cell projected |
| Independent form batch | 3 steps accepted and verified in 252.319 ms |
| Contenteditable software action | accepted and verified as a separate action |
| Normal-control mouse accuracy | 24/24 hard targets; mean 124.169 ms, p95 127.626 ms, max 128.827 ms |
| Best Buy Truth | Deal of the Day and Top Deals found in 2769.712 ms |
| NanMesh Truth | target identity found in 555.662 ms |
| Nanlogic Truth | NaNDesk and CTA found in 802.235 ms |
| Mythcast Era Truth | section and waitlist action found in 449.017 ms |
| Angular Material select | Basic Favorite food opened; stale option basis rejected safely; fresh Pizza option accepted and verified selected |
| Selenium form batch, one run | 5 independent edits/toggles accepted and verified in 226.967 ms; Submit not clicked |

## Blockers and gaps

1. Canvas/reflex accuracy dispatched the first target but did not observe the
   occurrence before the 5-second deadline. Receipt was `outcome_unknown`,
   `occurrence:dispatched`, `verification_timeout`, `retry_safe:false`. It was
   not retried.
2. A background-to-offscreen checkbox action was functionally verified, but its
   receipt carried geometry updates for all 63 page objects, roughly 20 KB.
   Opening Angular Material's overlay produced a roughly 64 KB receipt. These
   are not compact relevant deltas.
3. A one-object Angular semantic query still returned the full page authority
   list. Query bounding does not currently bound authority transfer.
4. The public `truth.read` MCP schema lacks `min_objects` and `timeout_ms`.
   Async iframe and client-rendered pages therefore required the runner to
   combine delta waits with another full semantic query instead of asking the
   Broker for one semantic readiness condition.
5. A labeled contenteditable fixture projected no semantic name.
6. DemoQA React fields projected their placeholders only as descriptions and
   no names. They can still be distinguished value-free by description, but
   the React batch later failed during dispatch with
   `operation_not_registered` and `retry_safe:false`; no Submit occurred and
   the failed action was not replayed.
7. Selenium affordances/action authorities were intermittent across fresh
   runs: one batch passed, while other fresh reads briefly exposed enabled
   controls without affordances and were rejected before dispatch. This needs
   a pushed readiness/authority stabilization gate.
8. Public Selenium and DemoQA Submit controls were not clicked. That exact
   external submission requires explicit owner approval.

## Optimization pass after the first run

The following first-pass gaps are fixed in source and covered by focused Node
and Extension tests:

- semantic working sets now scope `authorities` and `changes` to returned
  identities;
- `truth.read` publicly exposes `min_objects` and `timeout_ms`, and the Broker
  waits on pushed Truth revisions instead of model polling or fixed sleep;
- known `object_ids`, roles, affordances, visibility, and safe text can bound a
  working set without creating a locator language;
- verified action receipts exclude unrelated whole-page geometry churn;
- form batches still preflight completely, then revalidate each exact current
  token before dispatch so framework rerenders do not reuse a stale prepared
  DOM target;
- a batch that fails after earlier steps dispatched now returns
  `outcome_unknown`, `partially_dispatched`, and `retry_safe:false` with
  value-free step receipts; it cannot be mistaken for a safe whole-batch retry;
- public MCP schemas no longer use top-level schema composition; mutually
  exclusive `tabs.open` and `act` forms are checked strictly in the Broker.

Accessible naming, loading-page authority readiness, attached browser-family
reporting, and the Edge same-candidate rerun remain open. The canvas/reflex
occurrence gap is closed by the official Classic evidence below.

The earlier 24/24 hard mouse-accuracy result is a local authored fixture, not
the official MouseAccuracy Classic maximum. A later Saccade-only inspection of
`https://mouseaccuracy.com/classic/` confirmed the Classic choices include
`Epic` spawn speed and `Tiny` target size. The site implements those choices as
visually hidden native radios with visible associated labels, which exposed a
generic recognition gap: Saccade used the native radio for both semantic state
and geometry, then omitted it as hidden. The candidate now keeps radio identity,
checked state, and enabled state on the native input while deriving actionability
and bounds from its visible associated label. A focused segmented-radio fixture
and Extension tests cover the split without a site-specific selector. The
later post-reload run below supersedes the initial pending state.

## Post-reload live evidence

After restarting the previously day-old long-lived Broker, the new MCP schema
and receipt projection were observed through Saccade itself:

- a three-field software batch completed accepted/observed/verified; its receipt
  fell from 11,687 bytes with 16 unrelated objects to 3,189 bytes with exactly
  the three acted objects;
- an offscreen checkbox scrolled locally, executed, and verified in one action;
  its receipt was 1,291 bytes with one relevant object, instead of the earlier
  roughly 20 KB whole-page geometry receipt;
- `truth.read` satisfied a single `min_objects` request and returned a bounded
  semantic working set without model polling;
- DemoQA React safely rejected before dispatch with the newly preserved
  `prepare/stale_action_basis` diagnostic. No form submission or other side
  effect occurred.

The React diagnostic identified one more lost invariant: visibility and
transition were incorrectly included in the authority fingerprint, so merely
scrolling the same connected DOM element rotated its token. Source now treats
those fields as local actionability inputs while retaining strict invalidation
for replacement, role, affordance, enabled/state, document, and token changes.
That final Extension change requires another manual reload before the React
batch can be rerun.

After that reload, the seven-step DemoQA React batch passed completely:
accepted, observed, and verified, with seven relevant objects and seven
relevant changes in a 6,543-byte value-free receipt. Submit was not clicked.
The former `stale_action_basis` failure is closed.

Angular Material's overlay-open action also passed with a 1,366-byte receipt
and one relevant object, replacing the earlier roughly 64 KB result. Selecting
Pizza then dispatched but produced no page transition; reconciliation proved
the select remained expanded and Pizza remained unselected at the same
revision, so the action was not replayed. The remaining cause was the legacy
ARIA-select execution policy: synthesized Home/ArrowDown/Enter keydowns were
not accepted by Angular Material. Source now waits locally for the exact bound
option to be current, enabled, visible, topmost, and geometrically stable, then
dispatches the same bounded software pointer/click sequence to that option.
Replacement remains stale and no coordinate authority is exposed to the Agent.

After reloading that candidate, the clean Angular rerun passed. Opening the
Basic Favorite food overlay was accepted/observed/verified in a 1,366-byte
receipt. Selecting Pizza was then accepted/observed/verified in a 2,009-byte
receipt: the select became `has_value:true` and `expanded:false`, while Pizza
became `selected:true`. The Agent continued to receive current document- and
viewport-relative object bounds as Truth; only arbitrary-coordinate execution
remained outside its authority. The test tab was closed.

## Official MouseAccuracy Classic Epic/Tiny evidence

The Node-only route opened a fresh leased
`https://mouseaccuracy.com/classic/` tab, selected `Epic` and `Tiny` in one
fully preflighted two-radio batch, and started a bounded reflex run through the
existing `saccade.act` tool. No selector, locator, arbitrary coordinate,
Playwright, CDP, native driver, or Rust route was used.

- Extension source candidate:
  `af09e65c83d1ea38ebdf6abe88804cd056e2f57a0492c8de4f3365df1f1943d7`
  (`0.4.0`). The current capabilities schema still does not echo the installed
  candidate digest, so this remains operator-attested rather than a
  cryptographic same-candidate proof.
- The options batch was accepted, observed, and verified. Truth showed both
  `Epic` and `Tiny` checked.
- One bounded request completed 49 exact current-target dispatch occurrences
  with zero stale retries. Occurrence advanced monotonically from `0` to `49`;
  the bounded report was accepted and verified.
- The site's final semantic result independently reported
  `You clicked 49 targets.` and `You misclicked 0 times.` The final page score
  exactly matched the Extension occurrence count.
- Classic exposes its live click count only in page-private JavaScript and on
  the final result screen; it does not retain `.target.hit` or another live DOM
  counter. Its dedicated bridge therefore treats completion of the exact
  current target's software click dispatch as the per-step occurrence, then
  uses the final semantic result as the aggregate page-owned check. An
  ambiguous dispatch is still never replayed.

This closes the official Classic maximum-difficulty Chrome engineering gate.
Runtime reporting of the attached browser family and candidate digest is now
closed by `saccade.capabilities/8`; the release gate remains open only for the
identical candidate's Edge execution. Cleanup also exposed an independent
reload-ACL gap: the fresh test tab closed normally,
but two older reloaded tabs still appeared as `ownership:agent` through the
Broker while the Extension rejected `tabs.close` because it no longer regarded
them as Agent-owned. They were left open and were not retried. Broker and
Extension lease metadata must reconcile this state before the reload/close gate
can pass.

## Current MouseAccuracy Insane/Tiny evidence

A fresh leased `https://mouseaccuracy.com/game` tab started the current
30-second challenge directly. Initial Truth showed five simultaneously moving
targets shrinking from roughly 22.8 CSS px to 8.2 CSS px. The result page later
confirmed the exact settings were `Insane`, `Tiny`, and `30 Seconds`.

The first bounded request was safely rejected before dispatch because moving
target geometry advanced canonical revision from 180 to 881 between read and
act. No click or score occurred. This exposed an over-strict gate specific to
the non-actionable reflex controller: it was requiring the whole page revision
to remain equal even though the same document-local controller object and loop
authority remained current.

The Node Broker now permits only that dedicated controller form to rebase onto
newer canonical Truth when `document_id`, exact `object_id`, role, and current
`loop_class_token` still resolve uniquely. Every target dispatch still uses its
exact current action token and revision. Document replacement, missing or
replaced controller identity, future basis, target replacement, or ambiguous
dispatch still fails; ordinary actions retain strict revision matching.

After the focused Broker test passed and the same MCP session recovered across
a Node Broker restart, a second fresh tab completed the full challenge:

- bounded report: accepted and semantically verified;
- exact target actions: 86;
- stale retries: 0;
- total score: 3440 (`1720 pts + 1720 bonus`);
- target efficiency: 90%, with 86 of 96 targets hit;
- click accuracy: 100%, with 86 of 86 clicks accepted and 0 click misses;
- maximum combo: 86.

The live occurrence is the page's score, which advanced by the page-defined
Tiny-target point value rather than by one per hit. The bounded report's 86
actions independently matched the result page's 86 clicks and 86 hits. No
Playwright, CDP, arbitrary coordinate, native driver, or Rust route was used.

The remaining ten misses were a launch race, not target inaccuracy: opening
`/game` started the page timer before the Agent could read the controller and
submit the bounded request. The repaired launch form begins from the homepage's
explicit same-origin `START` link. One `saccade.act` deadline now covers the
verified start click, the same-document application route, discovery of the
new controller authority, and the complete 30-second run. It does not renew the
deadline at any layer or reuse pre-navigation controller authority.

A fresh leased homepage tab then completed the same current challenge:

- bounded report: accepted and semantically verified;
- exact target actions: 96;
- stale retries: 0;
- total score: 3920 (`1960 pts + 1960 bonus`);
- target efficiency: 100%, with 96 of 96 targets hit;
- click accuracy: 100%, with 96 of 96 clicks accepted and 0 click misses;
- maximum combo: 96;
- settings: `Insane`, `Tiny`, and `30 Seconds`.

Two earlier launch diagnostics safely returned
`start_controller_unavailable` with zero target actions while the
same-document route predicate was being narrowed. Neither start nor target
work was automatically replayed. The final successful request used one
35-second end-to-end deadline, providing bounded startup overhead around the
30-second game without changing its rules.

## Edge same-candidate live evidence

Edge connected concurrently with Chrome and reported the exact same Extension
candidate digest
`af09e65c83d1ea38ebdf6abe88804cd056e2f57a0492c8de4f3365df1f1943d7`
at version `0.4.0`. The first Edge open returned browser instance
`browser.772861aa480f3455d3d75e74cc3984b0fff67c7039dcf613`, matching the
machine-readable Edge connection in `saccade.capabilities/8`.

Concurrent attachment exposed a deterministic-routing gap: `tabs.open` chose
the most recently connected Extension when no browser was named. The Broker and
six-tool MCP schema now accept exact `browser_instance_id`; omission fails
before dispatch with bounded `AMBIGUOUS_BROWSER` candidates whenever multiple
browsers are online. Focused tests proved that the command is delivered only to
the selected Extension and that the resulting lease retains that instance.

Using only that exact Edge route:

- same-origin iframe plus restricted iframe and open Shadow DOM Truth passed:
  two observed frames, one restricted frame, and both expected buttons;
- table Truth projected one table, one row, and two cells;
- a three-field local form batch was accepted, observed, and verified in one
  request;
- the default current MouseAccuracy profile completed 52/52 targets and 52/52
  clicks with no misses, but was not counted as maximum-difficulty proof;
- after explicitly changing Edge-local settings from Normal to Insane and
  Medium to Tiny while keeping 30 Seconds, one launch-and-loop request completed
  96 exact actions with zero stale retries; the page independently reported
  96/96 targets, 96/96 clicks, and score 3920;
- DemoQA's React practice form completed one seven-step batch: five ordinary
  fields, Male radio, and Sports checkbox all became value-present/checked and
  were verified. Submit was deliberately not clicked.
- Angular Material's offscreen Basic `Favorite food` mat-select was activated
  locally, exposing one visible Pizza option. Strict semantic resolution found
  exactly one option, and one select action was accepted, observed, and
  verified with the control changing to `has_value:true` and `expanded:false`.
- with an active Edge lease, the exact Node Broker process was terminated. The
  restarted Broker preserved the Agent session and lease as `awaiting_truth`;
  both browsers reconnected, the same Edge tab and document returned through a
  fresh full Truth at revision 1, and a subsequent current-object action was
  accepted and verified. No action was replayed during recovery.
- `fixtures/controls/replacement_stale.html` replaced a target with a new DOM
  node under the same visible label. Acting on the old object at the current
  revision failed before dispatch with `OBJECT_UNKNOWN`; a fresh Truth exposed
  the replacement under a new object identity and action token. The old
  authority was never rebound.
- an independent Claude Code process connected through the current repository's
  Node MCP as Agent B while Agent A retained a Chrome lease. Its session ID
  differed from Agent A, `tabs.list` did not expose Agent A's tab, and a direct
  full-Truth attempt against that tab was denied as `Tab is leased to another
  Agent`. Agent B opened its own exact Edge instance, typed into one current
  ordinary field with an accepted, observed, verified receipt (revision 1 to
  2), closed only its tab, and ended with an empty lease list. Agent A then
  proved its original tab and untouched revision 1 Truth were still readable
  before closing it.

The remaining release lane is the explicit authenticated Steamworks upload
handoff. It requires an owner-authenticated tab, a real asset, and an explicit
final-submit decision. This section does not claim that lane passed.

## Cleanup

All tabs currently leased to this Agent session were closed after the Edge
rerun, and `tabs.list` returned an empty list. Earlier ownership-mismatch close
rejections remained safe: no close or action was replayed after rejection, and
no ambiguous action was replayed anywhere in the run.

## Final automated verification

- Node Broker and MCP: 48 passed, 1 environment-only loopback-listen skip;
- Extension: 53 passed, 0 failed;
- live two-independent-Agent tab isolation: passed with Codex Agent A and
  Claude Code Agent B;
- single-production-architecture check: passed;
- Extension 0.4.0 release packaging: passed;
- Markdown authority integrity and patch whitespace checks: passed.
