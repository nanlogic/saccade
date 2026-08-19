# Automatic full-to-delta delivery and unknown-page comparison

Date: 2026-08-18

## Angular public-select follow-up

The oversized Angular Material page now reaches an LLM through automatic,
tab-scoped bounded pages. A direct public MCP run, with local learned input
policy temporarily isolated and then restored, completed the product route
without Claude-in-Chrome: `tabs.open → truth.read → saccade.act` expanded the
Basic select, projected the rendered `Steak`/`Pizza`/`Tacos` options, and a
second `saccade.act` clicked `Pizza`. The receipt proved the exact option's
`selected` state changed `false → true` at revision 28→29.

The run exposed and fixed three generic boundaries: rendered enabled ARIA
options now advertise token-bound click while native options remain aliases of
their parent select; the Runtime Registry recognizes that role and verifies
`selected`; and unrelated page revisions no longer rotate authority for an
unchanged object/element/role/affordance contract. There are no Angular
selectors, coordinates, screenshots, or browser-client execution in this
path. Candidate `ed7f92e3c480b60d1f05c6481654fdc2f0dadfd92d00de0cb651d5b2da75fb57`
was live and equal to expected.

The immediate Claude Opus 5 low order-reversed rerun produced no comparison:
both lanes stopped before their first tool call with the account limit message
(`resets 12:20am America/Chicago`) and were correctly marked `INVALID`. Those
artifacts authorize no Saccade/Playwright conclusion; rerun after quota reset.
The explicit retry at `20260819T020547Z` reproduced the same result in both
orders and all four lane executions: return code 1, zero tool calls, and the
same account-limit message. Evidence is retained under
`~/Library/Application Support/Saccade Dev/evidence/20260819T020547Z/`.
The driver now classifies this as `infrastructure_failure:account_usage_limit`
rather than the misleading `browser_mcp_unavailable_no_tool_calls` fallback.

## Product result

Public `saccade.truth.read` is now a mandatory per-Agent cursor. The first read
of a document is full; later reads return only the revision-bounded delta from
the last delivered revision. Document replacement or a stream gap returns an
automatic full reset. The public schema no longer accepts `view_mode`.

The transport now follows the same rule before MCP: an authorized Collector
eagerly sends one complete Snapshot, then Extension → Host carries only
source-compiled deltas. Runtime materializes one current complete Truth plus
the bounded compact journal. If an Agent loses its folded cache, it may call
`truth.read({tab_id, resync:true})`; `tab_id` is required, only that Agent/tab
cursor is reset, and no all-tabs Truth or resync operation exists. A Host-side
continuity failure similarly requests a complete Collector Snapshot for only
the affected tab.

`saccade.act` folds its post-action observation through the same cursor and
returns additional semantic effects as `transition`. Runtime retains one
current full observation plus at most 256 compact journal entries containing
revision metadata and changed identities, rather than 256 full observations.

The installed 0.3.22 development Runtime reported
`strategy: automatic_full_then_delta`, `manual_view_selection: false`, and an
input schema containing `tab_id`, `after_revision`, `timeout_ms`,
`delivery_mode`, and the exact-tab recovery flag `resync`.

## Real-browser proof

Chrome and Edge candidate
`9b8b8d2a6d09ae866287ea4a5a7cd9b9408feaa630d2e618637c2dd74fbebc3c`
matched the expected candidate. Both clean-profile browser suites passed the
initial full → pushed delta → exact-tab full resync sequence, resource
subscription, 137/137 latency samples, 15 controls, and 30 semantic roles.
The exact requested and returned tab identities matched in both browsers and
the evidence records `all_tabs: false`. Evidence is under:

`~/Library/Application Support/Saccade Dev/evidence/20260818T224727Z`

The paired lifecycle matrix also passed under:

`~/Library/Application Support/Saccade Dev/evidence/20260818T224406Z`

