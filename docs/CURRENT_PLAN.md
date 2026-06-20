# Saccade Current Plan

Date: 2026-06-16
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

N8 now has a local shell selftest, an MCP API gate, a visible dogfood browser
grant shortcut, MCP import of that grant artifact, and a same-WebView control
ping plus redacted truth/actions, safe field fill, redacted field inspect, and
safe non-side-effect act from MCP into the already-open dogfood window. Current
tab FORMMAX also runs inside the same user-granted dogfood WebView. The
dogfood browser now has a visible native toolbar v0 with shell-consumed
Back/Forward/Reload/address/Copilot hit-zones plus visible shell-owned address
text, focus, selection, caret, and basic in-place URL editing. MCP also exposes
`saccade.browser.navigate` for status/navigate/reload/back/forward on an
already-granted same-WebView dogfood tab. Submit and other side-effect actions
still require user confirmation.

## Next Gate: N8 Current Tab Co-Pilot

Status: local v0 pass + MCP API pass + same-WebView co-pilot bridge pass +
current-tab FORMMAX pass.

Commands:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-current-tab-copilot
RUST_LOG=error cargo run -q -p saccade-mcp -- selftest
```

Latest evidence:

```text
CURRENT_TAB_COPILOT PASS selected_tab_seen=true grant_required=true redacted_truth=true agent_explains_page=true non_sensitive_filled=true sensitive_write_blocked=true sensitive_values_exposed=false confirmation_required=true replay=runs/browser_session_worker/worker_1781535424701_32946/replay.jsonl report=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_copilot/copilot_1781535424558/report.json
MCP PASS tools_registered=21 tab_scoping=true local_dev_audit=true policy_gate=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781583895286/report.json
DOGFOOD_GRANT status=granted owner=Human read_grant=FullTruth agent_input_grant=true artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/smoke.json
SAME_WEBVIEW_CONTROL ok=true same_webview_control_ping=true transport_status=same_webview_control_ping_plus_worker_truth_v0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_control_smoke_1781572417690.json artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_bridge_smoke.json
SAME_WEBVIEW_TRUTH_ACTIONS ok=true same_webview_attached=true transport_status=same_webview_control_truth_v0 truth_runtime=saccade-dogfood-control-v0 actions_runtime=saccade-dogfood-control-v0 actions_count=6 report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_truth_actions_smoke_1781575838106.json artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_truth_actions_smoke.json
SAME_WEBVIEW_FILL_ACT ok=true fill_runtime=saccade-dogfood-control-v0 inspect_runtime=saccade-dogfood-control-v0 safe_act_runtime=saccade-dogfood-control-v0 filled=3 rejected_sensitive=2 values_redacted=2 submit_blocked=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_fill_act_smoke_1781576647007.json artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_fill_act_smoke.json
SAME_WEBVIEW_FORMMAX ok=true fill_runtime=saccade-dogfood-control-v0 engine=saccade-dogfood-control-formmax-live-v0 rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true validation_errors=0 replay_events=2711 report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_formmax_smoke_1781578030042.json artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_formmax_live_smoke.json
SAME_WEBVIEW_SHELL_NAV ok=true runtime=saccade-dogfood-control-v0 initial=current_tab_copilot navigated=formmax reload_changed=true back_changed=true forward_changed=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_shell_nav_smoke_1781579239152.json artifact=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_shell_nav_smoke.json
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
simulates the grant boundary, MCP exposes it as `saccade.tabs.grant_current`,
and the dogfood browser exposes `Cmd+Shift+G` as a visible current-tab grant
that writes `runs/current_tab_grants/latest.json`. MCP can now consume that
artifact via `grant_path` and ping the same live dogfood WebView through the
artifact's loopback `control_endpoint`. MCP now reads redacted truth and action
maps from that same live dogfood WebView, fills agent-owned non-sensitive
fields, inspects explicitly requested fields with sensitive values masked,
dispatches safe non-side-effect actions, and runs the long FORMMAX local
fixture in the user-granted tab. The same control endpoint also exposes
primitive shell navigation commands (`shell_status`, `navigate`, `reload`,
`back`, `forward`) for the already-open dogfood window, and MCP wraps those as
the named `saccade.browser.navigate` tool for already-granted same-WebView
tabs. Submit remains user-confirmed.

### Done When

Run command or selftest prints:

```text
CURRENT_TAB_COPILOT PASS selected_tab_seen=true grant_required=true redacted_truth=true non_sensitive_filled=true sensitive_values_exposed=false confirmation_required=true replay=...
```

## Priority Order

Canonical queue: `docs/CURRENT_ACTION_ITEMS.md`.

1. Editor/contenteditable/auth gate: Gist-like editor detection, same-process
   live authenticated Gist draft fill, and local ServoShell bridge profile
   persistence are closed. Real providers may still require same-process login
   because of their own session/device policies.
2. DEVMAX follow-up: HTTP status awareness for resource loads and Chrome
   comparison polish. Browser-backed finding crops and multi-action receipts
   are closed in AI-009.
3. Browser layout/API follow-up: source ServoShell window resize math is fixed,
   local right-edge dropdown resize fixtures pass, and GitHub account-menu
   overflow is routed as Servo Web API compatibility (`IntersectionObserver`
   and adopted stylesheet APIs missing in source-release and official
   Servo.app).
4. Dogfood release packaging/signing so other sessions can use the current
   ServoShell bridge reliably.
5. MOUSEMAX evidence freeze/video/public report.

## Parking Lot

- IGN and similar sites where official Servo also struggles.
- Full WebGL/canvas product work unless it blocks local game or developer
  dogfood.
- Public launch package until current-tab co-pilot and the ServoShell-backed
  human browser path are understandable to a normal user.
