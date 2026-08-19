# Open findings from public and Playwright testing

Date: 2026-08-03

This ledger records observations before root-cause research or further code
changes. A row is not a confirmed product defect until the evidence separates
Collector behavior, public-site drift, and client execution behavior.

| ID | Area | Observation | Current classification | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| F-001 | Codex Computer Use | An expanded W3C APG menubar remained the active Accessibility tree after switching tabs and required a clean browser restart. | client same-tab incompatibility | `/private/tmp/saccade-public-20260803/w3c-menu-codex.json` plus Computer Use trace | open |
| F-002 | Angular Material | Official examples intermittently render only the documentation shell in both Saccade and Chrome Accessibility. | public-site drift / intermittent render | `docs/reports/2026-08-03-test-first-progress.md` | open |
| F-003 | Benchmark timing | Historical Selenium Codex lane proves completion but lacks one trusted end-to-end clock and token accounting. | measurement limitation | `/private/tmp/saccade-fair-selenium-codex-retained.json` | open |
| F-004 | Codex Computer Use / DemoQA | Clicking the exposed Sports checkbox did not change state; clicking its visible text label did. | client hit-target or site event-binding incompatibility | `/private/tmp/saccade-fair-20260803/demoqa-codex-raw.json` plus Computer Use trace | open |
| F-005 | Saccade measurement / DemoQA | Initial full Truth was an empty shell and the React page hydrated through 86 pushed views before success; completion worked, but discovery/token cost needs analysis. | Collector emitted authority/geometry-only collections as public revisions; the first empty full remains a required `tabs.open` lifecycle observation | `/private/tmp/saccade-fair-20260803/demoqa-codex-raw.json`; fixed load trace `/private/tmp/saccade-demoqa-fix-load-20260803.json`; Chrome/Edge regression evidence under `20260803T191428Z` | churn fixed; one cross-document lifecycle empty remains |
| F-006 | Saccade identity/state / Angular | Opening Basic select produced a transient second `Favorite food` select with `has_value:true`; after close it disappeared while the original remained `has_value:false`. The `Pizza` option nevertheless remained `selected:true`. | temporal choice-owner relation was discarded before the overlay lifecycle ended | `/private/tmp/saccade-fair-20260803/angular-codex-raw.json`, revisions 23–25; fixed trace `/private/tmp/saccade-angular-fix-action-20260803.json`, revisions 7–9 | fixed and publicly reproduced |
| F-007 | Fair benchmark | All three `Saccade-first` pairs completed, but order-reversed pairs have not run. Saccade client-native traces also lack model-token accounting and use a later timing origin than Codex Playwright lanes. | comparison limitation | `/private/tmp/saccade-fair-20260803/*/report.json` | open |
| F-008 | Optional Reference Actuator | Final-candidate actuator regression reached the `Email` input but native type dispatch returned `permission_required`, despite repair reporting Accessibility trusted. | optional executor permission/routing issue; core Truth unaffected | `~/Library/Application Support/Saccade Dev/evidence/20260803T191716Z/chrome/controls.json` | open; investigate only if retaining actuator support |
| F-009 | Fair benchmark ordering | `benchmark_agent_fair.py` accepted `playwright-first`, but always loaded completed client-native Saccade evidence before invoking Playwright and did not validate lane timestamps. | comparison harness defect | focused ordering tests in `tests/test_benchmark_agent_fair.py`; final first-pair reports under `/private/tmp/saccade-fair-final-20260803` | runner fixed; real order-reversed reruns pending |
| F-010 | Codex Saccade MCP lifecycle | A Codex task lost usable Saccade calls while development Runtime/browser cycles were running. The MCP adapter previously required a live grant/Host during initialization, so a temporary absence could make the client discard the task-owned MCP process. | generic MCP/Host lifecycle defect, compounded by the old harness rewriting live Codex MCP configuration | 2026-08-04 and 2026-08-05 task traces; Kickstarter dry run failed before navigation; real Unix-socket regression now proves one client starts before the grant exists and survives socket/capability rotation; MCP regression proves initialization succeeds while Host is absent | source root fix implemented and locally green; installed-candidate proof remains open until one unchanged Codex task survives real Runtime/Extension loss and recovery and completes the Kickstarter pre-navigation loop |
| F-011 | Codex Computer Use / Chrome for Testing | Computer Use refused the Saccade-managed Chrome for Testing app by display name, exact bundle id, and full application path. The test launcher intentionally disables every extension except Saccade. Loading the installed GPT store-extension directory as a second unpacked extension changed its identity, so its native host rejected it; this is not a valid workaround. | test-browser/client same-tab integration incompatibility | 2026-08-04 Angular task trace and Chrome native-messaging log | resolved for client-native testing by using an ordinary Chrome profile with both extensions installed |
| F-012 | Angular initial Truth size | The official select examples project 171 objects and about 40 KB / 10k estimated tokens before Profile filtering. The projection is complete, but it is larger than the “see and immediately understand” product target. | semantic prioritization/compression gap; not a correctness failure | 2026-08-04 Angular direct-MCP measurement | open; research bounded overview/profile strategy without hiding denominator items |
| F-013 | Dialog modality semantics | Opening the official Angular dialog pushes its heading and action buttons and removes background content from the active semantic view. The existing protocol already permits bounded `state.modal`; the Collector now projects it on the forced dialog heading without adding a role or changing the wire schema. | missing projection in the Collector | 2026-08-04 Angular dialog revisions 6–8 plus clean-profile Chrome/Edge pushed-delta gate `20260804T230400Z` | fixed; official Angular truthfully reports `modal:false` because its example does not author `aria-modal=true`, while the focused fixture proves `modal:true` appearance and disappearance |
| F-014 | PrimeVue initial observation | The first current-candidate Chrome open of the official Select page timed out before the first observation. The Collector suppressed every observation while `document.readyState` remained `loading`, so the normal authorization path could not satisfy the Runtime's bounded first-observation wait. | generic loading-state liveness defect, amplified by a dirty persistent test profile with many restored public tabs | failing retained runs on 2026-08-04; after removing the experimental watchdog/retry patch, a fresh-profile root-cause proof produced 5/5 first observations in 1.44–2.03 s and complete 336–337-object/27-select Truth 155–217 ms later; final dual-browser gate `20260805T005946Z` | fixed at the Collector boundary; loading pages publish non-actionable bounded Truth and recompile after DOMContentLoaded; no retry loop, site branch, or timeout increase |
| F-015 | Legacy `frames` harness | `./scripts/dev.sh frames all` recognized the same-origin frame and open-shadow buttons, then failed because the harness attempted Reference Actuator native clicks and received `permission_required`. The core semantic gate already proved frame/shadow composition without execution. | obsolete optional-actuator harness boundary; core Truth unaffected | failing Chrome evidence `20260804T231118Z`; repaired Chrome/Edge evidence `20260804T232323Z` | fixed; frame/shadow command now uses core MCP observation only and reports `execution_owner: agent_client` |
| F-016 | Codex / managed-browser same-tab route | In the 2026-08-05 lifecycle continuation, Saccade `tabs.open` created the public page in the managed Chrome for Testing instance, while Codex's installed Chrome execution extension exposed only ordinary-Chrome tabs. The Agent therefore could not act in the observed tab. | client same-tab incompatibility in the automated public-test route; not a Truth defect | dynamic-loading tab `294227834` was readable through core MCP but absent from the fresh Codex Chrome `openTabs()` listing | open; public action transitions remain blocked and no execution fallback is permitted |
| F-017 | Chrome delta completeness / latency stability | The original probe overwrote one object every 80 ms but required every intermediate value even though `truth.read(after_revision)` folds retained revisions into current Truth. Rendering-frame scheduling also allowed genuine semantic delivery tails in throttled tabs. | mixed harness-contract defect plus generic Collector scheduling defect | failing evidence `20260805T111727Z` and `20260805T111826Z`; corrected five-run completeness evidence `/private/tmp/saccade-f017-harness-{1..5}.json`; folded batch evidence `/private/tmp/saccade-f017-batches.json` | completeness root fixed in fixture; Collector semantic scheduling fixed independently of paint; strict clean-profile latency matrix still exposes environment/MCP tail variance and remains open |
| F-018 | Ordinary-Chrome automatic dogfood | After the managed browser released Native Host ownership, ordinary Chrome exhausted five reconnect attempts and never recovered. The workflow then incorrectly required Wayne to wake/share the Extension manually. | generic Extension reconnect defect plus the MCP/Host lifecycle defect in F-010 | 2026-08-05 ordinary-Chrome attach trace and Kickstarter pre-navigation failure; Extension reconnect-cap regression plus Host socket/capability-rotation and Host-absent MCP-init regressions now pass | source root fixes implemented; installed-candidate ordinary-Chrome zero-user-action loop remains open and must pass before dogfood is called fixed |
| F-019 | Kickstarter Goal amount Truth | Chrome exposed the live spinbutton value as `0`, while Saccade exposed `has_value:true` and description `500`. The latter was the authored placeholder, but the unlabeled description made it look like a current value. | generic editable placeholder provenance ambiguity; live state itself was not fabricated | 2026-08-05 Kickstarter Basics trace, Chrome snapshot and Saccade full revision 36 object `o70` | fixed generically: editable placeholder descriptions are explicitly prefixed `Placeholder:`; MCP instructions prohibit treating them as values |
| F-020 | Kickstarter AI disclosure persistence | Saccade consistently reported the Yes radio and generated-content checkbox as unchecked after save/navigation. Chrome could transiently click the visible label, but direct checkbox activation failed and the site restored unchecked state. | client custom-control execution/site persistence incompatibility; Saccade Truth was correct | 2026-08-05 Story revisions 61+ and Chrome checked-state probes | no Collector fix; generic Agent instructions now try the authored label once, require a checked-state delta before save, and retain a truthful blocker on failure |
| F-021 | Kickstarter location custom combobox | The client used broad text matching and `first()` while resolving location suggestions, producing an unintended selection before correction. | client ambiguous-hit selection, not a demonstrated Truth defect | 2026-08-05 Basics Chrome trace | generic Agent instruction now requires an exact authored option and semantic post-selection verification; no site selector added |
| F-022 | Revision-bounded read after reset | The client requested `after_revision:44` while the current observation was revision 36 and received a timeout/rejection, then required a full reread. | generic impossible/future revision recovery gap plus client basis misuse | 2026-08-05 Basics Saccade calls at 17:44:50–17:45:04 | fixed generically: an impossible future basis returns an immediate full gap reset; Agent instructions retain and fold deltas instead of treating omitted objects as absent |
| F-024 | Steam account-profile dogfood | The Agent refused ordinary work and repeatedly invented Saccade-side safety/confirmation categories. | MCP/Profile tried to encode safety policy that belongs to the calling LLM/Agent | 2026-08-06 Steam continuation; Extension 21/21, Profile 4/4, Runtime 29/29, architecture gate | fixed in source: MCP has no safety taxonomy/action gate; Extension alone protects password/SSN/EIN |
| F-025 | Ordinary-Chrome candidate installation | `attach` replaced the Runtime/Extension files but the running Chrome Extension continued using its old Collector: live Truth protected OTP/card, exposed SSN/EIN fields, and failed to mask formatted identifiers. Restarting only Native Host updated capabilities/Profile but not Extension JS. | unpacked Extension hot-reload gap; disk installation was incorrectly treated as live-candidate activation | original fixture tabs `1680319810`/`1680319811`; 2026-08-11 attach rejected the pre-handshake Worker, Chrome Extensions Reload activated candidate `ae471d3d…`, attach verified the exact live identity, and tab `1680321272` projected Password/SSN/EIN as protected, OTP/card as ordinary, plus masked SSN/EIN-shaped text | fixed: content-addressed Worker/Collector/Host handshake, reconnect self-reload, fail-closed attach verification, official legacy-worker bootstrap, and live protected-redaction evidence pass |