One final Claude Code Opus 5 low same-tab attempt was retained as a truthful
client-side FAIL, not product evidence: Saccade itself opened and observed the
fixture, but that fresh Claude subprocess had neither permission grants for its
Saccade MCP calls nor an addressable Claude-in-Chrome tab group, so it performed
no action. Evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260818-claude-eager-delta.json`

A second Opus 5 low run removed that unrelated client bridge and allowed only
the six Saccade MCP tools. It completed the product route on tab `1080079165`:
initial `mode: full`; `saccade.act` verified the target button's `pressed`
transition `false → true`; `truth.read({tab_id, resync:true})` returned
`mode: full` for the same tab; and `tabs.close` succeeded. Candidate was
`9b8b8d2a…`; the command reported no permission denials and zero web search or
web fetch requests. The live fixture advances roughly twice per second, so
Claude needed several stale-basis retries before acting at revision 101 and
verifying revision 102. This is valid correctness evidence, not latency or
token-efficiency evidence.

## Corrected same-model matrix

Three generated unknown-page kinds ran in both lane orders with
`claude-sonnet-5`, low effort, Chrome, and locked official
`@playwright/mcp@0.0.79`. One Playwright result initially scored false only
because its final reply put prose before a valid trailing JSON object; tool
output already proved success. The parser was fixed without accepting model
self-report as browser evidence, and the same seed and order passed on rerun.

| Metric (mean, 6 valid pairs) | Saccade | Playwright |
| --- | ---: | ---: |
| Completion | 6/6 | 6/6 |
| End-to-end time | 30.788 s | 29.557 s |
| Tool calls | 9.00 | 12.33 |
| Initial observation bytes | 3,811 | 811 |
| Post-initial observations | 0.00 | 2.00 |
| Logical model input tokens | 238,833 | 234,602 |
| Inline transition responses | 2.67 | 0 |

Saccade therefore proves fewer calls and no post-initial page reads. Total time
is close but does not favor Saccade in this sample, and first-read payload is
still larger. No blanket speed, token, or payload superiority claim is
authorized.

Main evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260818-auto-delta-unknown-3x2-sonnet-low-chrome`

Corrected same-seed rerun:

`~/Library/Application Support/Saccade Dev/evidence/20260818-auto-delta-unknown-3x2-sonnet-low-chrome-rerun-replace-saccade-first`

## Coordinate/mouse boundary

Official Playwright MCP does have an opt-in coordinate route. With
`--caps=vision`, it exposes `browser_mouse_click_xy` and related mouse tools;
`browser_snapshot(boxes=true)` supplies viewport-relative CSS-pixel bounds.
The default semantic comparison intentionally did not enable this capability.

A coordinate comparison must remain separate. Saccade bounds plus an Agent
client's mouse and Playwright boxes plus Playwright's mouse do not share one
executor, while Reference Actuator is not the default product. Until a neutral
same-coordinate-space executor is used for both lanes, no mouse-speed or
mouse-accuracy comparison is authorized.

## Opus 5 low rerun and public-site defect

The generated unknown-page matrix was rerun with Claude Opus 5 low after the
runner was corrected to reuse one exact seed, URL, page, goal, and proof marker
across both lane orders. All six reports passed in both lanes (12 successful
lane executions):

| Metric (mean, 6 valid pairs) | Saccade | Playwright |
| --- | ---: | ---: |
| Completion | 6/6 | 6/6 |
| Tool calls | 9.00 | 10.33 |
| Initial observation bytes | 3,794 | 795 |
| Post-initial observations | 0.00 | 2.33 |
| Logical model input tokens | 240,451 | 234,528 |

This confirms the call-count and zero-reread advantage, but not a blanket
payload, token, or speed advantage. Evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260818-unknown-opus5-low`

The first public Selenium run exposed a real Runtime defect. Public
`saccade.act` correctly ignored an obsolete native escalation during its
generation-aware preflight, then the shared Reference Actuator resolver
incorrectly reapplied the same generation-agnostic rule and rejected the
software checkbox/radio attempt. The public soft-only path now carries the
already-resolved page scope into the closed loop and cannot re-enter that
Reference policy. This also removes one duplicate `tabs.list` call. The focused
regression and all 65 Runtime tests passed.

After the fix, the official Selenium form passed in both lane orders. Saccade
used 10 calls and no reread; Playwright used 11 calls and two snapshots after
the initial snapshot. Saccade's initial payload remained larger (9,777 vs
2,580 bytes), logical input remained higher (about 284k vs 254k), and elapsed
time was close (about 39.7s vs 38.1s). This public task is a workflow check, not
unknown-page evidence, because it is a well-known public fixture. Evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260818-public-opus5-low`

