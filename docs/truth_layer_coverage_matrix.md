# Saccade Truth Layer coverage and evidence matrix

> Migration snapshot: this inventory was imported from the legacy worktree to
> drive the clean rebuild. In this minimal branch, **Current source** means an
> identified source implementation awaiting controlled migration; it does not
> mean the implementation is already present here. A row becomes implemented
> in this branch only after its module, tests, and evidence are migrated and the
> generated Catalog status replaces this snapshot.

This document is a capability inventory and publication evidence index. It is
not a second production contract. Normative behavior lives only in
[`extension_observation_contract.md`](extension_observation_contract.md). The
active Profile may remove named controls from the Agent-visible inventory. It
does not change a control's coverage or closed-loop implementation. See
[`PROFILE_ARCHITECTURE.md`](PROFILE_ARCHITECTURE.md).

## Status legend

| Status | Meaning |
| --- | --- |
| **Current source** | Implemented in the Extension → Host → MCP source tree and covered by current unit/static checks. Live clean-browser validation may still be pending. |
| **Current source (limited)** | The current route sees the family, but only as compact text/structure; it does not expose the dedicated widget semantics or actions named in the desired model. |
| **Historical verified** | Passed a bounded CEF or Servo gate in Git history, but the implementation was retired and is not in the current production route. |
| **Historical prototype** | Code and focused evidence exist, but the method was experimental, fixture-specific, or disabled by default. |
| **Explicit limitation** | The current Truth Layer reports the surface without claiming semantic understanding or actionability. |
| **Not supported** | No publication-safe implementation claim is available. |

## What the Agent sees now

The current projection is intentionally smaller than the DOM. Controls expose
a semantic role, safe page-authored name, safe state, affordances, and an
optional opaque action token. Document content exposes compact visible text.
Editable values, selectors, DOM paths, page-supplied coordinates, cookies, and
storage do not cross the observation boundary.

## Coverage target: what “95%” means

The target is product coverage, not merely catalog coverage:

| Tier | Scope | Required result before the tier is called covered |
| --- | --- | --- |
| **Common / full coverage** | Links; buttons; ordinary text/search/email/tel/url/password fields; textarea and contenteditable; checkbox, radio, switch; select/listbox/combobox/option; tabs; menus; labels; ordinary tables; dialogs; same-origin frames; and their common ARIA equivalents | Stable semantic identity, safe name and state, correct affordances, revision-bound native action, receipt or explicit failure, redaction checks, and current Chrome + Edge artifacts. A classifier entry alone is not coverage. |
| **Uncommon / basic coverage** | Specialized native inputs and structures such as number/range/date/time/month/week/datetime-local/color/file/image/reset, datalist, meter/progress, details/summary, popover, tree/grid composites, drag/drop, and media | At minimum recognize the family, expose safe name/bounds/state that the protocol can prove, and emit an explicit limitation for any unverified semantic or action. Safe actions may be added only after a browser gate. |
| **Outside the 95% core** | Browser/OS-owned prompts and chrome, arbitrary closed-shadow internals, arbitrary Canvas/WebGL semantics, PDF form internals, and novel custom widgets with no accessible semantics | Report an opaque/restricted surface or no object; never fabricate controls. These remain capability extensions rather than blockers for the 95% goal. |

“Full” therefore means observe → authorize → act → receipt across the current
Extension → Host → MCP route. “Basic” means truthful recognition and an honest
boundary; it does not imply actionability. The status columns below describe
today's implementation, while the tier above defines the release target.

### Controls and document objects