## 2026-08-04 React and Angular continuation

The current DemoQA React run completed at revision 16 and observed the success
text `Thanks for submitting the form`. Stable object `o12` (`Sports`) changed
from `checked:false` to `checked:true`. Computer Use clicks on the exposed AX
checkbox and AX text did not activate the custom checkbox, while a click on the
visible authored label did. This reproduces F-004 and does not indicate a Truth,
delta, or identity failure.

The Angular Material official select page then produced a complete initial
Truth through a fresh direct client connected to the same Saccade Runtime:

| Measurement | Result |
| --- | ---: |
| Revision | 2 |
| Projected objects | 171 |
| Selects / options | 25 / 37 |
| Buttons / links | 52 / 13 |
| Headings / paragraphs | 20 / 24 |
| Compact serialized bytes | 40,087 |
| Approximate tokens at four bytes/token | 10,022 |
| Passive changes during three-second wait | 0 |

Role, accessible name, enabled/expanded/value/required/selected state, and
object identity were present for the official Angular examples. The large
initial denominator is a semantic-compression concern for Agent comprehension,
not a silent omission.

After both extensions were installed in the same ordinary Chrome profile, the
client-native action continuation produced these results:

| Scenario | Evidence | Result |
| --- | --- | --- |
| Basic select opens detached overlay | stable select `o124` changed to `expanded:true`; Pizza appeared in the same persistent MCP session | pass |
| Select Pizza / close overlay | `o124` remained stable and changed to `expanded:false, has_value:true`; the visible value became Pizza | pass |
| Repeat open / select Steak | revisions 5–7 retained owner `o124`; Pizza was session-local `o209` and changed selected state without owner duplication | pass |
| Viewport/lazy visibility | Computer Use activated the initially offscreen example; Saccade pushed the newly visible owner/options | pass for this page's viewport behavior |
| Dialog appears | revision 6 pushed `No`, `Ok`, and `Delete file` semantic objects; background semantic content disappeared under modality; the dialog heading now carries bounded `state.modal` | pass; F-013 fixed without a new role or schema version |
| Dialog closes | revision 8 removed dialog objects and restored the same background object identities | pass; focused Chrome/Edge fixture also proves the `modal:true` object disappears |
| Same-tab Angular navigation | tab id remained `1680319129`; document id and revision base reset on navigation | pass |

