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
| F-010 | Codex Saccade MCP lifecycle | A Codex task retained a dead Unix-socket transport after the development Runtime restarted. A fresh direct MCP client reached the healthy Runtime, but the task-owned MCP connection continued returning `Connection refused`. | client MCP reconnect/lifecycle incompatibility | 2026-08-04 task trace; Runtime doctor reported `extension_connected:true` while task tool calls failed | open; core Truth unaffected |
| F-011 | Codex Computer Use / Chrome for Testing | Computer Use refused the Saccade-managed Chrome for Testing app by display name, exact bundle id, and full application path. The test launcher intentionally disables every extension except Saccade. Loading the installed GPT store-extension directory as a second unpacked extension changed its identity, so its native host rejected it; this is not a valid workaround. | test-browser/client same-tab integration incompatibility | 2026-08-04 Angular task trace and Chrome native-messaging log | resolved for client-native testing by using an ordinary Chrome profile with both extensions installed |
| F-012 | Angular initial Truth size | The official select examples project 171 objects and about 40 KB / 10k estimated tokens before Profile filtering. The projection is complete, but it is larger than the “see and immediately understand” product target. | semantic prioritization/compression gap; not a correctness failure | 2026-08-04 Angular direct-MCP measurement | open; research bounded overview/profile strategy without hiding denominator items |
| F-013 | Dialog modality semantics | Opening the official Angular dialog pushes its heading and action buttons and removes background content from the active semantic view. The existing protocol already permits bounded `state.modal`; the Collector now projects it on the forced dialog heading without adding a role or changing the wire schema. | missing projection in the Collector | 2026-08-04 Angular dialog revisions 6–8 plus clean-profile Chrome/Edge pushed-delta gate `20260804T230400Z` | fixed; official Angular truthfully reports `modal:false` because its example does not author `aria-modal=true`, while the focused fixture proves `modal:true` appearance and disappearance |
| F-014 | PrimeVue initial observation | The first current-candidate Chrome open of the official Select page timed out before the first observation. The Collector suppressed every observation while `document.readyState` remained `loading`, so the normal authorization path could not satisfy the Runtime's bounded first-observation wait. | generic loading-state liveness defect, amplified by a dirty persistent test profile with many restored public tabs | failing retained runs on 2026-08-04; after removing the experimental watchdog/retry patch, a fresh-profile root-cause proof produced 5/5 first observations in 1.44–2.03 s and complete 336–337-object/27-select Truth 155–217 ms later; final dual-browser gate `20260805T005946Z` | fixed at the Collector boundary; loading pages publish non-actionable bounded Truth and recompile after DOMContentLoaded; no retry loop, site branch, or timeout increase |
| F-015 | Legacy `frames` harness | `./scripts/dev.sh frames all` recognized the same-origin frame and open-shadow buttons, then failed because the harness attempted Reference Actuator native clicks and received `permission_required`. The core semantic gate already proved frame/shadow composition without execution. | obsolete optional-actuator harness boundary; core Truth unaffected | failing Chrome evidence `20260804T231118Z`; repaired Chrome/Edge evidence `20260804T232323Z` | fixed; frame/shadow command now uses core MCP observation only and reports `execution_owner: agent_client` |

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