| Browser surface family | Desired Agent role | Agent-visible truth | Expected affordance | Current source status | Historical evidence / provenance | Gap or test needed |
| --- | --- | --- | --- | --- | --- | --- |
| `<a href>` | `link` | `name`, `description`, `transition=navigation_possible`, bounds, `visibility` | click, hover, focus | **Current source** | Current link classifier + action pipeline export revisioned tokens. | Validate navigation-sensitive live gate in Edge/Chrome. |
| `<button>` | `button` | `name`, `description`, `enabled`, `pressed`, `expanded`, bounds | click, hover, focus | **Current source** | Current classifier and protocol role allowlist. Historical CEF route had explicit button action handling.[H2] | Validate pressed/expanded timing behavior under rapid DOM mutation. |
| `input[type=button]` | `button` | same as `<button>` | click, hover, focus | **Current source** | Explicitly classed in current classifier with other button-like input types. | Confirm focus/keyboard parity by browser matrix. |
| `input[type=submit]` | `button` | same as `<button>` | click, hover, focus | **Current source** | Explicitly classed in current classifier with button-like inputs. | Confirm form submission state is handled without leaking form payloads. |
| `input[type=reset]` | `button` | same as `<button>` | click, hover, focus | **Current source** | Explicitly classed in current classifier with button-like inputs. | Add explicit reset-path assertion (DOM event side effects + receipts). |
| `input[type=image]` | `button` | same as `<button>` | click, hover, focus | **Current source** | Explicitly classed in current classifier with button-like inputs. | Add browser-native image input payload handling tests. |
| `input[type=text]` | `text_field` | `name`, `description`, `has_value`, `enabled`, `required`, `readonly`, `invalid` | click, focus, type | **Current source** | Text-like path is the default classifier branch in `truth.js`; historical form path included text inputs.[H1] | Add live keyboard/mask/validation matrix. |
| `input[type=email]` | `text_field` | same as text | click, focus, type | **Current source** | Treated as text-like in current path. | Add explicit valid/invalid and autocomplete matrix. |
| `input[type=tel]` | `text_field` | same as text | click, focus, type | **Current source** | Treated as text-like in current path. | Validate mobile keyboard path does not change protocol claims. |
| `input[type=url]` | `text_field` | same as text | click, focus, type | **Current source** | Treated as text-like in current path. | Validate URL-specific native validation transitions. |
| `input[type=password]`, OTP, payment credential classes | `text_field` with `protected=true` | safe name only + `has_value`, enabled, required; no values | click, focus, protected fill only | **Current source** | Protected path is enforced in `consent.js` + protected-fill flow in host/worker. | Add redaction checks on all receipt/artifact channels. |
| `input[type=search]` | `search_field` | same as text | click, focus, type | **Current source** | Search is explicit in current map. | Add clear-control/native clear behavior tests. |
| `input[type=number]` | `spin_button` | `name`, `has_value`, `enabled`, `required`, `readonly`, `invalid` | click, focus, type | **Current source** | Current `truth.js` maps to `spin_button`; historical form inventory included number. [H1] | Add spin-up/down and bounds step tests if claimed. |
| `input[type=range]` | `slider` | `name`, `enabled`, `required`, `readonly`, `invalid` | focus only (manipulation intentionally unclaimed) | **Current source** | Current map includes slider role but currently does not expose value/drag semantics. | Add explicit unsupported/limited publication note for manipulation. |
| `input[type=date]` | `text_field` (date-like) | same as text | click, focus, type | **Current source** | Current map routes date-like controls into text path. | Validate picker and locale behavior in matrix. |
| `input[type=time]` | `text_field` (time-like) | same as text | click, focus, type | **Current source** | Current map routes time-like controls into text path. | Validate picker and locale behavior in matrix. |
| `input[type=month]` | `text_field` (date-like) | same as text | click, focus, type | **Current source** | Current map routes month-like controls into text path. | Validate locale behavior and calendar edge cases. |
| `input[type=week]` | `text_field` (date-like) | same as text | click, focus, type | **Current source** | Current map routes week-like controls into text path. | Validate locale behavior and edge case ranges. |
| `input[type=datetime-local]` | `text_field` | same as text | click, focus, type | **Current source** | Current map routes datetime-local into text path. | Validate native locale and seconds precision handling. |
| `input[type=color]` | `text_field` (color-like) | same as text + `name` | click, focus, type | **Current source** | Current map routes color-like controls into text path. | Add open-cancel-persist tests for color pickers. |
| `input[type=file]` | `file_input` | `name`, `has_value`, `enabled`, `required` | click, focus | **Current source** | Current classifier has `file_input` role; protocol supports flow but not full upload completion proof. | Add chooser/confirm/opening flow proof for native file controls. |
| `input[type=hidden]` | omitted from observation | omitted by collector | none | **Explicit limitation** | collector explicitly skips hidden inputs before token emission. | No action expectation; document omission remains intentional. |
| `input[type=checkbox]` | `checkbox` | `name`, `description`, `checked`, `enabled`, `required`, `invalid` | click, hover, focus | **Current source** | Current protocol maps checkbox role/state; historical CEF had checkbox proofs.[H3] | Add rapid-toggle and stale-replay proofs. |
| `input[type=radio]` | `radio` | same as checkbox + exclusive group behavior in page semantics | click, hover, focus | **Current source** | Current protocol maps radio role/state. | Add radiogroup coherence checks. |
| `<textarea>` | `text_area` | `name`, `description`, `has_value`, `enabled`, `required`, `readonly`, `invalid` | click, focus, type | **Current source** | Current map and protocol support text areas. | Add composition/IME and resize stress tests. |
| `<select>` | `select` | `name`, `description`, `has_value`, `enabled`, `required`, `invalid`, `expanded` | click, focus, select | **Current source** | Current path maps select and option; historical dropdown runs exist.[H3] | Add listbox keyboard and dynamic option matrix. |
| `<option>` | `option` | `name`, `selected`, `enabled` | no independent action; selection via parent select | **Current source** | Current protocol supports option state and selection relation. | Add duplicate-option and mutation tests. |
| `<optgroup>` | `text`/compacted | label only if visible as text | none | **Current source (limited)** | No dedicated protocol role; current collector treats as textish content when visible. | Decide whether to add dedicated semantic grouping role. |
| `<datalist>` | omitted/`text` compaction | list text only if surfaced in DOM text node | none | **Not supported** | No dedicated datalist role in current protocol. | Add explicit support only if required by target apps. |
| `<label>` | `label` | safe label text, protected binding state | click when bound to control | **Current source** | Current map handles bound labels and collector emits label affordance only. | Add multi-label and nested-label stress tests. |
| `<fieldset>` | `text`/limited | structural text only if visible | none | **Current source (limited)** | No dedicated fieldset role in protocol. | Keep limitation or add protocol role extension later. |
| `<legend>` | `text`/limited | legend text if visible | none | **Current source (limited)** | No dedicated legend role in protocol. | Keep limitation in v1. |
| `<output>` | `status` | visible text when present | none | **Current source** | Current classifier maps `OUTPUT` directly to the protocol's `status` role. | Add live update/replace timing and dedupe tests. |
| `<progress>` | `text`/limited | visible text only | none | **Explicit limitation** | protocol lacks progress semantics/state fields. | Add safe progressbar semantics if required. |
| `<meter>` | `text`/limited | visible text only | none | **Explicit limitation** | no dedicated meter state in protocol. | Add dedicated role/state only with safe numeric policy. |
| `<details>` / `<summary>` | `text` / `generic_control` fallback | visible summary/title text; an interactive summary may receive only generic-control affordances | generic click/focus only when the rendered summary is detected as interactive | **Current source (limited)** | No dedicated disclosure role or open/closed state exists in the protocol. | Add explicit disclosure identity, `open` state, and a Chrome/Edge live gate before claiming support. |
| `<dialog>` / `role=dialog` | `text`/limited | visible text and restrictions only | none | **Explicit limitation** | No dedicated dialog lifecycle role/state in current protocol. | Keep explicit limitation until protocol extension lands. |
| `popover` surface | `text`/limited | surfaced as regular elements only | none | **Explicit limitation** | Not in protocol role mapping. | Add dedicated popover support only if required. |
| `[contenteditable=true]`, rich editors | `content_editable` | `name`, `has_value`, editable affordances | click, focus, type | **Current source** | Current role classifier + protocol action allowlist include content-editable. | Add editor-iframe/editor command matrix for complex editors. |
| Drag/drop controls (`draggable`/drop zones) | none (folded) | visible text/role only | none currently | **Not supported** | No drag payload schema in current protocol. | Do not claim drag-and-drop actionability in v1. |
| Media controls (`<video>`, `<audio>`) | `opaque_surface` desired boundary | `<video>` is currently opaque; `<audio>` has no dedicated mapping and can fall back to compact text/generic structure | none claimed in v1 | **Explicit limitation** | Current `truth.js` explicitly maps `VIDEO`, but not `AUDIO`, to `opaque_surface`; neither exposes browser-owned playback controls semantically. | Add an explicit audio boundary and live gates before claiming media-control coverage. |
| `<img>` / `svg` (decorated) | `image` when labeled; omitted when decorative | page-authored name/alt, bounds | none unless independently interactive | **Current source** | Protocol supports image role and compaction rule. | Keep explicit rule to avoid noise and pixel assumptions. |
| `iframe` | `frame` | frame id, frame status, same/restricted distinction, descendants | descendant actions only | **Current source** | Protocol-level frame model with same-origin composition is active. | Add matrix for same/restricted frame destruction and replacement. |
| built-in PDF document | `restricted_document` | document presence and bounds; restricted status | explicit confirmed download/open flow only | **Explicit limitation** | Contract defines built-in PDF as restricted document route. | Keep explicit limitation; no PDF form semantics. |
| Canvas/WebGL surfaces | `opaque_surface` + limitation | safe surface presence/bounds only | none | **Explicit limitation** | See rendered-object section; no current production pixel pipeline. | Keep opaque policy. |
| Reflex target marker families (`data-saccade-reflex-target`, `.target:not(.hit)`) | `reflex_target` | `reflex_target`, `reflex_occurrence`, token, bounds, visibility | bounded loop click/hover | **Current source** | Current collector + host loop runtime preserve narrow audited target semantics. | Add post-receipt settled revision and stale-occurrence matrix. |

