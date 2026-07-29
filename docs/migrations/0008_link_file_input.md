# Migration 0008: link and file input

- Source commit: private legacy archive commit `8c4defb3f8b0` was reviewed for
  upload/file-chooser implementation and contained no approved reusable upload
  code. No legacy upload code was copied.
- Destinations: `extension/src/controls/link.js`,
  `extension/src/controls/file_input.js`, `extension/src/collector.js`,
  `crates/saccade_protocol`, `crates/saccade_control_sdk`, and
  `crates/saccade_runtime`.
- Link design: safe name/current/expanded projection, token-bound native
  primary click, and document-transition verification. Destination URLs remain
  undisclosed. A late navigation does not rewrite an already-unverified
  receipt.
- File design: one `upload` operation, `file_chooser` primitive, and `has_file`
  verifier. The Runtime accepts only an absolute accessible regular non-symlink
  file. The path is immediate action data, is not sent to the Extension, and is
  absent from receipts and evidence.
- Ephemeral chooser design: a visible button whose safe name unambiguously
  describes choosing, uploading, adding, or replacing a file or image may stand
  for the temporary native file input it creates. The collector deduplicates a
  hidden input and its visible trigger. The same token is verified only after a
  real file input emits a non-empty `change`; button delivery alone is
  insufficient.
- Repeated-action design: repeated generic buttons or links may carry a bounded
  visible label from their nearest action group. The collector precomputes
  repeated names once per observation. It never reads an input value, local
  filename, path, locator, or coordinate. This let the Agent distinguish
  server-rendered upload rows by their public filenames.
- Native plan: click the prepared center, wait for the OS dialog, invoke the
  platform path-entry flow, type the path through native Unicode input, confirm
  selection, and wait for the page to reobserve. macOS uses flagged
  `Command+Shift+G`; Windows uses the dialog filename field.
- Verification boundary: `has_file` proves chooser acceptance, not remote
  server persistence. A new page object or fresh server-loaded document must
  prove the upload result separately.
- Fixtures and checks: `fixtures/controls/link.html`,
  `fixtures/controls/file_input.html`, Extension browser-global and collector
  tests, SDK Registry/verifier tests, Runtime path validation, bounded native
  plan tests, closed-loop tests, and value-leak assertions.
- Managed integration evidence: authenticated itch.io Chrome dogfood selected
  a 37.8 MB Gear Up PDF with `accepted_by_os + verified` and found no path in
  the receipt. It made the v2 row public, verified the old filename's required
  deletion checkbox, deleted the old card PDF, and loaded a fresh document with
  the expected three files. Three screenshot uploads each returned
  `accepted_by_os + verified`; another fresh document contained three
  screenshot rows. A cover upload replaced its chooser target, but v1 cannot
  assert pixel identity. The fresh document preserved `Graphics=true`.
- Browser-owned confirmation evidence: itch.io screenshot deletion opened a
  browser confirmation dialog outside the DOM observation boundary. A human
  confirmed it. The Runtime did not add a browser-chrome or coordinate
  fallback.
- Public status: `link` and `file_input` remain `implementation`. Local Chrome
  dogfood is not same-candidate Chrome/Edge publication evidence.
