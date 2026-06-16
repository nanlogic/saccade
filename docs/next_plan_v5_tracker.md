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
| N3 MCP skeleton | Local pass + live browser worker + current-tab grant | `cargo run -q -p saccade-mcp -- selftest`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-browser-session` | Tool registry exists for 20 tools; all 20 have v0 handlers; stdio JSON-RPC handles initialize/tools/list/tools/call; Agent-owned local tabs spawn a live `browser_session_worker_v0`; `saccade.tabs.grant_current` attaches a Human-owned local current tab after explicit grant, exposes redacted truth, allows safe non-sensitive field fill, rejects sensitive writes, redacts sensitive inspection, and blocks submit with user-confirmation required; it now accepts either direct `url`/`reason` or a dogfood browser `grant_path` artifact; dogfood grant artifacts can include a loopback `control_endpoint`; MCP verifies the same-WebView control ping and, when present, binds `web.truth`, `web.actions`, `web.fill_agent_fields`, `web.inspect_fields`, safe non-side-effect `web.act`, and local FORMMAX `web.fill_form` to the same dogfood WebView instead of opening worker truth/actions; latest selftest PASS at `runs/mcp/selftest_1781578578728/report.json` with `tabs_grant_current=true` and `tabs_grant_artifact=true`; same-WebView fill/act smoke PASS at `runs/mcp/same_webview_fill_act_smoke_1781576647007.json`; same-WebView FORMMAX smoke PASS at `runs/mcp/same_webview_formmax_smoke_1781578030042.json`; worker artifacts are written under `runs/browser_session_worker/`; worker truth/actions/audit redact sensitive form values while preserving field kind/status; non-sensitive pages save screenshots and sensitive pages skip screenshots; worker safe fill rejects human-owned/sensitive fields and logs no values; worker explicit inspect can return named non-sensitive values while masking sensitive values |
| N4 FORMMAX Servo input runner | Local pass + live-tab pass + current-tab dogfood pass | `cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay`; `cargo run -q -p formmax -- validate-run runs/formmax/run_1781266239027`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-formmax-live`; dogfood grant + MCP `saccade.web.fill_form` | PASS: standalone rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 receipt_verified=true; live worker rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true with replay `runs/browser_session_worker/worker_1781367973334_69584/replay.jsonl`; same visible dogfood WebView current-tab FORMMAX has `runtime=saccade-dogfood-control-v0`, rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, validation_errors=0, replay_events=2711 in `runs/mcp/same_webview_formmax_smoke_1781578030042.json`; replay/result payloads do not echo table values or sensitive values; worker run IDs now include process IDs to avoid concurrent artifact collisions |
| N4A Servo native input/dropdown probe | Local pass + demo artifact | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input-demo` | Latest PASS: focused=true, value_len=9, keydown=9, input=9, keyup=9, dispatch_failed=0; select_value=gamma, select_input=1, select_change=1, select_controls=1; demo review at `runs/native_input_demo/demo_1781386930568/review.html`; Servo does not emit beforeinput on text input path |
| N4B Human-in-loop focused typing | Local pass + contenteditable fallback | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-focused-type` | PASS: current focused non-sensitive textarea receives agent text through Servo keyboard events; current focused contenteditable editor uses the safe insert fallback; replay records only lengths/field metadata and does not log typed text; focused password field is blocked by sensitive policy |
| N4C Worker viewport alignment | Runtime resize pass | `RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker --url https://example.com/ --width 1280 --height 800` + `ping`/`truth` + macOS resize | PASS: startup uses logical/CSS window size with real HiDPI; manual resize updates render geometry and JS/layout viewport (`1280x759` -> `1360x759` -> `1000x668` on `https://example.com/`); worker and dogfood no longer pre-resize the rendering context before `WebView::resize` |
| N5 Safety truth v1 | Local pass | `cargo run -q -p saccade-shell -- selftest-safety` | PASS: agent sees agent-filled values; human can see all; agent truth masks sensitive values while preserving completed/requires-user status |
| N6 Chrome adapter v0 | Minimal pass + parity evidence | `scripts/selftest_chrome_reference.sh`; `scripts/selftest_visual_parity.sh`; `scripts/build_demo_comparison_pack.py --fixtures all --native-browsers chrome safari firefox --timeout-sec 60`; `cargo run -q -p devmax -- audit --engine chrome --url file://... --replay` | Chrome CDP reference capture writes screenshot, redacted truth/action map, network summary, and manifest; default balanced block policy handles common ad/analytics hosts; DEVMAX and MCP expose `engine=chrome`; Chrome-vs-Saccade visual parity runner covers seven local edge-case pages, emits HTML diff reports, verifies enabled non-sensitive Saccade action points against Chrome hit-tests, and can build a public demo comparison pack; latest seven-fixture public pack captured Chrome/Safari native UI, embedded Saccade worker screenshots, and verified Chrome hit-test 35/35 with four blocked modal actions skipped; Firefox capture is supported but unavailable on this machine because Firefox is not installed |
| Browser productization | Pivoting to official ServoShell adapter | `docs/browser_productization_plan.md`; `docs/browser_compat_ledger.md`; `docs/webgl_runtime_probe_report.md`; `docs/servoshell_source_strategy.md`; `docs/servoshell_adapter_migration_plan.md`; `scripts/probe_servoshell_webdriver.py` | Current embedded Saccade browser path is based on crates.io `servo=0.2.0`, while downloaded official Servo.app is ServoShell `0.3.0` and can run the local game. Official ServoShell WebDriver probe passes: session creation, JS execution, element click, DOM-change verification, screenshot capture, and local game page reachability. Next browser-productization work moves to a Saccade external adapter over official ServoShell WebDriver; forking official ServoShell source remains fallback if WebDriver is too thin. |
| N8 Current Tab Co-Pilot | Local v0 pass + MCP API pass + visible dogfood grant + same-WebView co-pilot bridge + FORMMAX | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-current-tab-copilot`; `RUST_LOG=error cargo run -q -p saccade-mcp -- selftest`; dogfood grant + `saccade-mcp serve-stdio` `grant_current`, `web.fill_agent_fields`, `web.inspect_fields`, `web.actions`, `web.act`, `web.fill_form` | PASS: shell gate has `selected_tab_seen=true`, `grant_required=true`, `redacted_truth=true`, `agent_explains_page=true`, `non_sensitive_filled=true`, `sensitive_write_blocked=true`, `sensitive_values_exposed=false`, `confirmation_required=true`; report `runs/current_tab_copilot/copilot_1781535424558/report.json`; replay `runs/browser_session_worker/worker_1781535424701_32946/replay.jsonl`. MCP gate has `tabs_grant_current=true` and `tabs_grant_artifact=true` in `runs/mcp/selftest_1781578578728/report.json`. Dogfood browser exposes `Cmd+Shift+G`, shows `copilot=granted` in the title, and writes `runs/current_tab_grants/latest.json`; MCP can consume that artifact through `saccade.tabs.grant_current({grant_path})`. The artifact includes a loopback `control_endpoint`; MCP fill/act smoke has `fill_runtime=saccade-dogfood-control-v0`, `inspect_runtime=saccade-dogfood-control-v0`, `safe_act_runtime=saccade-dogfood-control-v0`, `filled=3`, `rejected_sensitive=2`, `values_redacted=2`, and `submit_blocked=true` in `runs/mcp/same_webview_fill_act_smoke_1781576647007.json`. Current-tab FORMMAX smoke has `fill_runtime=saccade-dogfood-control-v0`, `engine=saccade-dogfood-control-formmax-live-v0`, rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, validation_errors=0 in `runs/mcp/same_webview_formmax_smoke_1781578030042.json`. Next: browser shell basics and real editor/contenteditable dogfood. |
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
2. Browser productization P1: add clickable editable URL bar and visible
   clickable Back/Forward/Reload/Stop. Title-bar URL/title/loading/nav state,
   `Cmd+L` keyboard URL entry, page-click focus recovery, and mouse
   Back/Forward navigation are done.
3. Browser compatibility P1: Wayne logs in to GitHub/Gist inside Saccade using
   `runs/dogfood_profile/default`, then rerun `inspect-editors` on
   `https://gist.github.com/new` using the local BP-004 reduction as the oracle.
4. Browser compatibility P1: keep canvas/WebGL judgement routed to
   Chrome/reference while BP-011 is parked; resume later with Servo
   `WebView::take_screenshot()` versus manual `paint()+read_to_image()`.
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
