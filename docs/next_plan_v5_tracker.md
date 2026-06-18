# Saccade Next Plan v5 Tracker

Date: 2026-06-11
Updated: 2026-06-15

Canonical companion: `docs/CURRENT_PLAN.md`.

## Note

`SACCADE_NEXT_PLAN_v5.md` has a numbering conflict:

- Section 2 says `N2 = DEVMAX`.
- Section 4 says `N2 = Login Handoff`.
- Decision Summary says `N2 = DEVMAX`.

Use this tracker as the normalized execution map.

## Normalized Order

| Track | Status | Gate | Current Evidence |
| --- | --- | --- | --- |
| MOUSEMAX evidence freeze | Partial | M7 artifact validates, Chrome/Safari URL-bar references exist, 5 pure-pixel runs pending | `scripts/validate_m9_release.sh`, `runs/real/run_1781193985/parity_review.html` generated with Chrome/Safari references; Firefox pending on this machine |
| N1 Trusted Tabs runtime | Minimal pass | `cargo run -q -p saccade-shell -- selftest-tabs` | PASS: `webviews=2 cookie_shared=true storage_shared=true input_isolated=true read_policy_enforced=true` |
| N1B Login handoff protocol | Minimal pass | `cargo run -q -p saccade-shell -- selftest-login-handoff` | PASS: `human_login=true agent_session=true password_exposed=false otp_exposed=false agent_input_to_human_tab_blocked=true` |
| N1C Full user-flow dogfood gate | Local pass + manual worker ready | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-user-flow`; `browser-session-worker` `fill_agent_fields` and `inspect_fields` probes | PASS: Human login and handoff, Agent tab inherits session, agent fills four normal fields, user sees them, sensitive values stay masked, user changes page and fills part, agent fills remaining normal fields, preserves user values, and checks sensitive status without raw values. Worker now supports real user input, constrained agent fill, and explicit non-sensitive field inspection in the same visible tab |
| N2 DEVMAX local self-test | Gauntlet corpus minimum + Servo truth pass | `cargo run -q -p devmax -- selftest-fixtures`; `cargo run -q -p devmax -- selftest-servo-fixtures` | PASS: static `total=20 detected=20 false_positives=0`; Servo `total=8 detected=8 false_positives=0` |
| N3 MCP skeleton | Local pass + live browser worker + current-tab grant | `cargo run -q -p saccade-mcp -- selftest`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-browser-session` | Tool registry exists for 21 tools; all 21 have v0 handlers; stdio JSON-RPC handles initialize/tools/list/tools/call; Agent-owned local tabs spawn a live `browser_session_worker_v0`; `saccade.tabs.grant_current` attaches a Human-owned local current tab after explicit grant, exposes redacted truth, allows safe non-sensitive field fill, rejects sensitive writes, redacts sensitive inspection, and blocks submit with user-confirmation required; it now accepts either direct `url`/`reason` or a dogfood browser `grant_path` artifact; dogfood grant artifacts can include a loopback `control_endpoint`; MCP verifies the same-WebView control ping and, when present, binds `web.truth`, `web.actions`, `web.fill_agent_fields`, `web.inspect_fields`, safe non-side-effect `web.act`, local FORMMAX `web.fill_form`, and named browser shell navigation (`saccade.browser.navigate`) to the same dogfood WebView instead of opening worker truth/actions; MCP also consumes a live official ServoShell bridge grant and routes advertised bridge truth/actions/fill/inspect/act/formmax/navigation capabilities; latest selftest PASS at `runs/mcp/selftest_1781636671768/report.json` with `tabs_grant_current=true`, `tabs_grant_artifact=true`, `servoshell_bridge_grant=true`, `servoshell_bridge_formmax_live=true`, `servoshell_bridge_artifacts=true`, `browser_navigate=true`, `web_fill_agent_fields=true`, `web_inspect_fields=true`, `web_act=true`, and `web_fill_form_live=true`; same-WebView fill/act smoke PASS at `runs/mcp/same_webview_fill_act_smoke_1781576647007.json`; same-WebView FORMMAX smoke PASS at `runs/mcp/same_webview_formmax_smoke_1781578030042.json`; worker artifacts are written under `runs/browser_session_worker/`; official ServoShell bridge control artifacts are written under `runs/mcp/servoshell_bridge_grant_1781636716084/control/`; worker truth/actions/audit redact sensitive form values while preserving field kind/status; non-sensitive pages save screenshots and sensitive pages skip screenshots; worker safe fill rejects human-owned/sensitive fields and logs no values; worker explicit inspect can return named non-sensitive values while masking sensitive values |
| N4 FORMMAX Servo input runner | Local pass + live-tab pass + current-tab dogfood pass | `cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay`; `cargo run -q -p formmax -- validate-run runs/formmax/run_1781266239027`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-formmax-live`; dogfood grant + MCP `saccade.web.fill_form` | PASS: standalone rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 receipt_verified=true; live worker rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true with replay `runs/browser_session_worker/worker_1781367973334_69584/replay.jsonl`; same visible dogfood WebView current-tab FORMMAX has `runtime=saccade-dogfood-control-v0`, rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, validation_errors=0, replay_events=2711 in `runs/mcp/same_webview_formmax_smoke_1781578030042.json`; replay/result payloads do not echo table values or sensitive values; worker run IDs now include process IDs to avoid concurrent artifact collisions |
| N4A Servo native input/dropdown probe | Local pass + demo artifact | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input-demo` | Latest PASS: focused=true, value_len=9, keydown=9, input=9, keyup=9, dispatch_failed=0; select_value=gamma, select_input=1, select_change=1, select_controls=1; demo review at `runs/native_input_demo/demo_1781386930568/review.html`; Servo does not emit beforeinput on text input path |
| N4B Human-in-loop focused typing | Local pass + contenteditable fallback | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-focused-type` | PASS: current focused non-sensitive textarea receives agent text through Servo keyboard events; current focused contenteditable editor uses the safe insert fallback; replay records only lengths/field metadata and does not log typed text; focused password field is blocked by sensitive policy |
| N4C Worker viewport alignment | Runtime resize pass | `RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker --url https://example.com/ --width 1280 --height 800` + `ping`/`truth` + macOS resize | PASS: startup uses logical/CSS window size with real HiDPI; manual resize updates render geometry and JS/layout viewport (`1280x759` -> `1360x759` -> `1000x668` on `https://example.com/`); worker and dogfood no longer pre-resize the rendering context before `WebView::resize` |
| N5 Safety truth v1 | Local pass | `cargo run -q -p saccade-shell -- selftest-safety` | PASS: agent sees agent-filled values; human can see all; agent truth masks sensitive values while preserving completed/requires-user status |
| N6 Chrome adapter v0 | Minimal pass + parity evidence | `scripts/selftest_chrome_reference.sh`; `scripts/selftest_visual_parity.sh`; `scripts/build_demo_comparison_pack.py --fixtures all --native-browsers chrome safari firefox --timeout-sec 60`; `cargo run -q -p devmax -- audit --engine chrome --url file://... --replay` | Chrome CDP reference capture writes screenshot, redacted truth/action map, network summary, and manifest; default balanced block policy handles common ad/analytics hosts; DEVMAX and MCP expose `engine=chrome`; Chrome-vs-Saccade visual parity runner covers seven local edge-case pages, emits HTML diff reports, verifies enabled non-sensitive Saccade action points against Chrome hit-tests, and can build a public demo comparison pack; latest seven-fixture public pack captured Chrome/Safari native UI, embedded Saccade worker screenshots, and verified Chrome hit-test 35/35 with four blocked modal actions skipped; Firefox capture is supported but unavailable on this machine because Firefox is not installed |
| Browser productization | Official ServoShell adapter gates expanding | `docs/browser_productization_plan.md`; `docs/browser_compat_ledger.md`; `docs/webgl_runtime_probe_report.md`; `docs/servoshell_source_strategy.md`; `docs/servoshell_adapter_migration_plan.md`; `docs/servoshell_adapter_product_gate.md`; `scripts/probe_servoshell_webdriver.py`; `scripts/probe_canvas_screenshot_paths.py`; `scripts/probe_reflex_readback_canvas.js` | Current embedded Saccade browser path is based on crates.io `servo=0.2.0`, while downloaded official Servo.app is ServoShell `0.3.0` and can run the local game. Official ServoShell adapter now passes smoke, safety redaction, screenshot policy, FORMMAX, focused typing, native input/dropdown, same-session login handoff, local-game reachability, first live bridge grant/control smoke, MCP live bridge attach for truth/actions/fill/inspect/safe-act/FORMMAX/shell navigation with capability-aware routing, bridge control report/replay artifacts with Copilot grant state in `shell_status`, and the same MCP bridge gate against source-release ServoShell `0.3.0-805e6a423`. AI-004 now has a source-fork trusted chrome badge pass: `servo-saccade-upstream` draws Human/Copilot/blocked/error state in egui browser chrome from `SACCADE_COPILOT_STATUS_PATH`, and `saccade-servoshell` writes/passes that status file when launching ServoShell. AI-001 now has a source-fork Stop v0: toolbar Stop executes active-WebView `window.stop()`, which reaches Servo's `AbortLoadUrl` path. AI-008A narrowed Canvas2D red reductions to the manual diagnostic readback path: `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/report.json` has `manual_blocked=1`, `take_blocked=0`, `route=manual_readback_only`. AI-008B now defaults local Canvas diagnostics to `take-local`: `runs/webgl_runtime/canvas_reductions_1781806451861/report.json` is green, while `--saccade-screenshot-mode manual` keeps the readback gate red in `runs/webgl_runtime/canvas_reductions_1781806531266/report.json`. AI-008C proves source ServoShell reflex `read_to_image()` sees the focused Canvas2D foreground gate: positive `runs/webgl_runtime/reflex_readback_canvas_1781806982624/report.json`, negative control `runs/webgl_runtime/reflex_readback_canvas_1781807000176/report.json`. Evidence: installed app `runs/mcp/selftest_1781636671768/report.json` and `runs/mcp/servoshell_bridge_grant_1781636716084/control/replay.jsonl`; source-release `runs/mcp/selftest_1781636405474/report.json`, `runs/mcp/servoshell_bridge_grant_1781636453696/control/replay.jsonl`, `runs/ai004_badge/bridge_smoke/report.json`, `runs/webgl_runtime/canvas_screenshot_paths_1781805458432/report.json`, `runs/webgl_runtime/canvas_reductions_1781806451861/report.json`, and `runs/webgl_runtime/reflex_readback_canvas_1781806982624/report.json`. Forking official ServoShell source remains fallback for in-process ms reflex and APIs WebDriver cannot safely expose. |
| N8 Current Tab Co-Pilot | Local v0 pass + MCP API pass + visible dogfood grant + same-WebView co-pilot bridge + FORMMAX + shell nav + toolbar v0 + named MCP browser nav + trusted source-fork badge | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-current-tab-copilot`; `RUST_LOG=error cargo run -q -p saccade-mcp -- selftest`; dogfood grant + `saccade-mcp serve-stdio` `grant_current`, `web.fill_agent_fields`, `web.inspect_fields`, `web.actions`, `web.act`, `web.fill_form`, `saccade.browser.navigate`; dogfood control `shell_status/navigate/reload/back/forward`; source-fork badge tests | PASS: shell gate has `selected_tab_seen=true`, `grant_required=true`, `redacted_truth=true`, `agent_explains_page=true`, `non_sensitive_filled=true`, `sensitive_write_blocked=true`, `sensitive_values_exposed=false`, `confirmation_required=true`; report `runs/current_tab_copilot/copilot_1781535424558/report.json`; replay `runs/browser_session_worker/worker_1781535424701_32946/replay.jsonl`. MCP gate has `tabs_grant_current=true`, `tabs_grant_artifact=true`, `servoshell_bridge_grant=true`, `servoshell_bridge_formmax_live=true`, `servoshell_bridge_artifacts=true`, `browser_navigate=true`, `web_fill_agent_fields=true`, `web_inspect_fields=true`, `web_act=true`, and `web_fill_form_live=true` in `runs/mcp/selftest_1781636671768/report.json`; source-release gate repeats those booleans in `runs/mcp/selftest_1781636405474/report.json`. Live ServoShell bridge evidence includes `runs/mcp/servoshell_bridge_grant_1781636716084/control/replay.jsonl`, `runs/mcp/servoshell_bridge_grant_1781636453696/control/replay.jsonl`, and `runs/ai004_badge/bridge_smoke/report.json`. Dogfood browser exposes `Cmd+Shift+G`, writes `runs/current_tab_grants/latest.json`, and MCP can consume that artifact through `saccade.tabs.grant_current({grant_path})`. The source-fork ServoShell now draws the trusted Copilot badge in browser chrome from `SACCADE_COPILOT_STATUS_PATH`; page overlays/stylesheets/title mutation remain forbidden. |
| N7 Public release package | Pending | README/site/video/report package | Launch docs and parity requirements exist; video/site not done |
| Comparison benchmark | Smoke pass for Codex, Claude auth blocked | `python3 scripts/agent_compare.py run --agent both --tasks all --execute` | Codex-vs-Claude task suite, structured result schema, runner, parser, and SVG report generator exist under `eval/agent_compare/` and `scripts/agent_compare.py`; smoke run `runs/agent_compare/run_1781365508552` shows Codex passed `trusted_tabs_runtime` and `safety_truth_redaction`; Claude Code returned 403 subscription/API access blocked before tasks |

## What We Have Not Missed

The big buckets from v5 are all represented:

- MOUSEMAX freeze and parity evidence.
- Trusted Tabs.
- Login handoff.
- DEVMAX.
- MCP.
- FORMMAX runner.
- Native keyboard input probe.
- Safety policy UI.
- Chrome adapter.
- Playwright alternative comparison and Codex-vs-Claude agent comparison.
- Public release package.

The new gauntlet file is now the product scoreboard:

- Canonical copy: `docs/SACCADE_EVALUATION_GAUNTLET_v1.md`
- Execution plan: `docs/evaluation_gauntlet_execution_plan.md`
- Eval entry point: `eval/README.md`

## Immediate Queue

Do these in order:

1. N8 Current Tab Co-Pilot productization: local shell, MCP API, visible dogfood
   grant shortcut, MCP grant-artifact import, and same-WebView truth/actions/
   fill/inspect/safe-act pass.
2. Browser productization P1: add toolbar URL text editing and non-obscuring
   chrome. Title-bar URL/title/loading/nav state, `Cmd+L` keyboard URL entry,
   page-click focus recovery, mouse Back/Forward navigation, native toolbar
   hit-zones, named MCP shell navigation, and pinned-API Stop proof are done.
3. Browser compatibility P1: Wayne logs in to GitHub/Gist inside Saccade using
   `runs/dogfood_profile/default`, then rerun `inspect-editors` on
   `https://gist.github.com/new` using the local BP-004 reduction as the oracle.
