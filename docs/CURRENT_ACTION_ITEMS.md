# Saccade Current Action Items

Date: 2026-06-18
Status: canonical execution queue

This file is the short, current action list. Use it with
`docs/CURRENT_PLAN.md`, `docs/next_plan_v5_tracker.md`, and
`docs/browser_compat_ledger.md`.

## Now

Active next item: AI-005 when Wayne is ready to log in to GitHub/Gist inside
Saccade. Otherwise the remaining queue is choice-based: AI-008 is parked
Canvas/WebGL investigation, and AI-001 is blocked by the pinned Servo public
API. Use `docs/site_policy_matrix.md` before dogfooding third-party logged-in
or high-risk sites.

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-001 | P1 | blocked-public-api | Browser shell | Product-grade Stop behavior. The pinned Servo/WebView public API proof is done and no safe stop-loading method is exposed. | `docs/servo_api_map.md` records the API result. Reopen implementation when moving to official ServoShell source, a newer Servo API, or a deliberate fork hook. |
| AI-005 | P0 | blocked-on-user | Editor dogfood | Wayne logs in to GitHub/Gist inside Saccade with `runs/dogfood_profile/default`; then rerun `inspect-editors` on `https://gist.github.com/new`. | `inspect-editors` reaches an authenticated editor page and records whether writable body targets are usable or zero-rect. |

## Next

| ID | Priority | Status | Owner | Action | Done When |
| --- | --- | --- | --- | --- | --- |
| AI-008 | P1 | parked | Canvas/WebGL | Keep Canvas/WebGL routed to Chrome/reference until the Servo `take_screenshot()` versus manual readback path is re-investigated. | New investigation compares official ServoShell/reference, Saccade readback, and `take_screenshot()` on the reductions. |

## Recently Closed

