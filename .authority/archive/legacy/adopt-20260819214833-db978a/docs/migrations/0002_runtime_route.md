# Migration 0002 — Runtime transport and Host route

- Source commit: `8c4defb3f8b0ed9b0cb4cb6ff522f9a550ddb76b`.
- Reviewed source paths: `crates/saccade_protocol/src/transport.rs`,
  `crates/saccade_host_client`, `bins/saccade-host/src/main.rs`,
  `native_messaging.rs`, `ipc_server.rs`, `ipc_server/windows.rs`,
  `session.rs`, `input/mod.rs`, `input/macos.rs`, `input/windows.rs`, and
  `bins/saccade-mcp/src/main.rs` in the approved historical worktree.
- Destinations: `crates/saccade_protocol/src/transport.rs`,
  `crates/saccade_host_client`, `crates/saccade_runtime/{native_messaging,
  owner_ipc,session,platform_input,mcp}.rs`, and the single
  `bins/saccade-runtime` executable.
- Retained: bounded Native Messaging framing, strict transport types,
  owner-only Unix permissions, owner-only Windows pipe SDDL, capability bearer,
  separate Native Host/MCP lifecycles, quiet-window post-action observation,
  CoreGraphics Unicode/click/select input, SendInput Unicode/click/select
  input, and a semantics-free MCP forwarding boundary.
- Corrected during migration: the legacy Host treated any newer revision as a
  verified action. The new session dispatches through the Catalog Registry and
  button/text-field/checkbox/select-specific postconditions.
- Intentionally deferred: tab ACL/service-worker migration, protected-fill UI,
  downloads, bounded reflex loops, installer/repair behavior, and release
  packaging. No alternate browser or direct-coordinate route was introduced.
- Checks: `cargo test --workspace --offline`,
  `cargo clippy --workspace --all-targets --offline -- -D warnings`, Node
  Extension tests, Catalog generation, and the single-architecture gate.
- Integration evidence: Native Messaging framing and owner-only Unix IPC pass;
  Host session → prepare response → mock native Unicode input → fresh settled
  observation → verified receipt passes without leaking the sentinel to the
  Extension request or receipt. macOS code compiles locally. Windows source is
  migrated but still requires the Windows build/action gate.
- Public status: unchanged at `implementation`; Chrome and Edge evidence remain
  `pending`.