Object IDs are stable within one Agent/MCP observation session. IDs from two
separately initialized MCP clients are different identity namespaces and must
not be compared directly; the persistent-session rerun was used for the result
above.

## 2026-08-04 PrimeVue, Shoelace, iframe, and lifecycle matrix

This continuation was observation-only. It used Saccade's production
Extension → Host → Runtime → MCP route. It did not use Playwright, CDP, or the
Reference Actuator. Cases that require an external action in the managed test
browser are marked blocked rather than inferred from initial Truth.

| Public source | Chrome | Edge | Result |
| --- | --- | --- | --- |
| PrimeVue Select | repaired clean-profile run: 5/5 first observations followed by 337 objects and 27 selects | 336–337 objects, 27 selects in retained runs | initial Vue Truth passes; F-014 loading-state liveness defect fixed |
| Shoelace Select | 512 objects, 18 selects, revisions 1–6 during component upgrade | 523 objects, 18 selects, revisions 1–5 | delayed open-Shadow-DOM/Web Component projection passes |
| MDN iframe reference | root plus three same-origin child documents observed | repeated runs observed root plus two or three dynamic child documents; one intermediate run truthfully restricted one transient child | frame composition passes; dynamic embedded content count is site/load dependent |
| The Internet dynamic loading | 5 objects including Start button | same | initial Truth passes; transition blocked without same-instance external action |
| The Internet infinite scroll | 3 initial objects | same | initial Truth passes; append delta blocked without same-instance external scroll |
| The Internet sortable tables | 76 objects including 52 cells | same | initial table Truth passes; sort delta blocked without same-instance external action |
| The Internet slow resource | 4 objects including final paragraph | same | eventual initial Truth passes; this trace does not establish resource-by-resource mutation timing |