## Soft/native reflex diagnostic

The MouseAccuracy probe previously accepted the first server-rendered shell
containing only a heading. It now waits until both audited client-rendered
settings are observable through Saccade Truth. The regression test passes.

At verified `Insane` + `Tiny`, the optional soft Reference Actuator completed
48 revision-bound action loops in 30 seconds with zero failures (p50 12.47ms,
p95 17.90ms, max 25.87ms):

`~/Library/Application Support/Saccade Dev/evidence/20260819T005229Z/chrome/reflex.json`

The native diagnostic did not produce a comparison: its first receipt was
`permission_required`, with zero completed actions. That is a missing macOS
permission for the optional Reference Actuator, not a core Saccade requirement
and not evidence that soft is faster than native:

`~/Library/Application Support/Saccade Dev/evidence/20260819T005313Z/chrome/reflex.json`

## Large public page blocker

The Angular Material public select task did not produce a valid comparison.
Playwright completed it in 31.2 seconds. Saccade remained connected and
continued receiving deltas, but every full Truth result was about 59KB and the
Claude MCP client rejected it at its per-result token cap before exposing any
`object_id`. Four retries, including exact-tab resync and economy delivery,
failed at the same boundary. The lane therefore performed no action and the
report is `INVALID`, not a Playwright win:

`~/Library/Application Support/Saccade Dev/evidence/20260818-public-opus5-low/angular-saccade-first`

This establishes the next protocol requirement. Runtime must continue holding
one complete current Truth, while oversized first delivery is automatically
split into bounded, exact-tab, same-document/revision continuation pages. The
Agent cursor must not enter delta mode until the initial page sequence is
complete. This preserves complete availability without sending every tab,
asking the model to guess a view mode, or silently truncating objects.

## Codex-only public comparison and delta correction

The fair public comparison now uses `codex exec` for both lanes. Claude and
Claude-in-Chrome are absent. The Saccade lane has only the six Saccade MCP
tools and executes with `saccade.act`; the comparison lane has only locked
official `@playwright/mcp@0.0.79`. Both use the Codex default recommended model
at low effort, the same URL and goal, and are run in both orders.

The first Codex run exposed a Runtime projection error: opening the Angular
select changed enough geometry that MCP promoted the Extension-produced delta
to a new full Agent view. This contradicted the automatic full-then-delta
contract. The density heuristic was removed. After the initial full sequence,
large changes now remain deltas and are automatically paged if needed; only a
new document, stream gap, or exact-tab `resync` may return full. A regression
now changes 160 of 202 objects and requires bounded `mode: delta` pages.

The corrected order-reversed run passed all four lane executions. Each Saccade
lane delivered exactly one initial full sequence in five bounded pages; every
subsequent transition was `mode: delta`. Rendered options were clicked by their
own `click` affordance and the final receipt proved `Pizza selected:
false → true`.

| Metric (mean, 2 orders) | Saccade | Playwright |
| --- | ---: | ---: |
| Completion | 2/2 | 2/2 |
| End-to-end time | 69.45 s | 45.14 s |
| Tool calls | 17.0 | 11.0 |
| Initial observation bytes | 66,779 | 8,359 |
| Tool transcript bytes | 148,503 | 12,981 |
| Codex-reported input tokens | 505,112 | 193,626 |

This public page does not favor Saccade on speed, payload, calls, or reported
tokens. Its 203-object document makes the required complete first Truth much
larger, while Playwright uses server-side `browser_find` plus snapshots. The
result authorizes a correctness claim for automatic full → delta and bounded
software execution, not a superiority claim. It also shows that complete-page
first delivery has a material discovery cost on long documentation pages.