## ARIA and composite interaction patterns

| ARIA family | Desired Agent role | Agent-visible fields/state | Expected affordance | Current source status | Historical evidence / gap |
| --- | --- | --- | --- | --- | --- |
| `button` | `button` | `name`, `enabled`, `pressed`, `expanded`, optional `description` | click, hover, focus | **Current source** | Map is explicit in truth role table; add keyboard parity matrix. |
| `link` | `link` | `name`, optional `description`, navigation transition | click, hover, focus | **Current source** | Explicit ARIA mapping exists; add custom-link navigation and stale-token gates. |
| `checkbox` | `checkbox` | `checked`, `enabled`, `required`, `invalid`, name | click, hover, focus | **Current source** | Map is explicit in truth map + protocol. | Add `aria-checked` toggling matrix. |
| `radio` | `radio` | `checked`, `enabled`, `required`, `invalid`, name | click, hover, focus | **Current source** | Map is explicit in truth map + protocol. | Add radio-group exclusivity tests. |
| `radiogroup` | not projected | no relation state | none | **Not supported** | Protocol lacks explicit radiogroup relation. | Add if needed via explicit pattern object in protocol. |
| `switch` | `switch` | `checked`, `enabled` | click, hover, focus | **Current source** | Map included in ARIA role table. | Add switch-only state-change matrix. |
| `textbox` | `text_field` | `name`, `has_value`, `enabled`, `required`, `readonly`, `invalid` | click, focus, type | **Current source** | Explicit ARIA mapping exists; multiline semantics still require a dedicated gate. |
| `searchbox` | `search_field` | same safe editable state as text fields | click, focus, type | **Current source** | Explicit ARIA mapping exists; add custom search-widget clear and submit gates. |
| `combobox` | `select` | `name`, `expanded`, `has_value`, `enabled` | click, focus, select | **Current source** | Map is explicit and aligns with historical combobox select handling. | Add native options and open/close transition matrix. |
| `listbox` + `option` | `select` + `option` | `expanded`, `name`, `selected`, `enabled` | click/focus/select | **Current source** | Map is explicit; option selection remains parent-driven. | Add mutation and scroll-into-view tests for large lists. |
| `menu` / `menubar` | not projected | fallback text structures only | none | **Not supported** | No dedicated menu role/state in protocol. | Keep as non-actionable until explicit menu affordance model exists. |
| `menuitem`, `menuitemcheckbox`, `menuitemradio` | `menu_item` where interactive | `name`, `enabled`, state if inherited by control model | click, hover, focus | **Current source** | Role present in protocol but only for interactive control-shaped items. | Add ARIA keyboard/menu expansion matrix. |
| `tab` / `tablist` / `tabpanel` | `tab` only for `tab`, other two not projected | `selected`, `expanded`, `enabled` for tab only | click, hover, focus | **Current source** (tab), **Not supported** (tablist/tabpanel) | Current map has tab but no explicit associations. | Add parent/relationship assertions for tablist/tabpanel pairs. |
| `slider` | `slider` | `enabled`, `required`, `invalid`, `readonly` (no value claim) | focus | **Current source** | Map exists, manipulation intentionally unclaimed. | Add explicit unsupported note in docs for native slider moves. |
| `spinbutton` | `spin_button` | same as `spin_button` + has_value | click, focus, type | **Current source** | Map exists and aligns with number-like controls. | Add increment/decrement behavior matrix if claimed. |
| `tree` / `treeitem` | not projected | text fallback if present | none | **Not supported** | No tree semantics in current protocol. | Explicitly unsupported. |
| `grid` / `gridcell` / `row` / `table` | `text` roles (`table`, `row`, `cell` fallbacks) | visible text and geometry only | none | **Current source (limited)** | Current map chooses table/grid structural roles but not grid navigation affordances. | Keep limitation for full-grid interaction claims. |
| `scrollbar` | not projected | none | none | **Not supported** | Scroll remains page-level navigation, no widget semantics. | Add if future widget-safe model is introduced. |
| `separator` | not projected | text/structure only | none | **Not supported** | No dedicated separator state role in protocol. | Keep limitation. |
| `toolbar` | not projected | text fallback only | none | **Not supported** | No toolbar role/state currently modeled. | Keep limitation. |
| `tooltip` | not projected | text fallback only | none | **Not supported** | No tooltip role in protocol and no safe trigger semantics. | Keep limitation. |
| `dialog` / `alertdialog` | not projected as dedicated role | text + context only | none | **Explicit limitation** | No dedicated dialog lifecycle state in current protocol. | Keep explicit limitation. |
| `alert`, `status` | `alert`, `status` | visible text + `busy` where exposed | none | **Current source** | Map exists; protocol allows these roles. | Add update/replace timing and dedupe matrix. |
| `progressbar` | not projected | no value-safe state | none | **Not supported** | Protocol no safe progress semantic fields. | Keep as limitation until state policy exists. |

