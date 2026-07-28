# Saccade Control Runtime

Saccade is an open closed-loop browser control runtime for already-logged-in
Chrome and Edge tabs.

The only production route is:

```text
Codex / Claude / other Agent
             ↓
         MCP adapter
             ↓
Saccade Runtime (owner-only local IPC)
             ↓
Chrome / Edge Extension
             ↓
Agent-owned or explicitly shared tab
```

The Runtime is distributed as one executable with separate `native-host`,
`mcp`, `doctor`, and `repair` modes. macOS and Windows use the same protocol,
Catalog, Extension, and control modules; only packaging, signing, permissions,
and operating-system input implementations differ.

Start with:

- [`docs/FINAL_ARCHITECTURE.md`](docs/FINAL_ARCHITECTURE.md)
- [`docs/PROFILE_ARCHITECTURE.md`](docs/PROFILE_ARCHITECTURE.md)
- [`catalog/profile.schema.json`](catalog/profile.schema.json)
- [`profiles/default.json`](profiles/default.json)
- [`docs/MIGRATION_MANIFEST.md`](docs/MIGRATION_MANIFEST.md)
- [`docs/extension_observation_contract.md`](docs/extension_observation_contract.md)
- [`docs/truth_layer_coverage_matrix.md`](docs/truth_layer_coverage_matrix.md)

This branch begins as an intentionally minimal architecture skeleton. Production
code is migrated from the historical worktree only when the migration manifest
marks the component as approved.

## Current development slice

The workspace now contains the Catalog/Registry, the four first control-family
verifiers, Native Messaging framing, owner-only local IPC and HostClient,
separate `saccade-runtime native-host` and `saccade-runtime mcp` modes, and
audited macOS/Windows native-input adapters. It also loads the three-field
Profile, filters banned controls, and supplies `behavior` to MCP. Browser-store
Extension wiring now covers the first four controls, but clean cross-browser
release evidence is still pending. Catalog rows therefore remain
`implementation`, not `publishable`.

On macOS, start the isolated Chrome for Testing route with:

```text
./scripts/dev.sh up
./scripts/dev.sh status
./scripts/dev.sh test
./scripts/dev.sh down
```

This uses a dedicated browser profile and the `com.nanlogic.saccade.dev`
Native Messaging host. `up` builds and installs the Runtime as the fixed
user-level, Apple Development-signed `Saccade Dev Runtime.app`, caches Chrome for Testing, starts the
fixture server, and temporarily
points the Codex `saccade` MCP entry at the development Runtime. `down` stops
only recorded development processes and restores the prior MCP entry. The
first `up` may download Chrome, request macOS Accessibility once, and request
one administrator confirmation for Chrome for Testing's system-only Native
Messaging manifest directory. The administrator confirmation is repeated only
if that manifest changes or is removed.

`test` reaches the browser only through MCP JSON-RPC and the production
Extension, Native Host, owner IPC, and native-input route. Evidence is stored
under `~/Library/Application Support/Saccade Dev/evidence`; textfield contents
are excluded.

Run the current gates with:

```text
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
node --test extension/tests/*.test.js
python3 scripts/generate_control_matrix.py
python3 scripts/check_single_architecture.py
```
