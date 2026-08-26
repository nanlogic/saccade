# Saccade 0.2.0

Saccade is a Node.js semantic Truth Layer for authorized Chrome and Edge tabs.
It exposes six MCP tools and one production route:

```text
authorized tab → Extension → loopback Node Broker → MCP adapter → Agent
```

There is no Rust runtime, Native Messaging Host, platform driver, bundled test
browser, hidden profile, signing pipeline, or Playwright fallback.

## Why the Broker exists

The Broker is a small Node.js process shared by local MCP connections and the
browser Extension. It behaves like a bounded message broker:

- every MCP connection receives an opaque `agent_session_id`;
- every tab has at most one active writer lease;
- `tabs.open` creates and leases the tab atomically;
- an Agent disconnect leaves its leases `orphaned`; tabs are not closed or
  reassigned;
- a live MCP adapter can prove and resume its exact session after a Broker
  restart; the proof is rotated after use;
- commands have IDs, one end-to-end deadline, dispatch state, and receipts;
- an action delivered before a disconnect is never replayed automatically;
- the Broker keeps canonical current Truth and bounded revision history;
- reconnecting Extensions must push a fresh full snapshot before deltas resume.

The Broker persists only bounded recovery metadata in
`~/.saccade/broker-state.json`: hashed session proofs, Tab lease identity, and
value-free command occurrence. Canonical Truth, deltas, action payloads,
editable values, tokens, and credentials are never written there. After a
restart, leases remain unavailable until the same live MCP adapter proves its
session; otherwise they remain recoverable/orphaned and are never transferred.

## Six tools

| Tool | Purpose |
| --- | --- |
| `saccade.system.capabilities` | Return Node Broker, Extension, and session readiness. |
| `saccade.tabs.list` | List only tabs leased to this Agent session. |
| `saccade.tabs.open` | Open and atomically lease one Chrome/Edge tab. |
| `saccade.tabs.close` | Close one tab owned by this Agent session. |
| `saccade.truth.read` | Explicitly request `full` or `delta` Truth for one exact `tab_id`. |
| `saccade.act` | Run one strict object-addressed software action and verify its transition. |

`truth.read` never selects a tab implicitly. Delta reads require
`after_revision`; the Broker waits locally for browser-pushed change until the
request deadline. If bounded history cannot prove continuity it returns
`reset_required` instead of silently substituting a full page.

## Truth and action boundary

Agents receive semantic objects, safe state, affordances, stable document-local
identity, current geometry, and limitations. They do not receive selectors,
DOM paths, editable values, protected values, cookies, browser storage,
screenshots as Truth, arbitrary JavaScript, or arbitrary-coordinate authority.

An action must match the leased tab, current document, basis revision, unique
object ID, current action token, and registered affordance. Local Extension
waiting handles visibility, enabled state, stable geometry, focus/topmost state,
and bounded timeout. Replacement objects remain stale. A dispatched action with
an ambiguous outcome returns `outcome_unknown` and `retry_safe: false`.

## Install

Node.js 18 or newer is the only local runtime requirement.

```sh
npx -y @nanlogic/saccade install
```

Install the same Saccade Extension candidate in Chrome or Edge, then start a new
Agent task. Setup adds this MCP entry without a `postinstall` hook:

```text
npx -y @nanlogic/saccade mcp
```

Useful commands:

```sh
npx -y @nanlogic/saccade doctor
npx -y @nanlogic/saccade uninstall
npx -y @nanlogic/saccade uninstall --purge
```

Uninstall preserves the Profile unless `--purge` is supplied.

## Development

```sh
./scripts/dev.sh test
./scripts/dev.sh broker
./scripts/dev.sh mcp
./scripts/dev.sh pack
```

The release checks are:

```sh
node --test packages/setup/test/*.test.js
node --test extension/tests/*.test.js
python3 scripts/check_single_architecture.py
python3 scripts/package_extension_release.py --extension-root extension --output /tmp/saccade-extension
npm pack ./packages/setup --dry-run
```

The [product boundary](docs/current/product-execution-boundary.md),
[Node Broker contract](docs/current/saccade-0-2-0-runtime-contract.md),
[Truth contract](docs/current/truth-observation-contract.md), and
[Profile boundary](docs/current/profile-boundary.md) define the current product.
Older architecture documents and benchmark reports remain evidence, not
production routes.