### Accounting summary

- Counting rule: one row per listed family; rows are aggregated only when the
  intended Agent truth/action behavior is substantially identical. The two
  tables contain **41 HTML/browser-surface rows** and **24 ARIA/composite rows**.
- Total researched catalog: **65 rows**.
- Status accounting: **44 Current source**, **5 Current source (limited)**,
  **6 Explicit limitation**, **9 Not supported**, and **1 mixed row** (`tab` is
  current while `tablist`/`tabpanel` relationships are not modeled). These
  categories sum to all 65 rows.
- Rows advertising at least one affordance in the stated circumstances:
  **41**. This is source-level affordance coverage, not 41 independently
  live-verified control families.
- The catalog includes every standard HTML `<input>` type and 24 common ARIA
  interaction patterns. The 95% release target requires full coverage of the
  common tier and basic truthful coverage of the uncommon tier; it is not
  satisfied by merely listing those families here.
- Any source-covered row without a current Chrome/Edge artifact remains
  live-gate pending even if its classifier and protocol mapping exist.

### Why this remains publication-safe

- Families marked **Current source** are bounded by the single Extension → Host → MCP route.
- Rows with a non-empty affordance additionally enter the tokenized
  action/receipt path; the row's test column states where live proof is still
  required.
- Families marked **Explicit limitation** intentionally avoid semantics claims while retaining safe surface reporting.
- Families marked **Not supported** have no v1-safe proof path and require protocol-level expansion.

