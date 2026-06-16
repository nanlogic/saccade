# Saccade Current Action Items

Date: 2026-06-16
Status: canonical execution queue

This file is the short, current action list. Use it with
`docs/CURRENT_PLAN.md`, `docs/next_plan_v5_tracker.md`, and
`docs/browser_compat_ledger.md`.

## Now

Active next item: AI-012. AI-001 is blocked by the pinned Servo public API.

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-001 | P1 | blocked-public-api | Browser shell | Product-grade Stop behavior. The pinned Servo/WebView public API proof is done and no safe stop-loading method is exposed. | `docs/servo_api_map.md` records the API result. Reopen implementation when moving to official ServoShell source, a newer Servo API, or a deliberate fork hook. |
| AI-004 | P1 | open | Browser shell | Add a visible Human/Agent/Copilot state badge that normal users can understand at a glance. | Dogfood window visibly communicates Human-owned tab, agent grant state, and error state; MCP `shell_status` includes the same state. |
| AI-005 | P0 | blocked-on-user | Editor dogfood | Wayne logs in to GitHub/Gist inside Saccade with `runs/dogfood_profile/default`; then rerun `inspect-editors` on `https://gist.github.com/new`. | `inspect-editors` reaches an authenticated editor page and records whether writable body targets are usable or zero-rect. |
| AI-012 | P0 | open | ServoShell adapter | Promote official ServoShell UI as the human browser path and attach Saccade's agent bridge to that runtime instead of further productizing the legacy GL toolbar. | Official ServoShell UI remains intact; Saccade can collect redacted truth/actions, dispatch safe actions, record replay, and pass local game/FORMMAX/current-tab safety gates through that path or trigger the thin-fork fallback. |

## Next

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-006 | P1 | open | Browser compatibility | Add font/line-height/control text fixture and Chrome/Saccade metrics. | Fixture emits computed styles, text rects, screenshots, and a pass/yellow/red classification. |
| AI-007 | P1 | open | Browser compatibility | Add display-boundary/fullscreen probe before using 1600/1920 viewport benchmarks. | Large viewport tests report actual CSS/device size and refuse invalid comparisons. |
| AI-008 | P1 | parked | Canvas/WebGL | Keep Canvas/WebGL routed to Chrome/reference until the Servo `take_screenshot()` versus manual readback path is re-investigated. | New investigation compares official ServoShell/reference, Saccade readback, and `take_screenshot()` on the reductions. |
| AI-009 | P1 | open | DEVMAX | Add screenshot/finding crops and multi-action click verification polish. | DEVMAX gauntlet reports include crops for findings and verified multi-action receipts. |
| AI-010 | P2 | open | Packaging | Add macOS packaging/signing checklist and dev-signed local path. | A doc or script explains the unsigned/dev-signed run path without mixing with renderer fixes. |

## Recently Closed

| ID | Closed In | Result |
| --- | --- | --- |
| AI-000 | `33a7481 add mcp browser navigate tool` | MCP now exposes `saccade.browser.navigate` for already-granted same-WebView dogfood tabs; selftest `runs/mcp/selftest_1781583895286/report.json` has `browser_navigate=true`. |
| AI-001A | current docs update | Pinned Servo `0.2.0` stop-loading API proof completed: local rustdoc exposes `load`, `load_request`, `reload`, `can_go_back`, `go_back`, `can_go_forward`, and `go_forward`, but no public `stop_loading`/`stop` equivalent. |
| AI-002 | `f84d157 make dogfood address bar editable` | Toolbar address strip now paints URL/placeholder text, secure/search icon, active focus/error state, selection, caret, and supports in-place URL editing using native GL overlay only; no page DOM injection. |
| AI-003 | current docs update | Routed away from the legacy GL toolbar. Official ServoShell's egui toolbar already resizes the WebView below browser chrome, avoiding the top-overlay issue. |
| AI-011 | current docs update | Routed away from hand-polishing GL bitmap UI. Product-quality address bar polish should come from official ServoShell UI or a thin fork that keeps that UI intact. |
| AI-012A | current checkpoint | Official ServoShell adapter selftest now includes native input/dropdown. Text input passes through WebDriver `element/value`; select reaches `gamma` via recorded `js_select_fallback` with `input/change` verification. Evidence: `runs/servoshell_adapter/adapter_1781624931973/summary.json`. |
| AI-012B | current checkpoint | Official ServoShell adapter selftest now includes same-session login handoff. Human login + explicit Done lets agent phase continue with inherited cookie/storage; password/OTP are not exposed and login screenshots are blocked. Evidence: `runs/servoshell_adapter/adapter_1781626639174/summary.json`. |
| AI-012C | current checkpoint | `saccade-servoshell bridge --smoke` now launches official ServoShell, writes an MCP-compatible current-tab grant at `runs/current_tab_grants/servoshell_latest.json`, and verifies loopback `ping/truth/actions` over the bridge. Evidence: `runs/servoshell_adapter/bridge_1781627953527/report.json`. |
| AI-012D | current checkpoint | MCP now consumes a live official ServoShell bridge grant end-to-end, reports the bridge runtime/capabilities instead of overclaiming dogfood fill/act support, and gates same-WebView calls by advertised endpoint capability. Evidence: `runs/mcp/selftest_1781629760417/report.json` and `runs/mcp/servoshell_bridge_grant_1781629827258/report.json`. |
| AI-012E | current checkpoint | Official ServoShell bridge now advertises and handles safe `fill_agent_fields`, `inspect_fields`, and non-side-effect `act`; MCP selftest verifies fill/reject sensitive, redacted inspect, navigate, and safe click through the live bridge. Evidence: `runs/mcp/selftest_1781631232204/report.json` and `runs/mcp/servoshell_bridge_grant_1781631275577/report.json`. |
| AI-012F | current checkpoint | Official ServoShell bridge now advertises and handles `formmax_live_fill`; MCP selftest navigates the same granted ServoShell bridge tab to the FORMMAX fixture and verifies rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, validation_errors=0. Evidence: `runs/mcp/selftest_1781632241481/report.json` with `servoshell_bridge_formmax_live=true` and `runs/mcp/servoshell_bridge_grant_1781632282911/report.json`. |
| AI-012G | current checkpoint | Official ServoShell bridge control calls now write sanitized `control/report.json` and `control/replay.jsonl` artifacts, append FORMMAX no-value replay events, and expose Copilot grant state through `shell_status`. MCP selftest verifies `servoshell_bridge_artifacts=true` and replay summary readability. Evidence: `runs/mcp/selftest_1781634612147/report.json`, `runs/mcp/servoshell_bridge_grant_1781634660859/control/report.json`, and `runs/mcp/servoshell_bridge_grant_1781634660859/control/replay.jsonl`. |