The focused local matrix remains useful as protocol evidence but is not
substituted for missing public transitions. Candidate evidence
`20260804T230400Z` proves Chrome and Edge pushed deltas with zero missing
markers for disappearance, replacement, reorder, Canvas surface change, WebGL
surface change, dialog appearance/disappearance, and Resource notification.
It also proves same-origin iframe composition, open-shadow composition, and
truthful restricted/closed boundaries. The separate `frames` command now uses
the core observation MCP and passes in Chrome and Edge without optional native
execution (F-015 fixed, evidence `20260804T232323Z`).

Current lifecycle denominator:

| Scenario | Public result | Local protocol result | Final status for this round |
| --- | --- | --- | --- |
| dynamic loading / delayed render | Shoelace delayed upgrade passed; dynamic-loading initial state passed | pushed delta passed | partial pass; public button-caused transition blocked |
| element disappearance | prior APG/Angular dialog objects disappeared | focused disappearance passed | pass for dialog; generic second public source still missing |
| overlay / modal / dialog | APG and Angular action traces passed | `state.modal` lifecycle passed in both browsers | pass |
| infinite scroll | initial Truth passed | viewport/reorder mechanics passed | blocked for public append-on-scroll transition |
| sortable table | initial cells passed | reorder delta passed | blocked for public sort transition |
| slow resource | eventual public Truth passed | Resource notification passed | partial pass; precise resource mutation timing not captured |
| upload / download Truth | no new public action trace | file-input/restricted-document representation passes existing gate | blocked for public lifecycle trace |
| large DOM replacement / reorder | no new public action trace | replacement and reorder deltas passed | partial pass; public source missing |
| viewport change | prior Angular lazy-visibility trace passed | viewport/lifecycle gate passed | pass for one public implementation |
| stream gap / reset / Resource notifications | no public source required by the current trace | focused MCP tests and Resource subscription pass | local protocol pass |