### Role classifier migration source

The legacy worktree's role-to-control mapping is centralized in
`extension/src/truth.js`. Observation validation, role/kind consistency,
safe-state allowlisting, and text limits are centralized in
`crates/saccade_protocol/src/observation.rs`. These are approved migration
sources, not files already carried into this minimal branch.

## Canvas and rendered-object research inventory

The repository contains meaningful prior research for discovering newly drawn
objects. None of these methods automatically grants current production action
authority. Current v1 keeps arbitrary Canvas/WebGL opaque because object
discovery, semantic identity, and safe native action are separate problems.

| Method | What it detects | How a “new object” appears | Identity/evidence | Evidence level | Current production decision |
| --- | --- | --- | --- | --- | --- |
| Independent DOM target above/alongside canvas | A real DOM control or target created by the app while canvas remains opaque | Mutation plus bounded animation-frame rescans observe a newly actionable node | WeakMap object identity, occurrence ID, revision, topmost check, native receipt | **Current source** and historically verified by reflex gates | **Keep.** This is the only current Canvas-adjacent automatic action route. It does not claim to understand canvas pixels. |
| Canvas2D `getImageData` foreground components | Connected color regions different from a sampled border background | A new connected component produces `visual_object_seen` with center, bounding box, area, average RGBA, and confidence | Canvas node ID plus per-sample visual object ID; signature dedupe across samples | **Historical prototype**; disabled unless `allowCanvasPixelRead=true`.[H6] | **Do not restore as general v1 truth.** It is useful research for future opt-in visual capabilities, but page readback can fail, be tainted, or create privacy/compatibility risk. |
| Rendered-frame `PixelDetector` | Small high-contrast or red connected components in RGBA frame readback | Background delta or red-component detection creates a `TargetCandidate` | Pixel evidence includes area, fill ratio, contrast and temporal delta | **Historical verified** for MouseAccuracy-style targets; retired with Servo/CEF stack.[H7] | **Archive as detector research.** It was specialized for bounded target geometry, not arbitrary semantic UI understanding. |
| DOM + pixel fusion | Candidate regions from DOM rectangles and rendered pixels | Spatially overlapping candidates are grouped and confidence-fused | Source becomes `Fused`; geometry and confidence retained | **Historical prototype/benchmark**.[H7] | **Do not put in the current protocol.** A future optional detector would need explicit provenance and capability negotiation. |
| Temporal tracker | Appearance, movement/update, and disappearance of detected regions | Unmatched candidate creates a new runtime target ID; proximity updates it; missed frames remove it | `Appeared`, `Updated`, `Disappeared` events with first/last seen frame times | **Historical verified** in target-loop benchmarks.[H7] | **Reusable design idea**, not reusable current wire identity. Current DOM objects use WeakMap identity instead. |
| Palette/shape semantic classifier | Game-specific guesses such as player, enemy, hazard, drop, or projectile | A visual component is classified from average color, area, aspect ratio, and position | Preserves source visual object and emits confidence/reasons | **Historical prototype**, explicitly tailored to one local game.[H8] | **Never claim as generic truth.** It is a detector example and violates the current ban on site-specific production collectors. |
| App-provided semantic marker/overlay | Semantics intentionally exposed by the application | The app creates a normal DOM/accessibility object or audited marker | Normal current observation identity and receipt path | **Supported architectural option**, not a universal browser feature | **Preferred future path** for owned canvas apps because semantics come from the application and action authorization still comes from Saccade. |
| Canvas/WebGL runtime/readback diagnostics | Whether pixels, texture upload, shader execution, and `readPixels` work | Runtime status or readback changes after draw | Diagnostic report and artifact, not Agent object identity | **Historical verified on bounded fixtures**; broad third-party coverage remained per-site.[H9] | **Diagnostics only.** Successful `readPixels` is not semantic object recognition. |
| Screenshot + OCR/vision | Visual regions inferred by an external model | Model proposes regions from pixels | Model inference only unless separately revalidated | No current production evidence | **Not supported in v1.** The production architecture forbids screenshots/OCR/vision as an alternate truth or action route. |
| Draw-call interception or arbitrary WebGL scene reconstruction | Hypothetical render primitives/scene objects | Intercepted API calls would create implementation-defined objects | No stable browser-independent semantic identity | **Not implemented** | **Do not claim.** Draw calls are not business semantics and instrumentation would be invasive. |