| ID | Closed In | Result |
| --- | --- | --- |
| AI-000 | `33a7481 add mcp browser navigate tool` | MCP now exposes `saccade.browser.navigate` for already-granted same-WebView dogfood tabs; selftest `runs/mcp/selftest_1781583895286/report.json` has `browser_navigate=true`. |
| AI-001A | current docs update | Pinned Servo `0.2.0` stop-loading API proof completed: local rustdoc exposes `load`, `load_request`, `reload`, `can_go_back`, `go_back`, `can_go_forward`, and `go_forward`, but no public `stop_loading`/`stop` equivalent. |
| AI-002 | `f84d157 make dogfood address bar editable` | Toolbar address strip now paints URL/placeholder text, secure/search icon, active focus/error state, selection, caret, and supports in-place URL editing using native GL overlay only; no page DOM injection. |
| AI-003 | current docs update | Routed away from the legacy GL toolbar. Official ServoShell's egui toolbar already resizes the WebView below browser chrome, avoiding the top-overlay issue. |
| AI-004 | current checkpoint | Source ServoShell thin fork now draws a trusted Saccade Human/Copilot badge in browser chrome, not page DOM. The badge reads existing bridge-style Copilot JSON from `SACCADE_COPILOT_STATUS_PATH` or env vars, shows granted/blocked/error states, and forces an error label if the status claims page DOM injection or sensitive-value exposure. `saccade-servoshell` now writes the status JSON and passes the env var when launching ServoShell; official ServoShell ignores it, while the thin fork displays it. Evidence: `cargo test -p servoshell saccade_copilot_badge`, `cargo check -p servoshell`, `cargo build -p servoshell --bin servoshell`, `cargo check -p saccade-servoshell`, and `runs/ai004_badge/bridge_smoke/report.json`. |
| AI-011 | current docs update | Routed away from hand-polishing GL bitmap UI. Product-quality address bar polish should come from official ServoShell UI or a thin fork that keeps that UI intact. |
| AI-012A | current checkpoint | Official ServoShell adapter selftest now includes native input/dropdown. Text input passes through WebDriver `element/value`; select reaches `gamma` via recorded `js_select_fallback` with `input/change` verification. Evidence: `runs/servoshell_adapter/adapter_1781624931973/summary.json`. |
| AI-012B | current checkpoint | Official ServoShell adapter selftest now includes same-session login handoff. Human login + explicit Done lets agent phase continue with inherited cookie/storage; password/OTP are not exposed and login screenshots are blocked. Evidence: `runs/servoshell_adapter/adapter_1781626639174/summary.json`. |
| AI-012C | current checkpoint | `saccade-servoshell bridge --smoke` now launches official ServoShell, writes an MCP-compatible current-tab grant at `runs/current_tab_grants/servoshell_latest.json`, and verifies loopback `ping/truth/actions` over the bridge. Evidence: `runs/servoshell_adapter/bridge_1781627953527/report.json`. |
| AI-012D | current checkpoint | MCP now consumes a live official ServoShell bridge grant end-to-end, reports the bridge runtime/capabilities instead of overclaiming dogfood fill/act support, and gates same-WebView calls by advertised endpoint capability. Evidence: `runs/mcp/selftest_1781629760417/report.json` and `runs/mcp/servoshell_bridge_grant_1781629827258/report.json`. |
| AI-012E | current checkpoint | Official ServoShell bridge now advertises and handles safe `fill_agent_fields`, `inspect_fields`, and non-side-effect `act`; MCP selftest verifies fill/reject sensitive, redacted inspect, navigate, and safe click through the live bridge. Evidence: `runs/mcp/selftest_1781631232204/report.json` and `runs/mcp/servoshell_bridge_grant_1781631275577/report.json`. |
| AI-012F | current checkpoint | Official ServoShell bridge now advertises and handles `formmax_live_fill`; MCP selftest navigates the same granted ServoShell bridge tab to the FORMMAX fixture and verifies rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, validation_errors=0. Evidence: `runs/mcp/selftest_1781632241481/report.json` with `servoshell_bridge_formmax_live=true` and `runs/mcp/servoshell_bridge_grant_1781632282911/report.json`. |
| AI-012G | current checkpoint | Official ServoShell bridge control calls now write sanitized `control/report.json` and `control/replay.jsonl` artifacts, append FORMMAX no-value replay events, and expose Copilot grant state through `shell_status`. MCP selftest verifies `servoshell_bridge_artifacts=true` and replay summary readability. Evidence: `runs/mcp/selftest_1781636671768/report.json`, `runs/mcp/servoshell_bridge_grant_1781636716084/control/report.json`, and `runs/mcp/servoshell_bridge_grant_1781636716084/control/replay.jsonl`. |
| AI-012H | current checkpoint | Official ServoShell bridge upgradeability gate passes against the local source-release ServoShell `0.3.0-805e6a423` by rerunning the same MCP bridge attach/fill/inspect/act/FORMMAX/artifact gate with `SACCADE_SERVOSHELL_BIN=/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell`. Evidence: `runs/mcp/selftest_1781636405474/report.json` and `runs/mcp/servoshell_bridge_grant_1781636453696/control/replay.jsonl`. |
| AI-012 | current checkpoint | Closed the official ServoShell bridge product gate as the default Saccade dogfood human-browser path. Installed official ServoShell keeps the human UI intact while Saccade attaches a loopback bridge for redacted truth/actions, safe fill/inspect/act, FORMMAX live fill, navigation, replay artifacts, and Copilot state. Visible trusted badge is explicitly routed to AI-004. Evidence: `runs/servoshell_adapter/ai012_close_bridge_smoke_1781794791/report.json`, `cargo test -p saccade-servoshell`, and `cargo check -p saccade-mcp`. |
| AI-013A | current docs update | Added `docs/site_policy_matrix.md`, the first practical Green/Yellow/Orange/Red site list for Saccade dogfood, high-risk site fallback, and no-bypass boundaries. |
| AI-013B | current checkpoint | Implemented the shared site/action risk classifier in `saccade_core`, exposed `site_policy` through MCP and the official ServoShell bridge, and gate Red-site reads plus high-risk fill/action attempts. Evidence: `runs/mcp/selftest_1781641440418/report.json`. |
| AI-013C | current checkpoint | Official ServoShell bridge control errors now write a redacted `control/block_report.json` with query-free URL, site policy, request id extraction, visible block excerpt, and fallback recommendation. Evidence: `cargo test -p saccade-servoshell block_report`. |
| AI-013D | current checkpoint | MCP now exposes `saccade.report.redacted_note`, a safe copy/paste fallback path that writes `note.json`, `redacted_note.md`, and `ai_review_prompt.md` under `runs/redacted_notes/` for AI evaluation/editing without live-site access. Evidence: `runs/mcp/selftest_1781645696687/report.json`. |
| AI-013E | current checkpoint | Added owned-domain policy lanes through `SACCADE_OWNED_DOMAINS`; MCP and official ServoShell bridge classify explicitly owned non-high-risk domains as `owned_domain` Green while preserving auth/financial/government/high-risk overrides. Evidence: `cargo test -p saccade_core owned_domains`. |
| AI-013F | current checkpoint | Added paste-ready Saccade dogfood/policy handoff instructions for other Codex sessions, including risk classes, owned-domain launch command, high-risk fallback, and `saccade.report.redacted_note`. Evidence: `docs/SACCADE_DOGFOOD_HANDOFF.md`. |
| AI-013G | current checkpoint | Added `scripts/create_redacted_note_packet.js`, a convenience CLI that turns user-supplied redacted text into the existing MCP redacted-note packet without live-site access. Evidence: App Store Connect blocker sample run under `runs/redacted_notes/`. |
| AI-013H | current checkpoint | Made site policy evidence-first: unknown third-party sites are `unmeasured_unknown` Yellow, and site-specific policy changes require real dogfood artifacts, reference-browser comparison, provider block evidence, or primary-source high-impact proof. Evidence: `cargo test -p saccade_core site_policy`. |
| AI-009 | current checkpoint | Closed DEVMAX gauntlet evidence polish for browser-backed reports: Servo probe audits now write full-page screenshots, per-finding crop PNG artifacts, attach `screenshot_crop` evidence to each finding, record multi-action click receipts in report/replay, and the Servo fixture gate fails if any browser-backed finding lacks a crop or no case verifies multiple actions. Evidence: `runs/devmax/servo_selftest_1781796265942/summary.json`, `cargo check -p devmax`, `cargo run -q -p devmax -- selftest-servo-fixtures`, and `cargo run -q -p devmax -- selftest-fixtures`. |
| AI-006 | current checkpoint | Added `test_pages/visual_parity/font_control_metrics/`, extra Chrome/Saccade probe fields for text/font/client/scroll metrics, and `scripts/browser_compat_metrics.py` with `GREEN/YELLOW/RED/INVALID_VIEWPORT` classification. Evidence: `docs/browser_compat_metrics_report.md`. |
| AI-007 | current checkpoint | `scripts/browser_compat_metrics.py` now validates requested CSS viewport against Chrome truth, Saccade truth, and Saccade runtime geometry before trusting large-width comparisons. Evidence: `runs/browser_compat_metrics/metrics_1781650486498/browser_compat_metrics.json` marks `1600x760` as `INVALID_VIEWPORT` because Saccade is capped at `1440x760`. |
| AI-010 | current checkpoint | Added a local dogfood release plan and kit builder. `scripts/build_dogfood_release.sh` builds release binaries into `dist/saccade-dogfood-*/` with `open-saccade`, `servoshell-bridge`, profile dir, env file, and docs. Icon decision: distinct Saccade icon; do not reuse official Servo icon without permission. Evidence: `docs/dogfood_release_plan.md`. |
