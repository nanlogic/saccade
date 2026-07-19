# AI-034 Human/Agent Agreement

Date: 2026-07-15
Status: CEF migration gate complete

## Result

CEF now exposes the existing `saccade.web.render_preflight` contract through
the capability-based current-tab adapter. One synchronous renderer snapshot
measures redacted field facts, editor geometry, and center-point renderer
hit-tests. The browser process binds that snapshot to its trusted current URL
and start/end page revision before it permits normal agent input.

| Canary | Result | Route |
| --- | --- | --- |
| Local actionable fixture | `green`; 2/2 renderer hit-tests agree, one hidden zero-rect backing editor ignored, one revision | `cef` |
| Same local fixture while expecting `github_issue` | `red`; trusted URL is not a GitHub Issue authoring surface | `navigate_task_surface` |
| Local fixture with a different surface covering both proposed points | `red`; 0/2 renderer hit-tests agree | `block` |
| Logged-in `https://github.com/servo/servo/issues/new` | `green`; 3 fields, 3 eligible, 3/3 renderer hit-tests agree, one revision | `cef` |
| Logged-in GitHub account menu | Fact-bound native CEF click receipt verified; Settings and Sign out observed | normal CEF action policy |

The GitHub run wrote no fields, submitted nothing, did not click Sign out, and
captured no screenshot. The local gate also carries a populated password
sentinel; the sentinel is absent from the response and value-free replay.

## Contract

`saccade.web.render_preflight` accepts:

```json
{"tab_id":1,"expected_surface":"github_issue"}
```

Supported task surfaces remain `page`, `github_issue`, and
`github_discussion`. CEF advertises `render_preflight` in its owner-only grant,
so the existing MCP tool routes without an engine-specific API.

The result includes:

- redacted field/editor counts and geometry-derived visibility;
- start and end browser page revisions;
- trusted browser URL task-surface matching;
- renderer center-point hit-test counts and accuracy;
- typed reason codes plus `green`, `yellow`, or `red` routing;
- explicit privacy and screenshot status.

A page revision change routes to `refresh_replan`. A task URL mismatch routes
to `navigate_task_surface`. A visible fact whose proposed center hits another
surface routes to `block`.

## Evidence Boundary

The CEF measurement is labeled `cef_renderer_observed`. Its hit result is a
renderer/Blink structural hit-test, not an OS-native proof and not authority.
The separate GitHub account-menu canary supplies one actual CEF pointer receipt.
Page content still cannot authorize a side effect.

`full_agreement_measured=false` remains correct because screenshots are off by
default and no independent human reference image is supplied. Guarded visual
evidence remains optional for a user-authorized, no-protected-value page; it is
not required for structural green.

## Evidence

- `runs/cef_ai034/local_gate_20260715/report.json`
- `runs/cef_ai034/github_canary_20260715_final/report.json`
- `runs/ai034_human_agent_agreement/github_dashboard_expected_issue_20260713/report.json`
- `runs/ai034_human_agent_agreement/github_new_issue_expected_issue_20260713/report.json`
- `runs/ai034_human_agent_agreement/github_account_menu_agreement_20260713/report.json`

The CEF Release build passed with sandbox enabled. The public MCP unit suite
passed 9/9. Saved-profile canaries now refuse an ad-hoc app before launch; the
tested app is signed as `ai.saccade.browser` with Team ID `48KK2UWXQM`.

## Gate

```sh
SACCADE_CODESIGN_IDENTITY=auto engines/cef/scripts/build_macos.sh
python3 scripts/probe_cef_human_agent_agreement.py \
  --output-dir runs/cef_ai034/local_gate_20260715
python3 scripts/probe_cef_github_canary.py \
  --output-dir runs/cef_ai034/github_canary_20260715_final
cargo test -p saccade-mcp
```

## Milestone Report

```text
MILESTONE: AI-034 CEF human/agent agreement migration
GATE: local CEF agreement probe + logged-in GitHub canary + cargo test -p saccade-mcp -> PASS
MEASURED: local green 2/2; local occluded 0/2 and blocked; GitHub 3/3; MCP 9/9
DEVIATIONS: renderer hit-test is labeled renderer-observed, not native OS truth
SERVO API NOTES: none
RISKS RAISED/RETIRED: repeated Keychain prompts prevented by signed-profile preflight; optional visual evidence remains off by default
NEXT: AI-033 CEF adversarial safety migration gate
```
