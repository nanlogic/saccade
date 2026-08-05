# Saccade

[![CI](https://github.com/nanlogic/saccade/actions/workflows/ci.yml/badge.svg)](https://github.com/nanlogic/saccade/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Saccade gives any AI Agent a compact, live semantic view of an authorized
Chrome or Edge tab. The Extension continuously compiles the page and pushes
meaningful changes; the Agent uses its own web-act or computer-use tool in the
same browser tab.

```text
page → Extension compiler → full Truth Layer → semantic delta → Agent
```

Agents receive semantic objects, safe state, affordances, stable document-local
identity, and limitations. They do not receive selectors, DOM paths, editable
values, cookies, browser storage, or default execution authority.

## Product north star

> Saccade is a live semantic Truth Layer for the web. It continuously compiles
> browser pages into structured objects and pushes meaningful changes to any
> Agent. Execution belongs to the Agent client's own tools.

The Extension—not the Agent—continuously compiles the authorized webpage into
that Truth Layer. The Agent never scans a complete webpage to identify a form
or control. After the first compiled view, it receives semantic changes through
a subscribable Truth Layer resource or `saccade.truth.read(after_revision)`;
neither requires model polling.

The protocol is permanently aimed at five product qualities: fast interaction,
low model-token cost, easy maintenance and extension, trustworthy observation,
and model independence. The browser publishes one semantic Truth Layer and
then deltas; registered control modules provide reusable semantic vocabulary;
fresh observations expose the result of external actions. Behavioral policy is
declarative Profile data rather than a dependency on one model or prompt.

Implementations and control coverage will evolve. This positioning does not:
Saccade is not another browser-testing framework, coordinate clicker, or
model-specific browser plugin or replacement computer-use system.

## Status

Saccade is pre-release. The current vertical slice runs through the complete
Extension → Native Host → Runtime → MCP route on managed macOS Chrome for
Testing and Microsoft Edge profiles.

| Inventory | Count | Current evidence |
| --- | ---: | --- |
| Protocol Truth roles | 34 | local Chrome and Edge gate passed |
| Reference Actuator families | 15 | historical local closed-loop evidence; current rerun blocked by macOS permission |
| Reusable variants | 12 | local pushed-delta gate passed |
| Structural/push boundaries | 6 | local gate passed |

All roles, variants, and boundaries are defined in
`catalog/truth_inventory.json`. Full→delta compilation, Profile filtering,
structural reading, same-origin frames, open Shadow DOM, and MCP Resource
notifications have focused two-browser development evidence.

The previous native/soft closed-loop engine remains available only through the
explicit `reference-actuator-mcp` development mode. Its receipts do not
establish default Truth Layer execution capability.

This proves the local framework and projection route, not compatibility with
every modern website. Public-source compatibility, lifecycle coverage, and a
fair core-product Playwright comparison remain open. No current evidence
supports a blanket claim that Saccade is faster than Playwright. Catalog
entries remain `implementation` until one frozen release candidate passes the
public and same-candidate Chrome/Edge release gates. Saccade does not ship a
consumer installer yet.

See the [generated coverage table](docs/generated/control_coverage.md) for the
current Registry and [evidence roadmap](docs/CONTROL_ROADMAP.md) for the next
gates. The [Developer Preview release plan](docs/RELEASE_PLAN.md) defines the
product, evidence, packaging, and launch gates.

## One route

```text
authorized Chrome/Edge tab
  → Extension compiler
  → Native Messaging Host
  → owner-only local IPC
  → MCP adapter
  → Agent
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

Optional Reference Actuator and historical comparison commands are explicit:

```sh
./scripts/dev.sh test-actuator all
./scripts/dev.sh external all
./scripts/dev.sh compare all
./scripts/dev.sh accuracy chrome
./scripts/dev.sh accuracy all
./scripts/dev.sh reflex chrome soft
./scripts/dev.sh reflex chrome native
```

Core-product fair comparisons require a retained Codex or Claude native-Chrome
same-tab evidence file:

```sh
./scripts/dev.sh fair selenium both
./scripts/dev.sh fair demoqa both
./scripts/dev.sh fair angular both
```

The first `up` may download Chrome for Testing and request administrator
approval for the Chrome for Testing Native Messaging
manifest. `down` stops recorded development processes and restores the prior
Codex MCP configuration.

The Edge route uses the stable app at `/Applications/Microsoft Edge.app` and a
separate Saccade browser profile. Set `SACCADE_EDGE_PATH` when the executable
lives elsewhere. Chrome and Edge run one at a time so one browser instance owns
the Host session. `test all` gives each browser a disposable clean profile,
runs them in sequence, writes evidence under separate `chrome/` and `edge/`
directories, and restores the prior Codex MCP configuration when finished.
Single-browser `test chrome` and `test edge` retain the managed development
profiles for quick diagnosis. `up` synchronizes the Extension and
fixtures into the fixed Saccade Dev directory before launch so macOS TCC does
not make the managed jobs depend on repository-folder access. Development
profiles use an explicit generation independent of Extension version, while
the unpacked Extension directory is versioned. This forces MV3 code updates to
load without reading or copying browser cookies; advancing the profile
generation leaves the prior profile untouched. User Profile JSON remains in
the Runtime directory and is never overwritten.

`test` calls `tabs.open → truth.read`, proves pushed deltas plus MCP Resource
notifications, and gates all 34 roles, 12 variants, and 6 structural/push
boundaries in `catalog/truth_inventory.json`. `tabs.open` waits for the first
authorized Truth Layer. The first view is full; later views contain semantic
deltas instead of repeating the page. `test-actuator` runs the isolated
historical execution suite through `saccade.reference.*`. Managed testing stores
evidence under `~/Library/Application Support/Saccade Dev/evidence` and omits
editable contents. Reference evidence commands (`test-actuator`, `external`,
`compare`, `accuracy`, and `reflex`) temporarily isolate and restore
the user's local input policy so conformance runs cannot teach day-to-day
browsing rules.

The same Chrome/Edge core gate runs `probe_truth_latency.py`. Its deterministic
fixture checks 20 single changes, 10- and 100-object simultaneous batches,
disappearance, dynamic replacement, and a 100-object reorder. Evidence records
initial-full and mutation-to-MCP latency plus missing, duplicate, empty-delta,
and identity counts. Current tiered limits are 50 ms single-object p95, 100 ms
10-object p95, 500 ms 100-object p95, 250 ms lifecycle maximum, and 500 ms
initial full, with zero omissions, duplicates, empty deltas, or reorder identity
churn. See `docs/reports/2026-08-03-truth-latency-baseline.md`.

`test chrome|edge|all` uses a 150 ms single-object smoke ceiling because those
long-lived development profiles may contain retained tabs. The publishable
performance gate is `latency-matrix`, which creates a disposable profile for
every browser run and retains the strict 50 ms single-object p95 limit.

The latency fixture also performs a real Canvas 2D draw and WebGL clear. Each
application then updates its accessible semantic companion, which must arrive
as a delta on the same `opaque_surface`. Pure pixels remain opaque; this gate
does not infer internal controls or game objects from raster content.

`external` and `compare` remain historical Reference Actuator evidence. The
`fair` harness has been hard-cut to the core comparison boundary: Saccade Truth
plus the Agent client's own web-act tool in the same tab, with no Reference
Actuator; see the [evidence roadmap](docs/CONTROL_ROADMAP.md).

The fair runner imports the Saccade lane from client-native Chrome evidence; it
does not configure an execution MCP. Pass `--saccade-client-evidence` to the
underlying runner after Codex or Claude proves the same Saccade browser instance
and tab. Client-native evidence must include timezone-qualified `timing.started_at`
and `timing.completed_at`. For `saccade-first`, the Saccade completion must
precede the Playwright start. For `playwright-first`, start the runner first: it
runs Playwright, then waits up to `--client-evidence-timeout` seconds for a new
Saccade evidence file whose start follows Playwright completion. Timestamp
overlap or reversed order fails instead of trusting the `order` label. Without
client-native evidence, the comparison returns `BLOCKED` with
`client_native_same_tab_evidence_unavailable`; Playwright is not run by itself.

Default Saccade never chooses an input backend and does not request macOS
Accessibility. Codex, Claude, or another Agent client executes through its own
tool in the same authorized Chrome/Edge tab. The Reference Actuator retains its
value-free input-policy log only inside that explicit mode.

To share an existing HTTP or HTTPS tab, open the Saccade Extension popup in
that tab and choose **Share this tab**. The popup reports Agent On only for the
current session ACL. Choose **Stop sharing** to remove the tab, discard its
observation session, and invalidate collector tokens. Agent-owned tabs opened
through `tabs.open` are revoked by closing them.

The historical Reference `compare` command first completes radio, switch, tab,
and menu-item loops independently on public W3C WAI-ARIA pages. It then runs an isolated
Playwright oracle in fresh contexts, compares accessible names and false-to-true
state transitions, and saves oracle screenshots. Playwright is test-only: it
cannot create, repair, or replace a Saccade receipt.

Reference `accuracy` runs an ordinary static-target gate through the explicit actuator MCP and native
input route. It clicks 24 semantic buttons across left, center, right, and
scrolled positions: eight each at 32, 40, and 48 CSS pixels. The 24 targets are
split across baseline, moved, and moved-and-resized exact-window phases. Passing
requires 24 verified postconditions and zero misses; it is not the high-rate reflex loop.
An unrelated always-on-top desktop overlay can still cause a truthful
unverified native result.

Reference `reflex` opens the real MouseAccuracy game, reaches its audited highest settings
(`Insane` and `Tiny`) through semantic native button actions, then runs one
bounded local MCP loop. `soft` is a token-bound Extension software mouse and
`native` remains the real OS mouse. Neither accepts Agent coordinates. The
canvas itself stays opaque; only the site's current audited DOM target bridge is
actionable, and every successful receipt requires the score to advance.

## Profiles

A Profile has three fields: `name`, Agent-facing `behavior`, and `ban`.
Profiles can hide named controls from the Agent, but cannot change recognition,
state projection, or delta semantics.

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
| `catalog/` | Truth inventory, optional Reference Actuator catalog, and Profile schema |
| `extension/` | MV3 Extension, collector, ACL, and control modules |
| `crates/saccade_protocol/` | Strict wire types and validation |
| `crates/saccade_control_sdk/` | Catalog-backed semantic Registry and optional reference verifiers |
| `crates/saccade_runtime/` | Host session, Profile, IPC, Truth MCP, and optional Reference Actuator |
| `fixtures/` | Browser conformance fixtures |
| `scripts/` | Catalog generation, architecture checks, and managed development |

User-visible changes are recorded in [CHANGELOG.md](CHANGELOG.md).

Select a managed-development Profile with
`./scripts/dev.sh profile set smart-barbarian-eco`; inspect it with
`./scripts/dev.sh profile show` and restore the default with
`./scripts/dev.sh profile reset`.

## Checks

```sh
cargo fmt --all -- --check
cargo test --workspace --offline
cargo clippy --workspace --all-targets --offline -- -D warnings
node --test extension/tests/*.test.js
node --check tests/reference/playwright/oracle.cjs
python3 -m unittest tests/test_dev_profile.py
python3 -m unittest tests/test_dev_probe.py
python3 -m unittest tests/test_benchmark_agent_fair.py
python3 -m unittest tests/test_external_dogfood.py
python3 -m unittest tests/test_public_truth_cases.py
python3 -m py_compile scripts/dev_probe.py scripts/external_dogfood.py scripts/compare_external_evidence.py scripts/benchmark_playwright_parity.py scripts/benchmark_selenium_qa.py
python3 -m py_compile scripts/benchmark_agent_fair.py scripts/generate_public_truth_cases.py scripts/wait_for_mcp.py scripts/redact_benchmark_artifacts.py
python3 scripts/generate_control_matrix.py
python3 scripts/generate_public_truth_cases.py
python3 scripts/check_single_architecture.py
git diff --exit-code -- docs/generated/control_coverage.md
git diff --exit-code -- catalog/public_truth_cases.json
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before adding a control family. Report
security issues through GitHub's private vulnerability-reporting flow described
in [SECURITY.md](SECURITY.md).

Apache-2.0. See [TRADEMARKS.md](TRADEMARKS.md) for the Saccade name and marks.