## What can safely be published

| Claim | Publication status | Required qualifier or evidence |
| --- | --- | --- |
| Saccade exposes a compact semantic model of ordinary browser controls without sending form values. | **Publishable after current live matrix** | Show the role/name/state observation and verify sentinel values are absent from observation, MCP output, receipts, and logs. |
| Saccade covers every HTML control. | **Not publishable** | File chooser, slider manipulation, native date/color widgets, browser-owned dialogs, closed shadow roots, and broad custom widgets still need explicit gates. |
| Saccade has previously detected newly appearing objects from rendered pixels. | **Publishable as historical research** | Cite the retired PixelDetector/tracker and Canvas2D component prototypes; state clearly that they are not the current production route.[H6][H7] |
| Saccade understands arbitrary Canvas/WebGL applications. | **Not publishable** | Historical semantic classification was game-specific; current v1 correctly reports arbitrary canvas as opaque. |
| Saccade can act on a DOM target independently present over an opaque canvas. | **Publishable after current live gate** | Demonstrate the audited target marker/rule, topmost revalidation, native input, and receipt. |
| Saccade verified a bounded local Canvas2D game loop with visual objects and receipts in the retired Servo route. | **Publishable as archived evidence** | Cite AI-008D and identify the exact source-release ServoShell route; do not imply Chrome Extension support.[H9] |
| Saccade uses rendered state rather than DOM semantics alone to authorize an action. | **Narrowly publishable** | Current Extension uses layout, visibility, focus, and `elementFromPoint`, then Host revision/token checks. Do not claim direct Chrome compositor internals or perfect pixel provenance. |

