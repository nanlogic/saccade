# Migration manifest

The public repository starts from root commit `9f2b9c55a238` and carries no
legacy history. The private, archived `nanlogic/saccade-legacy` repository at
commit `8c4defb3f8b0` remains a reviewed source. Contributors migrate one
approved component at a time and record its provenance below.

## Approved to migrate

| Area | Historical/current source | Destination | Rule |
| --- | --- | --- | --- |
| Observation and action types | `crates/saccade_protocol` plus current uncommitted contract-aligned changes | `crates/saccade_protocol` | Preserve only `saccade.observation/1` and `saccade-extension-host/1`; migrate tests with code. |
| Extension ACL and consent | `extension/src/service_worker.js`, consent/storage helpers and tests | `extension/src` | Preserve agent-owned/user-shared isolation and session ACL. |
| Extension observation | current `extension/src/collector.js`, `truth.js`, protocol helpers | `extension/src/controls` and collector | Move through Registry modules; do not copy monolithic classification as the final design. |
| Native Messaging | current `bins/saccade-host` framing/session code | `crates/saccade_runtime` + `saccade-runtime native-host` | Preserve framing and validation; separate mode from shared runtime. |
| MCP adapter | current `bins/saccade-mcp` | `saccade-runtime mcp` | Keep a strict adapter; no browser semantics in MCP. |
| macOS input | current `bins/saccade-host/src/input/macos.rs` | Reference Actuator only | Preserve reviewed CoreGraphics behavior for explicit regression use; never initialize it or request Accessibility in the default Truth Layer. |
| Windows input | current `bins/saccade-host/src/input/windows.rs` | runtime platform input | Preserve `SendInput`; add missing primitives and semantic verifiers. |
| Protected fill | current Extension + Host protected-value path | runtime/Extension | Values must never enter MCP, observations, audit, diagnostics, or artifacts. |
| Installer/repair | current `installer`, packaging scripts and accepted DMG evidence | `installers/macos`, `installers/windows` | Migrate only after runtime paths/modes stabilize. |
| Contract and coverage inventory | current working-tree docs | `docs` and later generated Catalog output | Contract stays normative; matrix stays evidence-oriented and must eventually be generated. |

## Research/reference only

| Area | Source | Permitted reuse |
| --- | --- | --- |
| CEF form/control work | historical CEF renderer/form scripts and reports | Semantics, fixtures, evidence patterns, and bounded algorithms only. |
| PixelDetector/fusion/tracker | retired `saccade_detect` and reports | Optional detector research with explicit provenance; no production dependency. |
| Canvas2D/WebGL probes | historical scripts and reports | Diagnostics, fixtures, and semantic-bridge design input. |
| MouseMax/FormMax benchmarks | retired bins/reports | Conformance fixtures or archived benchmark evidence. |

## Do not migrate

- CEF or Servo browser shells, renderer bindings, engine IPC, browser-engine
  profiles, patches, release packaging, or native input. This does not refer to
  the three-field user Profiles in `PROFILE_ARCHITECTURE.md`.
- Retired browser abstraction, replay, benchmark, or site-specific production
  routes.
- Compatibility protocols, alternate schemas, direct-coordinate tools, or
  automatic Playwright/CDP/vision fallbacks.
- Large historical plan/report trees into the default product workspace.

## Migration sequence

1. Create the minimal Rust/Extension/test skeleton and architecture gate.
2. Add the Control Catalog schema and Markdown generator.
3. Consolidate Host/MCP shared code behind one runtime binary with two modes.
4. Migrate ACL, observation identity, token, revision, Native Messaging, and
   owner-only IPC tests. See `docs/migrations/0002_runtime_route.md` and
   `docs/migrations/0003_extension_managed_chrome.md`.
5. Implement button, text-field, checkbox, and select module loops, then run
   the isolated macOS Chrome for Testing development gate.
6. Run the managed macOS Chrome and Edge gate.
7. Freeze Control SDK v1, then migrate common controls one family at a time.
   The first editable family is recorded in
   `docs/migrations/0005_editable_controls.md`.
8. Migrate the reviewed macOS HID click sequence and add the ordinary mouse
   gate. See `docs/migrations/0006_native_mouse_accuracy.md`.
9. Migrate the reviewed current-target classifier and bounded reflex-loop
   behavior. See `docs/migrations/0007_reflex_target_soft_mouse.md`.
10. Add link and single-file chooser loops as new contract-aligned modules. No
    legacy upload code is approved or reused. See
    `docs/migrations/0008_link_file_input.md`.
11. Add radio, ARIA switch, tab, and expanded menu-item loops as new
    contract-aligned modules. No legacy classifier is reused. See
    `docs/migrations/0009_toggle_command_controls.md`.
12. Add bounded structural page reading from the current observation contract.
    No legacy classifier is reused. See
    `docs/migrations/0010_structural_page_reading.md`.
13. Extend the existing select module to ARIA listbox and combobox with enabled
    option identity and indexed native keyboard selection. No legacy classifier
    is reused. See `docs/migrations/0011_aria_choice_controls.md`.
14. Add the session-only Extension popup for sharing and revoking one current
    tab. See `docs/migrations/0012_shared_tab_ui.md`.
15. Add same-origin iframe and open-shadow composition inside the existing top
    collector. No legacy classifier or frame tree is reused. See
    `docs/migrations/0013_frame_shadow_composition.md`.
16. Run clean signed-product macOS/Chrome and Windows/Chrome/Edge
    installation/action gates before publication.
17. Add truthful basic coverage for uncommon controls.
18. Consider Canvas/WebGL semantic bridges before any detector capability.

## Per-component acceptance record

Every migrated component must record:

- source commit and path;
- destination module;
- behavior intentionally retained or dropped;
- unit/static checks;
- native integration evidence where applicable;
- value-leak scan;
- public Catalog/matrix status.

Nothing is migrated merely because it existed in the old tree.
