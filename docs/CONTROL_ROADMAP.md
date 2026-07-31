# Control roadmap

Saccade grows by implementation family. A batch changes the Catalog, Registry,
Extension collector, native plan, verifier, fixtures, tests, and evidence as
one unit. Agents continue to use generic observation/action tools; new controls
do not add MCP tools. `web.form.fill` is shared orchestration over existing
Registry loops, not a control-specific execution route.

## Definition of done

A control enters the Catalog only when the branch contains:

- one semantic role and safe-state projection;
- one Registry module with finite affordances and native primitives;
- one control-specific postcondition verifier;
- fixtures for success, unavailable state, stale state, coverage, and focus;
- Extension and Rust tests, including editable-value leak checks when needed;
- a real Extension → Native Host → Runtime → MCP → Registry-selected input receipt;
- Chrome and Edge evidence fields, even while they remain `pending`.

`implementation` means the source and focused development gate exist.
`publishable` requires Chrome and Edge artifacts from the same release
candidate.

## Batch 0: freeze the first slice

Controls: button, text field, checkbox, select, and select options.

Status: SDK v1 development freeze complete on 2026-07-28.

The macOS Chrome for Testing development route has verified all four loops.
The managed Edge route uses the same Extension source, Host protocol, Runtime,
MCP tools, registered input, fixtures, and probe. `./scripts/dev.sh test all` runs
both browser profiles in sequence and separates their evidence. Paired run
`20260728T224742Z` passed both browsers on one source candidate and froze the
module contract. The clean signed-product, Windows, and release gates remain
open, so Catalog evidence and publication status stay `pending` and
`implementation`. Keep stale, replay, covered, focus, navigation, Profile-ban,
and value-leak checks green.

## Batch 1: editable controls

Controls: search field, textarea, contenteditable, and spin button.

Status: development gate complete on 2026-07-29.

The family reuses the text-field native click-plus-Unicode plan and
`has_value` verifier while keeping role-specific safe state and name
derivation. Focused fixtures cover actionable and readonly controls; the
textarea gate includes multiline Unicode input, and no editable or numeric
contents enter observations, receipts, diagnostics, or saved evidence. Paired
managed run `20260729T043308Z` produced eight verified receipts in Chrome for
Testing and eight in Microsoft Edge through the same source candidate. Full
IME candidate-window behavior and numeric constraint manipulation remain
future focused gates; this batch claims native Unicode text entry only.

Catalog rows stay `implementation` with release evidence `pending` until the
signed-product release gate passes.

## Batch 2: toggles and choices

Controls: radio, radio group, ARIA switch, listbox, and combobox.

Reuse the checkbox transition verifier and select option identity. Prove radio
group exclusivity, switch checked transitions, duplicate option names, dynamic
options, disabled choices, and popup settling.

Status: radio and ARIA switch development loops completed on 2026-07-29.
Native radio selection proved group exclusivity, and ARIA switch clicks proved
checked-state transitions in paired managed Chrome and Edge runs. Public W3C
radio and switch pages passed Saccade and matched the Playwright oracle in run
`20260729T211221Z`.

ARIA listbox and combobox now reuse the `select` role, option-object identity,
and option-selected verifier. Preparation binds an enabled option to its owner
and computes its enabled keyboard position. The finite native plan clicks the
owner, waits for its popup, returns to the first enabled option, advances by
index, and confirms. Fixtures cover a disabled option, duplicate visible
names, a dynamically inserted option, and popup close. Static and Runtime
tests pass. Paired managed run `20260729T225249Z` subsequently produced 14
`accepted_by_os + verified` receipts in each browser and covered native select,
ARIA listbox, and ARIA combobox. Public-page and release-candidate evidence
remain pending.

## Batch 3: navigation and command controls

Controls: link, tab, menu item, bound label, and named generic control.

Reuse the button click path. Add postconditions for document transition,
agent-owned child tabs, selected-tab changes, expanded menus, and bound-label
control transitions. A revision change without one of these effects stays
unverified.

Status: link implementation and focused authenticated Chrome dogfood completed
on 2026-07-29. It uses native primary click and requires a document transition.
Tab and menu item development loops completed on 2026-07-29. Tabs require a
false-to-true selected transition. Menu items currently require an expanded
transition, so command-only menu items remain unverified. Both passed paired
managed Chrome and Edge runs and public W3C pages in comparison run
`20260729T211221Z`. Bound label, named generic control, child-tab
verification, and release evidence remain open.

## Batch 4: page understanding

Objects: headings, paragraphs, lists, tables, alerts, status messages, images,
frames, opaque surfaces, and restricted documents.

This batch improves observation rather than native action. It must compact
visible text, avoid duplicate labels, report truncation, compose same-origin
frames, and emit limitations for cross-origin frames, closed shadow roots,
Canvas, WebGL, video, and built-in PDF documents.

Status: visible headings, paragraphs, list items, table cells, alerts, and
status messages now project as bounded, non-actionable text objects. The
collector excludes hidden content, nested controls, editable values, and
duplicate nested structural objects. It reports truncation after a 256 KiB
structural-text budget. Node and Rust gates pass, and paired managed run
`20260729T225249Z` proved the projection in Chrome and Edge.

The image slice remains deliberately narrow. A named image may expose an
application-declared `data-saccade-image-identity` as a non-actionable
description. It does not inspect pixels, disclose URLs, or imply equality when
the bridge is absent.

Same-origin iframe and open-shadow composition completed on 2026-07-31 without
changing the root collector message route. The top collector assigns frame
identity, observes descendant mutations, composes native geometry through the
same-origin frame-element chain, and revalidates ancestor coverage. Inaccessible
frames emit `restricted_frame`; closed-shadow contents remain opaque and are not
claimed as generically detectable. Paired managed run `20260731T051006Z` in
Chrome and Edge proved two observed frames, one restricted
frame, and native `accepted_by_os + verified` receipts for a frame button and
an open-shadow button. Lists/table containers, Canvas/WebGL/video, built-in PDF,
and broader restricted-document reporting remain planned.

## Batch 5: specialized native controls

Controls: file input, slider, date/time/month/week/datetime-local, color, and
bounded drag/drop.

Start with truthful recognition and limitations. Add action only after a native
browser gate proves locale behavior, chooser ownership, cancellation, bounds,
and postconditions. Selected filenames and paths stay outside observations and
receipts; one supplied path may exist only in the immediate MCP action payload.

Status: single-file selection implementation and authenticated Chrome dogfood
completed on 2026-07-29. The absolute regular non-symlink path exists only in
the immediate MCP action payload and the finite OS chooser plan; it does not
reach the Extension or receipt. A real file-input change verifies chooser
acceptance, while server transfer persistence requires a separate page effect.
Multi-file, directory, cancellation, locale, Windows, Edge, and release gates
remain open.

## Product work outside control modules

The Runtime still needs these release tracks:

- user-shared tab UI and revocation in the store Extension;
- human-only protected fill for passwords, OTPs, and payment secrets;
- verified downloads and file-selection flows;
- signed/notarized macOS packaging plus signed Windows Setup and repair;
- Chrome Web Store and Edge Add-ons identities and clean-install evidence;
- Windows native-input and owner-only IPC gates;
- release manifests, SBOM, checksums, versioning, and evidence publishing;
- bounded performance, long-session, navigation, frame, and restart tests.

Batch 2 can extend the frozen SDK without changing the production route or
Profile schema. Batch 0 and Batch 1 release work continues in the product
tracks above.
