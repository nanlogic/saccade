# Saccade Current Action Items

Date: 2026-06-16
Status: canonical execution queue

This file is the short, current action list. Use it with
`docs/CURRENT_PLAN.md`, `docs/next_plan_v5_tracker.md`, and
`docs/browser_compat_ledger.md`.

## Now

Active next item: AI-003. AI-001 is blocked by the pinned Servo public API.

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-001 | P1 | blocked-public-api | Browser shell | Product-grade Stop behavior. The pinned Servo/WebView public API proof is done and no safe stop-loading method is exposed. | `docs/servo_api_map.md` records the API result. Reopen implementation when moving to official ServoShell source, a newer Servo API, or a deliberate fork hook. |
| AI-003 | P1 | open | Browser shell | Remove or route around the top 44 CSS px toolbar overlay so browser chrome does not obscure page content or action maps. | Toolbar no longer covers page content, or reports a measured safe inset/routing policy; action coordinates stay page-true. |
| AI-004 | P1 | open | Browser shell | Add a visible Human/Agent/Copilot state badge that normal users can understand at a glance. | Dogfood window visibly communicates Human-owned tab, agent grant state, and error state; MCP `shell_status` includes the same state. |
| AI-005 | P0 | blocked-on-user | Editor dogfood | Wayne logs in to GitHub/Gist inside Saccade with `runs/dogfood_profile/default`; then rerun `inspect-editors` on `https://gist.github.com/new`. | `inspect-editors` reaches an authenticated editor page and records whether writable body targets are usable or zero-rect. |

## Next

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-006 | P1 | open | Browser compatibility | Add font/line-height/control text fixture and Chrome/Saccade metrics. | Fixture emits computed styles, text rects, screenshots, and a pass/yellow/red classification. |
| AI-007 | P1 | open | Browser compatibility | Add display-boundary/fullscreen probe before using 1600/1920 viewport benchmarks. | Large viewport tests report actual CSS/device size and refuse invalid comparisons. |
| AI-008 | P1 | parked | Canvas/WebGL | Keep Canvas/WebGL routed to Chrome/reference until the Servo `take_screenshot()` versus manual readback path is re-investigated. | New investigation compares official ServoShell/reference, Saccade readback, and `take_screenshot()` on the reductions. |
| AI-009 | P1 | open | DEVMAX | Add screenshot/finding crops and multi-action click verification polish. | DEVMAX gauntlet reports include crops for findings and verified multi-action receipts. |
| AI-010 | P2 | open | Packaging | Add macOS packaging/signing checklist and dev-signed local path. | A doc or script explains the unsigned/dev-signed run path without mixing with renderer fixes. |
| AI-011 | P2 | open | Browser shell polish | Replace the GL bitmap toolbar text/buttons with platform-quality browser chrome once the shell layout contract is stable. | The address bar looks like standard macOS browser chrome while preserving page-DOM isolation and same-WebView agent control. |

## Recently Closed

| ID | Closed In | Result |
| --- | --- | --- |
| AI-000 | `33a7481 add mcp browser navigate tool` | MCP now exposes `saccade.browser.navigate` for already-granted same-WebView dogfood tabs; selftest `runs/mcp/selftest_1781583895286/report.json` has `browser_navigate=true`. |
| AI-001A | current docs update | Pinned Servo `0.2.0` stop-loading API proof completed: local rustdoc exposes `load`, `load_request`, `reload`, `can_go_back`, `go_back`, `can_go_forward`, and `go_forward`, but no public `stop_loading`/`stop` equivalent. |
| AI-002 | current workspace | Toolbar address strip now paints URL/placeholder text, secure/search icon, active focus/error state, and caret using native GL overlay only; no page DOM injection. |