No Collector, Registry, identity, delta, frame/shadow, or protocol code was
changed during this continuation.

## First-pair comparison results

These are engineering observations, not publishable speed claims.

| Task | Lane | Completed | Measured time | Browser calls | Model input tokens |
| --- | --- | ---: | ---: | ---: | ---: |
| Selenium official form | Saccade + Codex Computer Use | yes | unavailable | unavailable | unavailable |
| Selenium official form | Playwright MCP | yes | 40.801 s | 6 | 113,681 |
| DemoQA React form | Saccade + Codex Computer Use | yes | 18.848 s | 7 | unavailable |
| DemoQA React form | Playwright MCP | yes | 30.980 s | 5 | 98,839 |
| Angular Material select | Saccade + Codex Computer Use | yes | 6.034 s | 3 | unavailable |
| Angular Material select | Playwright MCP | yes | 39.680 s | 9 | 159,336 |

Saccade time is measured from its initial Truth to the success delta. Playwright
time includes the isolated Codex lane startup. The boundaries are not equal,
and only the Playwright lane currently exposes model usage. Do not calculate a
speed or token advantage from this table.

## Final-candidate first-pair rerun

All three `Saccade-first` pairs completed after the F-005/F-006 fixes. These
remain engineering measurements because Computer Use timing includes desktop
UI round trips and does not expose model-token usage.

| Task | Lane | Completed | Measured time | Calls | Transcript bytes | Model input tokens |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Selenium | Saccade + Codex Computer Use | yes | 49.785 s | 7 | 9,639 | unavailable |
| Selenium | Playwright MCP | yes | 33.508 s | 5 | 6,111 | 96,681 |
| DemoQA | Saccade + Codex Computer Use | yes | 64.802 s | 9 | 30,347 | unavailable |
| DemoQA | Playwright MCP | yes | 32.907 s | 5 | 12,164 | 99,013 |
| Angular Material | Saccade + Codex Computer Use | yes | 18.470 s | 3 | 55,173 | unavailable |
| Angular Material | Playwright MCP | yes | 31.223 s | 6 | 16,957 | 123,788 |

