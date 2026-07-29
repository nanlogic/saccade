# Control roadmap

Saccade grows by implementation family. A batch changes the Catalog, Registry,
Extension collector, native plan, verifier, fixtures, tests, and evidence as
one unit. Agents continue to use `web.observe` and `web.act`; new controls do
not add MCP tools.

## Definition of done

A control enters the Catalog only when the branch contains:

- one semantic role and safe-state projection;
- one Registry module with finite affordances and native primitives;
- one control-specific postcondition verifier;
- fixtures for success, unavailable state, stale state, coverage, and focus;
- Extension and Rust tests, including editable-value leak checks when needed;
- a real Extension → Native Host → Runtime → MCP → native-input receipt;
- Chrome and Edge evidence fields, even while they remain `pending`.

`implementation` means the source and focused development gate exist.
`publishable` requires Chrome and Edge artifacts from the same release
candidate.

## Batch 0: freeze the first slice

Controls: button, text field, checkbox, select, and select options.

Status: SDK v1 development freeze complete on 2026-07-28.

The macOS Chrome for Testing development route has verified all four loops.
The managed Edge route uses the same Extension source, Host protocol, Runtime,
MCP tools, native input, fixtures, and probe. `./scripts/dev.sh test all` runs
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

## Batch 3: navigation and command controls

Controls: link, tab, menu item, bound label, and named generic control.

Reuse the button click path. Add postconditions for document transition,
agent-owned child tabs, selected-tab changes, expanded menus, and bound-label
control transitions. A revision change without one of these effects stays
unverified.

Status: link implementation and focused authenticated Chrome dogfood completed
on 2026-07-29. It uses native primary click and requires a document transition.
Tab, menu item, bound label, named generic control, child-tab verification, and
same-candidate Edge/release evidence remain open.

## Batch 4: page understanding

Objects: headings, paragraphs, lists, tables, alerts, status messages, images,
frames, opaque surfaces, and restricted documents.

This batch improves observation rather than native action. It must compact
visible text, avoid duplicate labels, report truncation, compose same-origin
frames, and emit limitations for cross-origin frames, closed shadow roots,
Canvas, WebGL, video, and built-in PDF documents.

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
