# Migration 0005: first editable control family

- Source baseline: public Saccade commit `d77b397`, specifically
  `extension/src/controls/text_field.js`, `extension/src/collector.js`,
  `crates/saccade_control_sdk`, and the Runtime platform-input adapter.
- Legacy review: the private `nanlogic/saccade-legacy` archive remained a
  reference only. No legacy directory, monolithic classifier, or alternate
  execution route was copied.
- Destination: dedicated Registry modules for `search_field`, `text_area`,
  `content_editable`, and `spin_button`; Catalog rows; focused fixtures; SDK
  registration; Runtime verifier tests; and the managed native probe.
- Retained: revision-bound preparation, real center click before Unicode text,
  the finite `unicode_text` primitive, `has_value` verification, receipt
  redaction, Profile filtering outside control modules, and stale-token
  rejection.
- Role boundaries: contenteditable names use only external accessible metadata
  and its state is limited to `has_value` and `readonly`. Editable contents and
  numeric values never enter observation objects or evidence. Readonly controls
  have no affordances or action tokens.
- Intentionally excluded: password/protected fill, full IME candidate-window
  conformance, stepper manipulation and numeric constraints, form submission,
  and any locator, arbitrary-coordinate, CDP, Playwright, or vision route.
- Checks: Extension Registry/collector tests, Catalog generation and
  architecture gate, SDK and Runtime tests, Clippy, fixture leak scanning, and
  paired managed native tests.
- Native evidence: run `20260729T043308Z` produced eight `accepted_by_os` and
  `verified` receipts in Chrome for Testing and eight in Microsoft Edge. Both
  browsers also passed Profile behavior/ban and stale-token rejection, and no
  supplied or fixture editable sentinel appeared in saved evidence.
- Public status: all eight current Catalog rows remain `implementation`; Chrome
  and Edge release evidence remains `pending` until the signed-product gate.
