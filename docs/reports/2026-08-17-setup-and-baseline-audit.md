# Setup lifecycle and comparison-baseline audit

Date: 2026-08-17. Candidate `0.3.22`
(`c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`).
Release artifact `target/release/saccade-runtime` SHA-256
`e4140b180e85557b483a9cd232648642decaaab8854bc653c254c8da24ac780b`.

## Runtime checksum drift check

The signed development wrapper at
`~/Applications/Saccade Dev Runtime.app/Contents/MacOS/saccade-runtime` hashes to
`64944c01e588b45d9d4a31e371de5a321c86179480854f058b2a635fd42b055d`, which differs
from the documented release checksum only because codesigning rewrites the
binary. `scripts/build_setup_release.py --runtime target/release/saccade-runtime`
reproduces the documented `e4140b18…`. No checksum drift.

## Setup CLI defects found and fixed

Both defects were invisible to the previous suite because its tests drive a stub
Runtime and an argument-order-agnostic fake client.

1. **Claude Code was never configured.** `configureClients` invoked
   `claude mcp add --scope user -e KEY=VALUE saccade -- …`. The real Claude Code
   CLI reads the server name as another environment pair and fails with
   `Invalid environment variable format: saccade`. Setup swallowed this as a
   warning, so every install silently produced `Clients: codex, claude-desktop`.
   The name now precedes the flags on both `mcp add` and `mcp remove`. The fake
   client in the test suite now enforces the same name-first contract.

2. **`uninstall --purge` could not remove a preserved Profile.** After an
   ordinary `uninstall`, the setup state file is gone, so a later `--purge`
   returned `Saccade setup is not installed.` and left the Profile and Runtime
   data behind permanently. Purge now removes the managed root when the state is
   already absent, and stays idempotent afterwards.

## Real-binary, real-client lifecycle evidence

Run against the real `target/release/saccade-runtime` through
`packages/setup/bin/saccade-setup.js`, in isolated `HOME`/`CODEX_HOME` values,
with the real local `codex` and `claude` executables.

| Step | Result |
| --- | --- |
| install | `Clients: codex, claude-code, claude-desktop` |
| doctor | setup state, Runtime checksum, Profile, both Native Host manifests, and all three client MCP entries `OK` |
| update | idempotent; custom Profile preserved |
| uninstall | Runtime and all three client entries removed; Profile preserved |
| purge | managed root removed; repeat purge idempotent |

The real Runtime's `doctor` emits `saccade.doctor/1` with
`observation_schema`, `host_protocol`, `ready`, and `capabilities` exactly as
`setup.js` expects, confirming the stub Runtime is faithful. The only failing
doctor leg is
`exact Extension → Native Host → Runtime → MCP candidate: operation timed out`,
which is correct for an isolated home whose browser never connects.

Wayne's real `~/.codex/config.toml` and `~/.claude.json` were checksummed before
and after every run and never changed.

## Comparison baseline re-resolved through Saccade

`benchmarks/playwright-mcp.lock.json` carried `online_latest_verified: false`.
The baseline was re-resolved through the authorized Saccade route only:

```text
saccade.tabs.open https://www.npmjs.com/package/@playwright/mcp
→ saccade.truth.read (semantic revision 8, 1465 objects, ownership agent)
→ saccade.tabs.close
```

Truth carried a `status` object reading
`Viewing @playwright/mcp version 0.0.79`, so the official current version is
`0.0.79` and the previous `0.0.78` pin was stale. No comparison run had consumed
either version, so the lock was corrected rather than re-benchmarked. The
historical Reference Actuator harnesses keep their recorded `0.0.78` default so
their retained evidence stays reproducible. The temporary research tab was
closed and `tabs.list` returned empty.

## Still blocked

- The fair comparison remains `BLOCKED` with
  `client_native_same_tab_evidence_unavailable`; the Playwright lane is
  correctly not run on its own.
- `scripts/audit_public_evidence.py` reports 63 rows blocked on
  `public_client_evidence_incomplete`; only client-owned same-tab public
  evidence can promote them.
- No Claude same-tab loop ran. `claude -p` exits `Not logged in`, and
  `list_connected_browsers` returns no Chrome, so neither Claude execution route
  is available without Wayne.
