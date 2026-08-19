# Saccade

[![CI](https://github.com/nanlogic/saccade/actions/workflows/ci.yml/badge.svg)](https://github.com/nanlogic/saccade/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Saccade gives any AI Agent a compact, live semantic view of an authorized
Chrome or Edge tab. The Extension continuously compiles the page and pushes
meaningful changes; the Agent uses its own web-act or computer-use tool in the
same browser tab.

```text
page change → Extension compiler → current Truth → MCP delta → Agent reaction
```

Agents receive semantic objects, safe state, affordances, stable document-local
identity, current document/viewport bounds, and limitations. Geometry changes
arrive as deltas on the same identity. Agents do not receive selectors, DOM
paths, editable values, cookies, browser storage, or default execution
authority.

The Extension owns page interpretation. The Native Host keeps the latest Truth
and bounded revision history. MCP delivers a full view or folded delta. The
Agent chooses `live` delivery for fast reactions or `economy` delivery for
lower model churn, then acts through its own same-tab tool.

Read [How Saccade works](docs/HOW_SACCADE_WORKS.md) for the public architecture
overview. The [final architecture](docs/FINAL_ARCHITECTURE.md) and
[Extension Truth contract](docs/extension_observation_contract.md) define the
normative boundary.

## Product north star

> Saccade is a live semantic Truth Layer for the web. It continuously compiles
> browser pages into structured objects and pushes meaningful changes to any
> Agent. Execution belongs to the Agent client's own tools.

The Extension continuously compiles the authorized webpage into that Truth
Layer. The Agent builds its plan from the compiled view. After the first view,
it receives semantic changes through a subscribable Truth Layer resource or
`saccade.truth.read(after_revision)` without model polling.

The protocol favors fast interaction, low model-token cost, trustworthy
observation, and model independence. Registered control modules provide a
reusable semantic vocabulary. Fresh observations expose the result of external
actions, while declarative Profile data supplies Agent-facing behavior.

Saccade remains a live Truth Layer as implementations and control coverage
evolve. Browser testing, arbitrary-coordinate execution, and model-specific
input backends stay outside the core product.

## Status

Saccade is pre-release. The current vertical slice runs through the complete
Extension → Native Host → Runtime → MCP route on managed macOS Chrome for
Testing and Microsoft Edge profiles.

| Inventory | Count | Current evidence |
| --- | ---: | --- |
| Protocol Truth roles | 34 | local Chrome and Edge gate passed |
| Reference Actuator families | 15 | local closed-loop evidence; soft reflex reached 96/96 on `Insane + Tiny` |
| Reusable variants | 12 | local pushed-delta gate passed |
| Structural/push boundaries | 6 | local gate passed |
| Lifecycle scenarios | 11 | page-driven Chrome and Edge matrix passed locally |

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
public and same-candidate Chrome/Edge release gates. The public setup package
is the release target and is not published yet.

See the [generated coverage table](docs/generated/control_coverage.md) for the
current Registry and [evidence roadmap](docs/CONTROL_ROADMAP.md) for the next
gates. The [Developer Preview release plan](docs/RELEASE_PLAN.md) defines the
product, evidence, setup, and launch gates.

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
arbitrary-coordinate action fallback. The
[How Saccade works](docs/HOW_SACCADE_WORKS.md) overview explains this route for
Agent builders.

## Development on macOS

The managed development environment uses its own browser profile, Extension
identity, Native Messaging manifest, headless Runtime, and fixture server. Its
internal macOS app wrapper exists only for development signing and Native
Messaging tests; it is not a public product component.

```sh
./scripts/dev.sh mcp install
./scripts/dev.sh up chrome
./scripts/dev.sh status
./scripts/dev.sh test chrome
./scripts/dev.sh test edge
./scripts/dev.sh test all
./scripts/dev.sh public-truth chrome
./scripts/dev.sh public-truth edge
./scripts/dev.sh lifecycle all
./scripts/dev.sh denominator
./scripts/dev.sh down
```

Install the Codex MCP entry once. Normal `up`, `down`, `attach`, and test
commands never rewrite or restore the live Codex configuration, because doing
so destroys the MCP transport owned by an already-running task. Use
`./scripts/dev.sh mcp restore` only when intentionally removing the development
entry; start a new Codex task after either explicit configuration change.

The managed browser intentionally contains only Saccade and cannot prove a
Codex/Claude same-tab execution loop. For that test, use ordinary Chrome with
both browser extensions installed:

```sh
./scripts/dev.sh down
./scripts/dev.sh attach
```

Then open the target HTTP(S) page in ordinary Chrome and enable it from the
Saccade popup. Do not run `up chrome` at the same time: that would give the
single Native Host session to a different Chrome instance.

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

Core-product fair comparisons use one Claude model and effort level for both
lanes. The Saccade lane uses Truth plus `saccade.act`; the Playwright lane uses
the locked official Playwright MCP. The command temporarily isolates and then
restores user-local input-policy state so prior diagnostics cannot bias either
lane. Defaults are Claude Opus 5 at low effort; override them with
`SACCADE_FAIR_MODEL` and `SACCADE_FAIR_EFFORT`:

```sh
./scripts/dev.sh fair selenium both
./scripts/dev.sh fair demoqa both
./scripts/dev.sh fair angular both
```

The unknown long-horizon gate generates oracle-checked review queues at
lengths 1, 5, 10, 25, and 50 for same-identity updates, DOM replacement, and
document navigation. Each seed runs in both lane orders. Interrupted runs may
resume only reports already marked `PASS`; failed, invalid, or missing reports
are rerun:

```sh
python3 scripts/run_long_horizon_matrix.py \
  --runtime "$HOME/Applications/Saccade Dev Runtime.app/Contents/MacOS/saccade-runtime" \
  --runtime-dir "$HOME/Library/Application Support/Saccade Dev/runtime" \
  --fixture-root "$HOME/Library/Application Support/Saccade Dev/fixture-root" \
  --output "$HOME/Library/Application Support/Saccade Dev/evidence/long-horizon" \
  --resume
```

The six frozen public read-only tasks use the same resumable PASS-only rule:

```sh
python3 scripts/run_public_agent_fair_matrix.py \
  --runtime "$HOME/Applications/Saccade Dev Runtime.app/Contents/MacOS/saccade-runtime" \
  --runtime-dir "$HOME/Library/Application Support/Saccade Dev/runtime" \
  --output "$HOME/Library/Application Support/Saccade Dev/evidence/public-matrix" \
  --resume
```

The first `up` may download Chrome for Testing and request administrator
approval for the Chrome for Testing Native Messaging
manifest. `down` stops recorded development processes and leaves the installed
Codex MCP configuration unchanged.

The Edge route uses the stable app at `/Applications/Microsoft Edge.app` and a
separate Saccade browser profile. Set `SACCADE_EDGE_PATH` when the executable
lives elsewhere. Chrome and Edge run one at a time so one browser instance owns
the Host session. `test all` gives each browser a disposable clean profile,
runs them in sequence, writes evidence under separate `chrome/` and `edge/`
directories, without mutating the live Codex MCP configuration.
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

Sequential completeness markers use distinct stable semantic objects. This is
intentional: `truth.read(after_revision)` folds retained source revisions into
the latest current state, so repeatedly overwriting one object cannot demand an
event log of obsolete intermediate values. Delivery evidence retains folded
revision batches separately from object completeness.

`truth.read` also accepts an optional per-call `delivery_mode`. `live` is the
compatible default and returns the next push immediately. `economy` waits a
bounded 150 ms inside MCP and folds the burst into one latest truthful delta,
reducing routine model-facing churn without hiding objects or geometry. The
Agent chooses freely on every call and can switch modes without restarting.

Truth view delivery is automatic rather than model-selected. The first read is
optionally one bounded semantic `query` working set when the task already names
useful roles, labels, or affordances; Runtime keeps the complete canonical
observation locally. Distinguishing task words plus a narrow role are preferred
for a single target; all whitespace-separated words must match across the
object's name, text, or description. `visible_only: false` includes rendered offscreen controls but never
hidden ones, and `min_objects` waits through bounded initial hydration. A
follow-up query for a revealed option folds queued ambient geometry locally
instead of sending it to the model. Without a query it is one bounded full view
or, for an oversized document, one complete compact catalog of stable object
IDs, roles, labels, affordances, and visibility. The
Agent requests full records only for task-relevant IDs against that exact
document and revision. Every later ordinary read in that MCP session is only
the delta from the last delivered revision. A document change, stream gap, or
unavailable history base returns a new full-or-catalog reset. If the Agent's
folded cache is wrong, `truth.read` accepts `resync: true` only together with
the exact required `tab_id`; it resets only that Agent/tab cursor. There is no
all-tabs Truth read or reset. The public schema has no model-selected
`view_mode` or routine repeated-full override. Runtime stores one current full
observation plus at most 256 compact change-journal entries, not 256 full pages.
`saccade.act` advances the same cursor. `verified: true` or batch
`all_verified: true` is complete proof and carries no structural transition or
ambient pending hint. Unrelated animation, timer, advertisement, geometry, and
frame churn remains silently queued on the ordinary Truth cursor. An
unverified action may still return same-frame structural evidence, so ordinary
verified actions require no follow-up read.
Independent ordinary form edits can be sent once as `actions`; Runtime
sequentially rebases and verifies them, while submit, navigation, upload, and
other material actions remain separate. The batch result supplies
`next_basis_revision` for a following separate action, so the Agent does not
read ambient churn merely to recover the new basis. Updated delta objects use recursive
merge patches: only changed fields cross MCP, and a `null` value removes a
cached field. Appeared objects remain complete.

MCP initialize is a compact route/loop invariant. The first
`system.capabilities` call returns the active Profile name and behavior once,
with `behavior_delivery: "capabilities_once"`, `profile_digest`, Runtime
version, and MCP contract hash. The ban list remains private. A contract change
requires a new Agent session; setup doctor rejects stale installed identities.

Current Truth links may include a resolved, bounded HTTP(S)
`navigation_target`. Agents use the existing `tabs.open` tool to inspect that
source; search titles and snippets alone are not verified research. Temporary
search tabs should be closed, while useful source pages supporting the answer
should remain open for the user to inspect.

`public-truth chrome|edge` opens official Selenium, W3C APG, Angular Material,
PrimeVue, and Shoelace pages through default Saccade Truth. A separate
Reference Actuator process supplies test-only page stimulus; default MCP reads
the initial view and verifies the pushed transition. Public evidence contains
no action token, receipt, or editable test value. A stimulus failure remains
`blocked` or `fail` and never counts as a Truth pass. This command is an
observation regression diagnostic, not Codex dogfood and not the public
compatibility release gate. A valid end-to-end gate uses Codex or another Agent
client's own same-tab browser tool. The report therefore publishes two numbers:
`recognition_rate` for targets truthfully present in default Truth and
`closed_loop_rate` for transitions completed by the optional test stimulus.
It also closes every temporary Agent-owned case tab before starting the next
case.

`denominator` runs the complete clean-profile Truth inventory and the
11-scenario lifecycle gate in both Chrome and Edge against one candidate, then
emits a 63-row report. Local `pass` and `truthful_limitation` results remain
separate from public `publishable` evidence; the command never promotes a
Catalog row merely because its fixture passed.

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
value-free input-policy log only inside that explicit mode. A permission error
from `test-actuator`, `public-truth`, `accuracy`, or `reflex native` belongs to
that optional diagnostic harness; it is not a Saccade Runtime requirement and
must not be reported as a product or release blocker.

To share an existing HTTP or HTTPS tab, open the Saccade Extension popup in
that tab and choose **Share this tab**. The popup reports Agent On only for the
current session ACL. Choose **Stop sharing** to remove the tab, discard its
observation session, and invalidate collector tokens. Agent-owned tabs opened
through `tabs.open` are labeled `agent` by `tabs.list` and may be closed with
`tabs.close`. The close tool rejects user-shared tabs. Agents should close
temporary research tabs at task completion and retain user-facing results,
unfinished work, and tabs the user asked to keep.

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

## Public setup target

The first release uses the browser-store Extension plus one explicit command:

```sh
npx -y @saccade/setup
```

The command will install the headless local Runtime, user-level Native
Messaging manifests, and local MCP entries for supported Codex and Claude
clients. It will not install a visible app or request Accessibility. The npm
scope is not yet confirmed, so this command defines the target and is not yet
an available package. See [the setup target](docs/SETUP_TARGET.md) for the
normative install, update, doctor, uninstall, and client boundaries.

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
`./scripts/dev.sh profile set smart-barbarian-ceo`; inspect it with
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
python3 -m unittest tests/test_operation_inference_ab.py
python3 -m unittest tests/test_external_dogfood.py
python3 -m unittest tests/test_public_truth_cases.py
python3 -m unittest tests/test_dev_lifecycle.py tests/test_lifecycle_truth.py tests/test_summarize_fair_matrix.py tests/test_build_setup_release.py tests/test_audit_public_evidence.py
python3 -m unittest tests/test_truth_latency.py tests/test_denominator_evidence.py
python3 -m unittest tests/test_benchmark_same_model_fair.py
python3 -m unittest tests/test_run_same_model_matrix.py
python3 -m unittest tests/test_run_claude_same_tab.py
python3 -m unittest tests/test_probe_no_window_recovery.py
python3 -m unittest tests/test_generate_long_horizon_benchmark.py
node --test packages/setup/test/*.test.js
npm pack ./packages/setup --dry-run
python3 -m py_compile scripts/*.py
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