The reports are under `/private/tmp/saccade-fair-final-20260803`. A true
`Playwright-first` rerun is blocked by F-009; changing only the `order` field is
not accepted as evidence.

## Root-cause analysis

### F-004: DemoQA checkbox click

Saccade recognized `Sports` and pushed its eventual `checked:true` state. Codex
Computer Use clicking the exposed AX checkbox left it unchecked; clicking the
visible `Sports` label changed it. The Playwright lane used
`getByRole('checkbox', {name:'Sports'}).setChecked(true)`, which is not an
equivalent physical click path. The current evidence therefore points to a
client AX hit-target versus page-authored label/input event-binding mismatch,
not Truth recognition or delta failure. Confirming which side owns it requires
a minimal hidden/custom-checkbox page exercised by Computer Use independently
of Saccade.

### F-005: DemoQA revision and push volume

The retained trace contains 86 pushed revisions but only 31 with semantic
changes; 55 have `changes=[]`. Of 59 `updated` objects, 50 changes are dominated
by viewport visibility/frame projection and only seven are state changes.
`collect()` incremented both `revision` and `viewportRevision` and posted a full
snapshot after every scheduled collection, regardless of whether
`compileChanges()` was empty. Focus, scroll, resize, transition/animation,
input, change, and relevant MutationObserver callbacks all schedule collection.
The repair suppresses post-compilation empty semantic deltas. It restores the
previous connected token/object authority map
when suppressing a collection, so the optional Reference Actuator does not lose
valid authority merely because a geometry-only collection was discarded. The
local pushed-delta fixture now injects focus/blur before the real status update;
Chrome and Edge both report that status update as the first pushed delta in
evidence `20260803T191428Z`. A public DemoQA load-only reproduction fell from
86 pushed views (55 empty) in the original completed-task trace to two pushed
views, one empty. The first empty full observation is intentionally
retained: suppressing it caused `tabs.open` to time out on DemoQA before React
hydration, so changing that lifecycle handshake requires a separate Host/MCP
contract design rather than a Collector shortcut.

### F-006: Angular transient duplicate select

At revision 15, stable object `o142` is the expanded `Favorite food` combobox
and listbox options are correctly attributed through `aria-controls`. During
selection, Angular removes that current owner relation before its overlay
listbox finishes disappearing. `comboboxForListbox()` consults only the current
`aria-controls/aria-owns`; it therefore cannot find `o142` and emits the still-
rendered listbox as a new select `o175`. The selected-option boolean is retained
on that transient owner. When the overlay disappears, `o175` disappears while
`o142` has no selected options reachable from its now-removed relation and
remains `has_value:false`. The root cause is loss of temporal choice ownership
across an animated detached-overlay lifecycle, not unstable DOM identity of the
original combobox. The generic repair retains document-local listbox/combobox
ownership while both nodes remain connected, invalidates it when either side is
disconnected or the listbox acquires a different current owner, and resets it
on configure/deauthorization. It contains no Angular selector. In the public
reproduction, stable select `o102` changed from `expanded:true, has_value:false`
to `expanded:false, has_value:true`; option `o173` (`Pizza`) changed to
`selected:true`, and no transient second `Favorite food` select appeared.

### Other findings

- F-023: after a TaxIdentity support message was sent, Chrome exposed the
  visible confirmation inside a dialog but Saccade emitted no corresponding
  text object. The dialog used an otherwise-unmarked generic text container,
  outside the structural selector. The generic repair projects authored
  `aria-live` regions as status and deepest unmarked visible dialog text as
  bounded `text`; the latency/completeness fixture now requires both dynamic
  cases. No TaxIdentity selector was added.

- F-001 is an Agent-client Accessibility focus/modal-menu issue: Saccade had
  already produced correct menu deltas, while Computer Use remained scoped to
  a standalone AX menu after the tab changed.
