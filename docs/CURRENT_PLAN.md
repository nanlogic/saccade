# Saccade Current Plan

Date: 2026-06-15
Status: active source of truth

## One Sentence

Saccade is becoming a human-safe AI browser layer: the user can browse normally,
grant an agent access to the current tab, and get redacted browser truth,
verified actions, policy gates, and replay.

## What Is Already Proven

### MOUSEMAX / Reflex Trust

- Real benchmark proof exists for rendered truth, fast input, verification, and
  replay.
- Local game reflex now runs through release ServoShell, consumes semantic
  browser facts, drives motor commands, and writes `review.html`.

Key docs:

- `docs/m7_benchmark_report.md`
- `docs/local_game_reflex_gate.md`
- `docs/browser_fact_stream.md`

### FORMMAX / Practical Forms

- Long two-page table fixture passes.
- Non-sensitive fields can be filled.
- Sensitive fields are blocked and marked for user input.
- Receipts and replay are validated without leaking values.
- Live browser worker path exists.

Key docs:

- `docs/m10_formmax_fixture_report.md`
- `docs/browser_session_report.md`
- `docs/user_flow_selftest_report.md`

### Trusted Tabs / Safety

- Human tabs deny agent input.
- Agent truth masks sensitive values.
- Login handoff and profile persistence have local passes.
- Worker can fill constrained agent-owned non-sensitive fields.
- Worker can inspect explicitly named non-sensitive fields while masking
  sensitive fields.
- MCP now exposes a current-tab co-pilot grant for local tabs: Human ownership
  stays visible, agent gets redacted truth, safe field fill, and submit remains
  user-confirmed.

Key docs:

- `docs/tabs_runtime_profile.md`
- `docs/login_handoff_profile.md`
- `docs/profile_persistence_report.md`
- `docs/safety_truth_profile.md`

## Current Product Gap

The remaining product bridge is:

```text
User opens a browser tab first.
Agent attaches to that current tab after explicit user grant.
```

N8 now has both a local shell selftest and an MCP API gate for this bridge.
What remains is binding it to visible browser UI and eventually real selected
tabs instead of local fixture URLs.

## Next Gate: N8 Current Tab Co-Pilot

Status: local v0 pass + MCP API pass.

Commands:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-current-tab-copilot
RUST_LOG=error cargo run -q -p saccade-mcp -- selftest
```

Latest evidence:

```text
CURRENT_TAB_COPILOT PASS selected_tab_seen=true grant_required=true redacted_truth=true agent_explains_page=true non_sensitive_filled=true sensitive_write_blocked=true sensitive_values_exposed=false confirmation_required=true replay=runs/browser_session_worker/worker_1781535424701_32946/replay.jsonl report=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_copilot/copilot_1781535424558/report.json
MCP PASS tools_registered=20 tab_scoping=true local_dev_audit=true policy_gate=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781535319538/report.json
```

### Goal

```text
CURRENT_TAB_COPILOT PASS
selected_tab_seen=true
user_grant_required=true
agent_read_redacted_truth=true
agent_explains_page=true
agent_fills_non_sensitive=true
sensitive_values_exposed=false
user_can_complete_sensitive=true
submit_requires_confirmation=true
replay_written=true
```

### User Story

1. User opens Saccade and navigates to a local form page.
2. User clicks or triggers "let agent help on this tab".
3. Agent receives redacted truth and action map for the selected tab.
4. Agent explains what the page is asking for.
5. Agent fills ordinary fields.
6. User fills sensitive fields directly in the browser.
7. Agent checks only completion/status for sensitive fields.
8. Submit or external side effects require user confirmation.
9. Run writes `report.json` and `replay.jsonl`.

### First Implementation Shape

- Reuse the existing browser worker, truth redaction, and safe-fill paths.
- Add selected-tab discovery and explicit grant state.
- Keep it local-first with `test_pages/login_handoff/user_flow.html` or a new
  current-tab fixture.
- Do not start with arbitrary third-party sites.

Current v0 uses `test_pages/current_tab_copilot/index.html`. The shell selftest
simulates the grant boundary, and MCP exposes it as
`saccade.tabs.grant_current`. The next step is making the grant boundary visible
in the browser shell and binding it to a real selected tab.

### Done When

Run command or selftest prints:

```text
CURRENT_TAB_COPILOT PASS selected_tab_seen=true grant_required=true redacted_truth=true non_sensitive_filled=true sensitive_values_exposed=false confirmation_required=true replay=...
```

## Priority Order

1. N8 Current Tab Co-Pilot.
2. Browser shell basics: clickable URL bar, Back, Forward, Reload, Stop,
   visible Human/Agent badge.
3. Current-tab FORMMAX: run long form fill inside the user-granted tab.
4. Editor/contenteditable gate: Gist-like editor and forum composer.
5. DEVMAX gauntlet polish: multi-action verification, screenshots/finding crops,
   Chrome comparison.
6. MOUSEMAX evidence freeze/video/public report.
7. Mac packaging/signing.

## Parking Lot

- IGN and similar sites where official Servo also struggles.
- Full WebGL/canvas product work unless it blocks local game or developer
  dogfood.
- Public launch package until current-tab co-pilot and browser shell basics are
  understandable to a normal user.
