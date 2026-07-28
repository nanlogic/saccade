# Migration 0001 — Protocol and first control slice

- Source commit: `8c4defb3f8b0ed9b0cb4cb6ff522f9a550ddb76b`.
- Reviewed source paths: `crates/saccade_protocol/src/{lib,action,observation}.rs`,
  `crates/saccade_protocol/tests/canonical.rs`, `extension/src/truth.js`,
  `extension/src/collector.js`, and their focused tests in the historical
  `/Users/waynema/Documents/GitHub/SACCADE` worktree.
- Source state note: the approved protocol/Extension files were uncommitted,
  contract-aligned additions over the recorded source commit. They were
  reviewed file-by-file; the old tree was not copied wholesale.
- Destinations: `crates/saccade_protocol`, `catalog`,
  `crates/saccade_control_sdk`, `crates/saccade_runtime`, and
  `extension/src/controls`.
- Retained: the two existing wire version strings, strict unknown-field
  rejection, safe-state allowlist, opaque single-use token authority,
  prepared-action geometry/focus/topmost gates, protected text rejection,
  and role/state semantics for button, text field, checkbox, select/option.
- Intentionally dropped from this slice: monolithic Extension classification,
  browser semantics in MCP, arbitrary coordinates/keys, CEF/Servo routes,
  and all unapproved control families.
- Checks: `cargo test --workspace`, `node --test extension/tests/*.test.js`,
  `python3 scripts/generate_control_matrix.py`, and
  `python3 scripts/check_single_architecture.py`.
- Native evidence: pending. Trait-level native dispatch is tested; CoreGraphics,
  SendInput, Native Messaging, owner-only IPC, Chrome, and Edge wiring are not
  claimed by this record.
- Value-leak scan: the closed-loop receipt and Extension projection tests use a
  sentinel and assert it is absent.
- Public Catalog status: `implementation`; Chrome and Edge evidence are
  `pending`, so no row is `publishable`.