- F-002 is not currently attributable to Saccade because the same failed run
  lacked examples in Chrome's independent Accessibility tree. The latest run
  rendered normally. Network/resource and public-site hydration evidence is
  needed before assigning a code owner.
- F-003 and F-007 are benchmark-harness design limitations rather than runtime
  defects: clocks, model-token accounting, and order-reversed acquisition are
  not yet symmetric.

## 2026-08-05 lifecycle evidence continuation

This round executed the requested lifecycle matrix and classification pass
without modifying product code. The public action cases were attempted first
through the required product boundary: Saccade opened and observed the managed
tab, and Codex was reserved as the external executor. F-016 prevented the
external action because the two clients exposed different Chrome instances.
The run stopped there instead of substituting Reference Actuator, Playwright,
CDP, selectors, or another browser.

| Scenario | Public evidence | Local protocol evidence | Outcome |
| --- | --- | --- | --- |
| dynamic loading / delayed render | initial The Internet state and prior Shoelace upgrade retained; button transition blocked by F-016 | pushed delta and delayed-render tests pass | `blocked` for new public action trace |
| element disappearance | prior APG/Angular dialog disappearance retained | disappearance marker received in both 2026-08-05 latency runs | `pass` for existing public dialog; second generic source missing |
| overlay / modal / dialog | prior APG and Angular transitions retained | modal appearance/disappearance tests pass | `pass` |
| infinite scroll / viewport | initial public Truth retained; append-on-scroll blocked by F-016 | viewport/reorder mechanics pass | `blocked` for new public append trace |
| sortable table | 52 public cells retained; sort action blocked by F-016 | reorder identity remained stable | `blocked` for new public sort trace |
| slow resource | eventual public Truth retained | Resource push notification arrived in 390.692 ms | `truthful_limitation`: exact resource mutation timestamp unavailable |
| upload / download | no external file action performed because the executor was not in the observed instance | file-input and restricted-document representation tests pass | `blocked` for public action trace |
| large replacement / reorder | no second public action source obtained | replacement disappearance/appearance and 100-object reorder pass | `blocked` for second public source |
| stream gap / reset | public source not required | focused Runtime gap-reset test passes | `pass` locally |
| Resource notification | public source not required | unsolicited notification passes with zero Agent requests during wait | `pass` locally |
| Canvas / WebGL change | arbitrary internals remain opaque by contract | semantic companion changes arrived; 152.858 ms Canvas and 36.122 ms WebGL in the first run | `pass` with truthful opaque boundary |

The first Chrome gate at `20260805T111727Z` passed pushed delta and Resource
subscription but failed the latency/completeness gate because `single:10` was
missing. Its lifecycle markers were otherwise complete: removal 8.811 ms,
replacement 20.950 ms, stable reorder 10.120 ms, Canvas 152.858 ms, and WebGL
36.122 ms. The immediate clean-profile repeat at `20260805T111826Z` received
all 136 markers and preserved reorder identity, but the initial full took
611.993 ms. These two failures are recorded as F-017 and are intentionally not
repaired in this evidence-only round.

Focused non-browser checks remained green: all 18 Extension tests passed, and
the Runtime test `missing_extension_revision_forces_a_full_gap_reset` passed.

## 2026-08-11 authenticated-workflow evidence continuation

An ordinary-Chrome Steamworks onboarding run exercised the production
Saccade-observe / Codex-act / Saccade-verify boundary in a signed-in session.
It completed ordinary company, agreement, mailing-address, permission, and
post-save observation work. Complex controls and the site's explicit
account-mismatch response remained visible through Truth without a
site-specific selector or Saccade execution route.

The run also preserved two truthful limitations: a CAPTCHA remained human
work, and a cross-origin Google account selector remained a restricted frame.
The user stopped before Steam Direct payment and app registration, so neither
is claimed as evidence. This is a successful dogfood continuation, not a new
defect ID and not a Chrome/Edge publication claim. The sanitized report and
non-regression criteria are in
`2026-08-11-steamworks-onboarding-dogfood.md`.