4. Browser compatibility P1: keep broad canvas/WebGL-heavy judgement routed to
   Chrome/reference or Servo `WebView::take_screenshot()` diagnostics unless a
   site/game has evidence. AI-008C proves the source ServoShell reflex
   `read_to_image()` path sees the focused Canvas2D foreground gate; optional
   AI-008D expands that gate to the live local game when the game server is
   running.
5. Finish DEVMAX gauntlet evidence polish: screenshot crop per finding,
   multi-action click verification, live-worker/Chrome finding parity, and HTTP
   status awareness for resource loads.
6. Harden browser-backed MCP sessions: shared multi-tab process. Worker
   report/replay, live audit, screenshot policy, sensitive-value redaction,
   manual input forwarding, constrained agent fill, explicit non-sensitive
   inspect, live FORMMAX fill, and MCP wrappers are in place.
7. Add replay metadata for masked status and user action boundaries without
   sensitive values.
8. Finish MOUSEMAX parity references for `runs/real/run_1781193985`: Chrome and
   Safari URL-bar screenshots are complete; next add Firefox URL-bar screenshot
   and optional Chrome result screenshot.

## Parking Lot

Do not start these until DEVMAX gauntlet bar, FORMMAX runner, and safety selftest are green:

- real third-party website automation,
- Playwright comparison benchmark,
- public release push.
Chrome adapter work is allowed as a comparison/runtime gate, but not as a replacement for the Servo evidence layer.
