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

Reuse the text-field native plan and redaction rules. Add role-specific name
derivation and `has_value` verification. Cover multiline input, IME/composition,
readonly, invalid, required, and numeric constraints without exposing content
or numeric values.

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
and postconditions. File paths and selected filenames stay outside MCP and
receipts.

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

Batch 1 can now reuse the frozen SDK without changing the production route or
Profile schema. Batch 0 release work continues in the product tracks above.