Corrected evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260819T022038Z`

The pre-correction Codex evidence is retained separately and must not be used
for the corrected delta claim:

`~/Library/Application Support/Saccade Dev/evidence/20260819T021312Z`

The same Codex-only runner then completed both orders for the official Selenium
form and DemoQA React form. Across all three public tasks, all 12 lane
executions passed:

| Task / mean of 2 orders | Saccade time / calls / initial bytes | Playwright time / calls / initial bytes |
| --- | ---: | ---: |
| Angular Material select | 69.45 s / 17.0 / 66,779 | 45.14 s / 11.0 / 8,359 |
| Selenium web form | 48.29 s / 10.0 / 8,806 | 38.20 s / 7.0 / 2,969 |
| DemoQA React form | 60.25 s / 22.5 / 12,019 | 46.03 s / 6.0 / 4,025 |
| Overall | 59.33 s / 16.5 / 29,201 | 43.12 s / 8.0 / 5,118 |

The public matrix proves route completeness, not superiority. Saccade was
slower and transferred more in this sample. DemoQA also exposed a generic
client-loop inefficiency: after the first stale-basis rejection, Codex sent the
rest of its planned actions with the same obsolete revision before reading the
delta, then repeated them successfully. The benchmark prompt now requires it
to stop at the first stale result, fold one complete delta, and resume from the
new revision. A future product-level batch action could remove additional
model round trips, but no such unimplemented optimization is counted here.

Additional evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819T022607Z`
  (Selenium, both orders)
- `~/Library/Application Support/Saccade Dev/evidence/20260819T022921Z`
  (DemoQA, both orders)

## Automatic stable-ID catalog correction

The five-page Angular initial read remained too expensive even though each page
was bounded. Runtime now chooses one of two initial projections by serialized
size. Small documents receive one full view. Oversized documents receive a
compact catalog containing every semantic object's stable `object_id`, role,
label preview, affordances, and non-default visibility. The Agent requests full
details for at most 64 relevant IDs against the catalog's exact document and
revision. Detail reads do not advance the cursor. Later ordinary reads remain
revision-bounded deltas.

The first catalog implementation reused the 14KB full/delta page limit. Angular
therefore required three catalog calls. Runtime now omits default and null entry
fields and gives catalogs a 48KB bound. Angular's 202-object catalog fits in one
call; an extreme catalog still pages and does not advance the cursor before the
last page. Rust tests cover both cases.

One Codex low-effort run on Angular passed both lanes after the correction:

| Metric | Saccade | Playwright |
| --- | ---: | ---: |
| Completion | PASS | PASS |
| End-to-end time | 69.23 s | 30.27 s |
| Tool calls | 15 | 7 |
| Initial observation bytes | 27,584 | 26,531 |
| Initial modes | 1 catalog + 1 details | 1 snapshot |
| Full Truth views | 0 | n/a |

Saccade reduced its Angular initial transfer from the earlier 66,779-byte mean
to 27,584 bytes and removed repeated full reads. Playwright remained faster and
used fewer calls in this run. The result validates the delivery correction; it
does not support a performance superiority claim.

Codex evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260819T025025Z`

The Claude runner now configures only the Saccade MCP for its Saccade lane. It
does not enable Claude-in-Chrome, `--chrome`, tab claim, screenshots, or
coordinates. Its first real retry produced no browser calls because the Claude
account returned `You've hit your limit`; the report classifies this as
`INVALID / account_usage_limit`, not a Saccade failure. Rerun it after the
account resets.

Claude infrastructure evidence:

`~/Library/Application Support/Saccade Dev/evidence/20260819-claude-catalog-saccade-only`

## 2026-08-19 compact-delta and Codex rerun

Agent-facing `updated` changes now carry a stable `object_id` and recursive
JSON merge patch instead of repeating the complete object. `appeared` still
carries a complete object, and `disappeared` remains identity-only. This is an
MCP projection change; the Extension and Host continue to retain and transport
canonical complete current objects. Rust and Python consumers now test nested
patch folding, removal through `null`, catalog continuation, and diagnostic
catalog expansion.

The change reduced the Angular overlay transition to two bounded pages for 195
changes. It did not make Saccade faster than Playwright. Opening the overlay
legitimately moves most of the long documentation page, so current coordinates
still account for most of that transition. Both Angular orders passed, with
Saccade averaging 50.28 seconds, 12 calls, and 27,552 initial bytes; Playwright
averaged 37.38 seconds, 9 calls, and 4,326 initial bytes.