## Remaining coverage gates

Before claiming broad control coverage, run a current Chrome and Edge matrix for:

1. button, link, text/search/email/number/date/time/color inputs;
2. textarea and contenteditable editors;
3. checkbox, radio, switch, select, option, tabs, and menu items;
4. disabled, readonly, required, invalid, expanded, and partially offscreen state;
5. file chooser, download, modal/dialog, drag/drop, and native browser prompts;
6. open shadow root, closed shadow root limitation, same-origin frame, and
   cross-origin frame limitation;
7. ordinary canvas opacity plus a separately present audited DOM target;
8. protected password, OTP, and payment sentinels across observation, MCP,
   receipt, diagnostics, audit, and artifact paths.

For every row record observation bytes/tokens, disclosed object count, task
steps, success/failure, native receipt, human intervention, and value-leak scan.
This turns the table from a source inventory into publishable evidence.

## Historical source index

- **[H1] Historical CEF form inventory and execution:**
  [`saccade_form_script.h` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/engines/cef/host/saccade_form_script.h#L37-L305)
- **[H2] Historical CEF action collector:**
  [`saccade_renderer.cc` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/engines/cef/host/saccade_renderer.cc#L49-L217)
- **[H3] Signed CEF forms and safety report:**
  [`cef_day4_forms_safety_report.md` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/docs/cef_day4_forms_safety_report.md)
- **[H4] Public Build 94 reflex evidence:**
  [`evidence/build94-mouseaccuracy/README.md` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/evidence/build94-mouseaccuracy/README.md)
- **[H5] Historical iframe evidence:**
  [`runs/windows_dogfood/build79_iframe_parity` at `8c4defb`](https://github.com/nanlogic/saccade/tree/8c4defb/runs/windows_dogfood/build79_iframe_parity)
- **[H6] Historical generic browser facts and Canvas2D components:**
  [`browser_fact_stream.js` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/scripts/lib/browser_fact_stream.js#L209-L605)
- **[H7] Historical pixel detector, fusion, and tracker:**
  [`crates/saccade_detect/src/lib.rs` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/crates/saccade_detect/src/lib.rs#L16-L617)
- **[H8] Historical game-specific visual semantics:**
  [`local_game_fact_classifier.js` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/scripts/lib/local_game_fact_classifier.js)
- **[H9] Historical Canvas/WebGL and AI-008D evidence:**
  [`webgl_runtime_probe_report.md` at `8c4defb`](https://github.com/nanlogic/saccade/blob/8c4defb/docs/webgl_runtime_probe_report.md#ai-008d-live-local-game-reflex-gate)

## Maintenance rule

Update a row to **Current source** only when the single Extension → Host → MCP
route implements it and the current regression matrix passes. Update a claim to
**Publishable** only when a reproducible artifact exists for the same route and
release candidate. Historical CEF/Servo evidence remains valuable engineering
research, but it must never silently substitute for current Chrome/Edge proof.

[H1]: https://github.com/nanlogic/saccade/blob/8c4defb/engines/cef/host/saccade_form_script.h#L37-L305
[H2]: https://github.com/nanlogic/saccade/blob/8c4defb/engines/cef/host/saccade_renderer.cc#L49-L217
[H3]: https://github.com/nanlogic/saccade/blob/8c4defb/docs/cef_day4_forms_safety_report.md
[H4]: https://github.com/nanlogic/saccade/blob/8c4defb/evidence/build94-mouseaccuracy/README.md
[H5]: https://github.com/nanlogic/saccade/tree/8c4defb/runs/windows_dogfood/build79_iframe_parity
[H6]: https://github.com/nanlogic/saccade/blob/8c4defb/scripts/lib/browser_fact_stream.js#L209-L605
[H7]: https://github.com/nanlogic/saccade/blob/8c4defb/crates/saccade_detect/src/lib.rs#L16-L617
[H8]: https://github.com/nanlogic/saccade/blob/8c4defb/scripts/lib/local_game_fact_classifier.js
[H9]: https://github.com/nanlogic/saccade/blob/8c4defb/docs/webgl_runtime_probe_report.md#ai-008d-live-local-game-reflex-gate
