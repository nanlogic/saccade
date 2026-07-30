# Saccade

[![CI](https://github.com/nanlogic/saccade/actions/workflows/ci.yml/badge.svg)](https://github.com/nanlogic/saccade/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Saccade gives AI agents a compact semantic view of an authorized Chrome or
Edge tab and executes browser actions through registered closed-loop input.
Each action follows one closed loop:

```text
observe → prepare → revalidate → input → reobserve → verify → receipt
```

Agents receive semantic controls and opaque action tokens. They do not receive
selectors, DOM paths, arbitrary coordinates, editable values, cookies, or
browser storage.

## Status

Saccade is pre-release. The current vertical slice runs through the complete
Extension → Native Host → Runtime → MCP route on managed macOS Chrome for
Testing and Microsoft Edge profiles. Fifteen actionable controls are currently
in the Registry. Button, editable controls, checkbox, radio, switch, the
select/listbox/combobox slice, tab, and menu item have paired Chrome/Edge
development evidence; reflex target and file input currently have focused
Chrome evidence. Paired managed run `20260730T002519Z` produced 15 verified
receipts in both browsers: seven automatically
selected software clicks and eight native actions, including link, native
select, ARIA listbox, ARIA combobox, structural reading, stale-token rejection,
Profile gates, and the local software-to-native learning gate.

Radio, ARIA switch, tab, and menu item also pass public W3C WAI-ARIA examples
in both browsers. Run `20260729T211221Z` matched all four Saccade native results
against an isolated Playwright reference oracle without using Playwright as an
action fallback.

| Control | Action | Verified postcondition |
| --- | --- | --- |
| Button | automatic software/native click | pressed or expanded state changes |
| Link | automatic software/native click | top-level document identity changes |
| Text field | native click and Unicode input | field changes from empty to non-empty |
| Search field | native click and Unicode input | field changes from empty to non-empty |
| Textarea | native click and multiline Unicode input | field changes from empty to non-empty |
| Contenteditable | native click and Unicode input | editable region changes from empty to non-empty |
| Spin button | native click and numeric text input | field changes from empty to non-empty |
| Checkbox | automatic software/native click | checked state changes |
| Radio | automatic software/native click | target checked state changes |
| ARIA switch | automatic software/native click | checked state changes |
| Select, listbox, combobox | native indexed selection by option identity | requested option becomes selected |
| Tab | automatic software/native click | target becomes selected |
| Menu item | automatic software/native click | expanded state changes |
| Reflex target | automatic software/native click | same-loop score/occurrence advances |
| File input | native operating-system chooser | real file input accepts a non-empty selection |

The paired development run covers stale-token rejection, Profile behavior,
Profile bans, and editable-value leak checks in both browsers. Catalog entries
remain `implementation` until Chrome and Edge pass the release gate for the
same candidate. Saccade does not ship a consumer installer yet.

See the [generated coverage table](docs/generated/control_coverage.md) for the
current Registry and [control roadmap](docs/CONTROL_ROADMAP.md) for the planned
batches. The [Developer Preview release plan](docs/RELEASE_PLAN.md) defines the
product, evidence, packaging, and launch gates.

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
./scripts/dev.sh compare all
./scripts/dev.sh accuracy chrome
./scripts/dev.sh accuracy all
./scripts/dev.sh reflex chrome soft
./scripts/dev.sh reflex chrome native
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
not make the managed jobs depend on repository-folder access. Development
profiles use an explicit generation independent of Extension version, while
the unpacked Extension directory is versioned. This forces MV3 code updates to
load without reading or copying browser cookies; advancing the profile
generation leaves the prior profile untouched. User Profile JSON remains in
the Runtime directory and is never overwritten.

`test` calls `tabs.open → web.observe → web.act` through MCP JSON-RPC. `tabs.open`
waits for the first authorized Truth Layer. The first Agent Browser view is
full; later results contain semantic deltas and opaque authority refreshes
instead of repeating the page. `saccade.web.form.fill` accepts one bounded form
plan and locally runs every field through its independent closed loop; submit
and navigation remain separate actions. Managed testing stores
evidence under `~/Library/Application Support/Saccade Dev/evidence` and omits
editable contents. Managed evidence commands (`test`, `compare`, `accuracy`,
and `reflex`) temporarily isolate and restore the user's local input policy so
conformance runs cannot teach day-to-day browsing rules.

`web.act` chooses the backend automatically. The Catalog supplies the default:
software for finite click controls and native input for editable, select, and
file-input controls. A receipt-backed local log at
`~/Library/Application Support/Saccade Dev/runtime/input-policy.json` remembers
page/control exceptions. If software input is accepted but cannot verify the
effect, Saccade records native for the next fresh action; it never retries the
same action with the real mouse. `saccade.input_policy.list` exposes this
value-free log, and `saccade.input_policy.remember_native` records an explicit
stronger choice for a current token. Profile remains only `name / behavior /
ban` and does not select an input backend.

```json
{
  "schema": "saccade.input-policy/1",
  "rules": [
    {
      "page": "https://example.com/settings",
      "role": "button",
      "control": "Save",
      "backend": "native",
      "evidence": "user_remembered_native"
    }
  ]
}
```

To share an existing HTTP or HTTPS tab, open the Saccade Extension popup in
that tab and choose **Share this tab**. The popup reports Agent On only for the
current session ACL. Choose **Stop sharing** to remove the tab, discard its
observation session, and invalidate collector tokens. Agent-owned tabs opened
through `tabs.open` are revoked by closing them.

`compare` first requires Saccade to complete the radio, switch, tab, and menu
item loops independently on public W3C WAI-ARIA pages. It then runs an isolated
Playwright oracle in fresh contexts, compares accessible names and false-to-true
state transitions, and saves oracle screenshots. Playwright is test-only: it
cannot create, repair, or replace a Saccade receipt.

`accuracy` runs an ordinary static-target gate through the same MCP and native
input route. It clicks 24 semantic buttons across left, center, right, and
scrolled positions: eight each at 32, 40, and 48 CSS pixels. The 24 targets are
split across baseline, moved, and moved-and-resized exact-window phases. Passing
requires 24 verified postconditions and zero misses; it is not the high-rate reflex loop.
An unrelated always-on-top desktop overlay can still cause a truthful
unverified native result.

`reflex` opens the real MouseAccuracy game, reaches its audited highest settings
(`Insane` and `Tiny`) through semantic native button actions, then runs one
bounded local MCP loop. `soft` is a token-bound Extension software mouse and
`native` remains the real OS mouse. Neither accepts Agent coordinates. The
canvas itself stays opaque; only the site's current audited DOM target bridge is
actionable, and every successful receipt requires the score to advance.

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
python3 -m py_compile scripts/dev_probe.py scripts/external_dogfood.py scripts/compare_external_evidence.py scripts/benchmark_playwright_parity.py scripts/benchmark_selenium_qa.py
python3 scripts/generate_control_matrix.py
python3 scripts/check_single_architecture.py
git diff --exit-code -- docs/generated/control_coverage.md
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before adding a control family. Report
security issues through GitHub's private vulnerability-reporting flow described
in [SECURITY.md](SECURITY.md).

Apache-2.0. See [TRADEMARKS.md](TRADEMARKS.md) for the Saccade name and marks.