The same Codex low-effort runner then produced:

| Task / two orders | Saccade | Playwright | Result |
| --- | ---: | ---: | --- |
| Selenium native form | 40.34 s / 10 calls / 8,806 initial bytes | 29.96 s / 7 calls / 2,969 bytes | both orders PASS |
| DemoQA React form | 47.57 s / 17 calls / 12,019 initial bytes | 29.43 s / 6 calls / 4,025 bytes | three lane scores PASS; one Saccade score FAIL |

The React failure was not an action failure. The form was submitted and the
result cells appeared in Truth, but one run did not expose the fixed
`Thanks for submitting the form` proof string that the scorer requires. The
model's final statement is not accepted as browser evidence, so the run stays
FAIL. This is a success-evidence coverage defect and prevents a clean public
matrix claim.

Fresh generated vanilla pages removed model-memory confounding. Three page
kinds—initially visible, checkbox-revealed, and DOM-replaced controls—each ran
in both orders. All 12 lane executions passed. Means across the six reports:

| Metric | Saccade | Playwright |
| --- | ---: | ---: |
| End-to-end time | 36.65 s | 27.79 s |
| Tool calls | 9.0 | 8.0 |
| Initial observation bytes | 3,491 | 1,374 |
| Post-initial observations | 0 | 1.67 |

Saccade's useful advantage remains zero post-initial page reads: each software
action returns its revision-bound semantic transition. Playwright is still
faster, transfers less initial data, and uses fewer calls overall. The next
performance boundary is a public soft batch action for already-planned form
steps; changing labels, hiding measurements, or weakening success checks would
not address it.

Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819T035221Z`
  (Angular, both orders)
- `~/Library/Application Support/Saccade Dev/evidence/20260819T040743Z`
  (Selenium, both orders)
- `~/Library/Application Support/Saccade Dev/evidence/20260819T041017Z`
  (DemoQA React, both orders)
- `~/Library/Application Support/Saccade Dev/evidence/20260819-unknown-codex-matrix`
  (three generated kinds, both orders)

## Upload, download, iframe, Canvas, and WebGL coverage

Chrome and Edge both passed pushed deltas, resource subscriptions, 137 latency
markers, all 16 Catalog controls, and all 30 semantic roles. Chrome's aggregate
p95 was 27.59ms; Edge's was 26.17ms. The lifecycle run also passed slow
same-origin iframe loading, restricted iframe coverage, disappearance,
replacement, modal open/close, infinite append, table reorder, and viewport
geometry updates. Canvas and WebGL remain truthful opaque surfaces unless a
semantic companion is authored; both companion transitions passed.

Upload and download are intentionally split by authority. `file_input` is
value-free Truth and a successful upload receipt can expose only
`has_value=true`, never the local path. The real macOS Reference upload reached
native dispatch but returned `permission_required` under the browser Host's
responsible-process identity, even though the interactive repair process was
trusted. It is therefore not marked passed. A download link was correctly
projected with `navigation_disposition=download`; public `saccade.act` returned
`external_execution_required`, `retry_safe=true`, and did not pretend that a
transfer occurred. Actual download transfer remains an Agent-client execution
test.

Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819T040539Z`
  (Chrome Truth matrix)
- `~/Library/Application Support/Saccade Dev/evidence/20260819T040640Z`
  (Edge Truth matrix)
- `~/Library/Application Support/Saccade Dev/evidence/20260819T040226Z`
  and `20260819T040723Z` (Chrome and Edge lifecycle)
- `~/Library/Application Support/Saccade Dev/evidence/20260819-upload-download-chrome-complete.json`

## 2026-08-19 public batch and final Codex matrix

The existing sixth public tool now accepts one preplanned batch of independent
ordinary form edits. It does not add an executor or broaden authority: the
Runtime preflights stable object IDs, rejects protected and unsupported roles,
rebases every step to current Truth, uses software input only, and returns
value-free per-step verification plus one final transition. Submit,
navigation, upload, and arbitrary controls remain separate actions.

