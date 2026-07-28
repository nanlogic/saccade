# Saccade

[![CI](https://github.com/nanlogic/saccade/actions/workflows/ci.yml/badge.svg)](https://github.com/nanlogic/saccade/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Saccade gives AI agents a compact semantic view of an authorized Chrome or
Edge tab and executes browser actions through native operating-system input.
Each action follows one closed loop:

```text
observe → prepare → revalidate → native input → reobserve → verify → receipt
```

Agents receive semantic controls and opaque action tokens. They do not receive
selectors, DOM paths, arbitrary coordinates, editable values, cookies, or
browser storage.

## Status

Saccade is pre-release. The first vertical slice runs through the complete
Extension → Native Host → Runtime → MCP route on managed macOS Chrome for
Testing and Microsoft Edge profiles.

| Control | Action | Verified postcondition |
| --- | --- | --- |
| Button | native click | pressed or expanded state changes |
| Text field | native click and Unicode input | field changes from empty to non-empty |
| Checkbox | native click | checked state changes |
| Select | native selection by option identity | requested option becomes selected |

The paired development run covers stale-token rejection, Profile behavior,
Profile bans, and editable-value leak checks in both browsers. Catalog entries
remain `implementation` until Chrome and Edge pass the release gate for the
same candidate. Saccade does not ship a consumer installer yet.

See the [generated coverage table](docs/generated/control_coverage.md) for the
current Registry and [control roadmap](docs/CONTROL_ROADMAP.md) for the planned
batches.

## One route

```text
Agent
  → MCP mode
  → owner-only local IPC
  → Native Host mode
  → Chrome/Edge Native Messaging
  → Saccade Extension
  → authorized tab
```

Saccade has no Playwright, CDP, embedded-browser, screenshot, vision, or
coordinate fallback. The [final architecture](docs/FINAL_ARCHITECTURE.md) and
[Truth Layer contract](docs/extension_observation_contract.md) define the
route and its boundaries.

## Development on macOS

The managed development environment uses its own browser profile, Extension
identity, Native Messaging manifest, Runtime app, and fixture server.

```sh
./scripts/dev.sh up chrome
./scripts/dev.sh status
./scripts/dev.sh test chrome
./scripts/dev.sh test edge
./scripts/dev.sh test all
./scripts/dev.sh down
```

The first `up` may download Chrome for Testing, request macOS Accessibility,
and request administrator approval for the Chrome for Testing Native Messaging
manifest. `down` stops recorded development processes and restores the prior
Codex MCP configuration.

The Edge route uses the stable app at `/Applications/Microsoft Edge.app` and a
separate Saccade browser profile. Set `SACCADE_EDGE_PATH` when the executable
lives elsewhere. Chrome and Edge run one at a time so one browser instance owns
the Host session. `test all` runs them in sequence and writes evidence under
separate `chrome/` and `edge/` directories. `up` synchronizes the Extension and
fixtures into the fixed Saccade Dev directory before launch so macOS TCC does
not make the managed jobs depend on repository-folder access.

`test` calls `tabs.open → web.observe → web.act` through MCP JSON-RPC. It stores
evidence under `~/Library/Application Support/Saccade Dev/evidence` and omits
textfield contents.

## Profiles

A Profile has three fields: `name`, Agent-facing `behavior`, and `ban`.
Profiles can hide named controls from the Agent, but cannot change a control's
execution or verification loop.

```json
{
  "name": "cautious",
  "behavior": "Explain consequential actions before acting.",
  "ban": [
    { "control": "Delete account" },
    { "control": "Continue", "condition": "payment" }
  ]
}
```

Read [Profile architecture](docs/PROFILE_ARCHITECTURE.md) and the
[Profile schema](catalog/profile.schema.json) before adding Profile behavior.

## Repository map

| Path | Purpose |
| --- | --- |
| `catalog/` | Machine-readable controls and Profile schema |
| `extension/` | MV3 Extension, collector, ACL, and control modules |
| `crates/saccade_protocol/` | Strict wire types and validation |
| `crates/saccade_control_sdk/` | Catalog-backed Registry and verifiers |
| `crates/saccade_runtime/` | Host session, Profile, IPC, MCP, and native input |
| `fixtures/` | Browser conformance fixtures |
| `scripts/` | Catalog generation, architecture checks, and managed development |

User-visible changes are recorded in [CHANGELOG.md](CHANGELOG.md).

## Checks

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
node --test extension/tests/*.test.js
python3 scripts/generate_control_matrix.py
python3 scripts/check_single_architecture.py
git diff --exit-code -- docs/generated/control_coverage.md
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before adding a control family. Report
security issues through GitHub's private vulnerability-reporting flow described
in [SECURITY.md](SECURITY.md).

Apache-2.0. See [TRADEMARKS.md](TRADEMARKS.md) for the Saccade name and marks.