Benchmark runs explicitly ignored user-local learned input preferences and
recorded that override in capabilities. The override does not edit the user's
policy file. This fixed a real contamination where an old remembered-native
rule prevented Angular software execution.

All reports below ran both lane orders with the same Codex and locked official
Playwright MCP. Every report passed:

| Task | Saccade mean | Playwright mean | Outcome |
| --- | ---: | ---: | --- |
| Selenium native form | 26.17 s, 6 calls, 0 rereads | 35.25 s, 8.5 calls, 3 rereads | Saccade faster/fewer calls |
| DemoQA React form | 46.54 s, 10.5 calls, 4.5 delta pages | 31.34 s, 6 calls, 1 reread | Playwright faster; third-party iframe churn dominates Saccade |
| Angular Material select | 43.78 s, 11 calls | 36.41 s, 10 calls | Playwright faster/smaller |
| Generated unknown pages, 3 kinds × 2 orders | 31.39 s, 7 calls, 0 rereads | 32.06 s, 7.83 calls, 1.67 rereads | Saccade slightly faster/fewer calls |

Playwright retained a smaller initial payload and lower model-token count in
every category. Saccade's advantage is local closed-loop continuity: on the
Selenium and generated-unknown tasks it required no post-initial page read.
The evidence therefore supports task-specific results, not a blanket
superiority claim.

Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819-batch2-selenium`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-batch2-react`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-fresh2-angular`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-batch-unknown`

## Same-tab file transfer

Saccade opened an ordinary-Chrome Agent-owned tab, projected a `file_input`
with `has_value:false`, and exposed no local path. Codex claimed that exact tab
through its own Chrome executor. The upload chooser could not be handed to the
Agent because the ChatGPT browser extension lacks Chrome's **Allow access to
file URLs** permission; upload therefore remains unpassed and Truth stayed
unchanged.

Download completed end to end. Saccade projected the link with
`navigation_disposition:download` and returned
`external_execution_required/retry_safe:true`. Codex clicked it in the same
tab. Chrome's download-event promise timed out, but the actual file appeared at
`~/Downloads/anchor.html`; its 276-byte SHA-256
`ceb9f325254d83ed31bfbe306a38c2c9ce566178de645bef228f6e9b1d764994`
exactly matched the served fixture. The event timeout is a Codex Chrome adapter
observability issue, not a failed transfer or Saccade claim.

## 2026-08-19 semantic working-set and receipt convergence

This section supersedes the earlier batch numbers for the current Runtime
behavior. No Extension selector, browser, CDP, Playwright component, or new MCP
tool was added.

Runtime now keeps the complete canonical Truth locally while returning a
bounded semantic `working_set`. Text query words are conjunctive over safe
name/text/description fields, rendered offscreen controls are eligible while
hidden controls are not, and `min_objects` waits through bounded initial
hydration. A query projected from the latest canonical observation also folds
older queued ambient pages locally.

Verified actions no longer return unrelated structural or geometry churn.
`all_verified` batches return `next_basis_revision`, so a following separate
submit can run without a delta recovery read. An unverified action still
returns same-frame appeared/disappeared evidence when that is the useful result.
Profile behavior remains injected at MCP initialization, but capabilities no
longer retransmits the same body; a live MCP check returned a 2,680-byte
structured capabilities payload with the Profile name and applied marker.

The real Angular `Favorite food -> Pizza` probe completed with two semantic
queries and two verified software actions. Its select and option working sets
were 2,490 and 1,370 bytes. The first receipt proved `expanded false -> true`;
the second proved `selected false -> true`. The 191 ambient changes caused by
scrolling/opening the overlay stayed local.

One clean paired Codex run per public task, using the same model/effort and the
locked official Playwright MCP, produced:

| Task | Saccade | Playwright | Honest result |
| --- | ---: | ---: | --- |
| Angular Material select | 31.47 s, 7 calls, 2,595 initial bytes, 1 re-observation | 43.25 s, 8 calls, 13,362 initial bytes, 2 re-observations | Saccade led all recorded dimensions |
| Selenium native form | 30.10 s, 6 calls, 6,797 initial bytes, 0 re-observations | 36.27 s, 7 calls, 3,002 initial bytes, 2 re-observations | Saccade was faster/fewer calls; Playwright's initial view was smaller |
| DemoQA React form | 42.91 s, 6 calls, 6,126 initial bytes, 0 re-observations | 39.91 s, 7 calls, 41,872 initial bytes, 1 re-observation | Saccade transferred/read less; Playwright was about 3 s faster |

## 2026-08-19 exact multi-target hydration and concurrent-session isolation

Two additional Runtime defects were found through dogfood. A semantic query
kept waiting while unrelated page revisions arrived even after `min_objects`
was satisfied; on a miss it consumed the full six-second maximum instead of
returning after a bounded quiet window. Multi-control form discovery also had
only one conjunctive `text` phrase, forcing broad role queries that transferred
password, disabled, readonly, and otherwise irrelevant controls.

Runtime now returns immediately once `min_objects` is met, keeps waiting through
the bounded declared hydration timeout when it is not, then returns
`settled:false`; it also accepts bounded
`text_any` phrases for exact multi-target projection. ASCII terms use word
boundaries, preventing `Male` from matching `Female`. Native selects query both
the parent and desired option and can batch `select` without a speculative open.
The Extension's complete Snapshot and pushed deltas remain unchanged and local.

The same-machine dogfood also exposed that independent MCP processes inherited
the browser-session ACL's complete Agent-tab list. MCP now projects only the
Agent tabs opened or claimed by that process plus `user_shared` tabs. A real
two-process probe proved each list contained only its own tab and that
cross-session Truth read and close were rejected.

The Angular dogfood then exposed a second discovery problem: a role-only query
returned 8 of 25 selects without the named example context, so the Agent opened
the wrong control and issued four recovery queries. MCP semantic matching now
includes up to three nearby preceding headings already present in Truth. A
query for `Basic select` therefore returned the relevant `Favorite Food` and
`Favorite Car` controls without a site selector or Extension filter.

On the same public Angular task under final candidate `4c5ff8e2…`, the Saccade
lane completed in 39.923 s with 7 tool calls, 4,559 initial bytes, and one
necessary option query. The locked Playwright MCP 0.0.79 lane completed in
51.175 s with 12 calls and 6,780 initial bytes. Both passed. This is one paired
run, not a blanket performance claim; it demonstrates that the earlier
71.781 s / 12-call Saccade trace was a discovery defect rather than an inherent
delta cost.

Clean paired Codex runs with locked official Playwright MCP 0.0.79 produced:

| Task | Saccade | Playwright | Result |
| --- | ---: | ---: | --- |
| DemoQA React | 34.86 s / 6 calls / 4,072 initial bytes / 0 rereads | 45.03 s / 7 calls / 26,381 bytes / 1 reread | both PASS; Saccade led recorded dimensions |
| Selenium form | 25.64 s / 6 calls / 3,490 initial bytes / 0 rereads | 31.84 s / 7 calls / 2,969 bytes / 2 rereads | both PASS; Saccade faster/fewer calls, Playwright first view 521 bytes smaller |

These are single paired runs and authorize no blanket superiority claim.
Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819-text-any2-react-playwright-first`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-text-any2-selenium-playwright-first`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T111319Z/chrome/truth`

These are paired smoke results, not a full repeated statistical matrix. They
show that the protocol defects responsible for repeated full/delta reads are
removed; they do not authorize a blanket speed-superiority claim.

Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819-working-set2-angular-playwright-first`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-working-set4-selenium-playwright-first`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-working-set4-demoqa-playwright-first`
- `scripts/probe_dynamic_query_action.py` live public-site output in this run

Post-change real-browser regressions also passed on the identical Extension
candidate in both browsers: 137/137 latency markers, 16 controls, and 30
semantic roles. Chrome p95 was 35.99 ms and Edge p95 was 26.81 ms.

- `~/Library/Application Support/Saccade Dev/evidence/20260819T103613Z/chrome/truth`
- `~/Library/Application Support/Saccade Dev/evidence/20260819T103657Z/edge/truth`
