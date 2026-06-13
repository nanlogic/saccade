# Saccade Next Plan v5 Tracker

Date: 2026-06-11
Updated: 2026-06-13

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
| N3 MCP skeleton | Local pass + live browser worker | `cargo run -q -p saccade-mcp -- selftest`; `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-browser-session` | Tool registry exists for 19 tools; all 19 have v0 handlers; stdio JSON-RPC handles initialize/tools/list/tools/call; Agent-owned local tabs spawn a live `browser_session_worker_v0`; selftest routes live audit/truth/actions/act/fill_agent_fields/inspect_fields through that worker and routes static audit/click/form/report tools through DEVMAX/Servo/FORMMAX evidence while verifying tab scoping plus sensitive policy gate; generated artifacts are indexed at `runs/mcp/artifacts.jsonl`; worker artifacts are written under `runs/browser_session_worker/`; worker truth/actions/audit redact sensitive form values while preserving field kind/status; non-sensitive pages save screenshots and sensitive pages skip screenshots; worker safe fill rejects human-owned/sensitive fields and logs no values; worker explicit inspect can return named non-sensitive values while masking sensitive values |
| N4 FORMMAX Servo input runner | Local pass | `cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay`; `cargo run -q -p formmax -- validate-run runs/formmax/run_1781266239027` | PASS: rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 receipt_verified=true; replay has 2712 events, screenshots=2, and no table-value echo; artifact validator passes |
| N4A Servo native input/dropdown probe | Local pass | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input` | PASS x3: focused=true, value_len=9, keydown=9, input=9, keyup=9, dispatch_failed=0; select_value=gamma, select_input=1, select_change=1; Servo does not emit beforeinput on text input path |
| N5 Safety truth v1 | Local pass | `cargo run -q -p saccade-shell -- selftest-safety` | PASS: agent sees agent-filled values; human can see all; agent truth masks sensitive values while preserving completed/requires-user status |
| N6 Chrome adapter v0 | Minimal pass + parity evidence | `scripts/selftest_chrome_reference.sh`; `scripts/selftest_visual_parity.sh`; `scripts/build_demo_comparison_pack.py --fixtures all --native-browsers chrome safari firefox --timeout-sec 60`; `cargo run -q -p devmax -- audit --engine chrome --url file://... --replay` | Chrome CDP reference capture writes screenshot, redacted truth/action map, network summary, and manifest; default balanced block policy handles common ad/analytics hosts; DEVMAX and MCP expose `engine=chrome`; Chrome-vs-Saccade visual parity runner covers seven local edge-case pages, emits HTML diff reports, verifies enabled non-sensitive Saccade action points against Chrome hit-tests, and can build a public demo comparison pack; latest seven-fixture public pack captured Chrome/Safari native UI, embedded Saccade worker screenshots, and verified Chrome hit-test 35/35 with four blocked modal actions skipped; Firefox capture is supported but unavailable on this machine because Firefox is not installed |
| N7 Public release package | Pending | README/site/video/report package | Launch docs and parity requirements exist; video/site not done |
| Comparison benchmark | Pending | `devmax compare` and `formmax compare` | Not started |

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
- Playwright alternative comparison.
- Public release package.

The new gauntlet file is now the product scoreboard:

- Canonical copy: `docs/SACCADE_EVALUATION_GAUNTLET_v1.md`
- Execution plan: `docs/evaluation_gauntlet_execution_plan.md`
- Eval entry point: `eval/README.md`

## Immediate Queue

Do these in order:

1. Move Chrome/Firefox visual parity earlier: Chrome CDP capture, Chrome-vs-Saccade compare, Chrome-side click verification, native Chrome/Safari UI capture, and a seven-fixture demo comparison pack exist; next is Firefox capture on a machine with Firefox installed.
2. Harden FORMMAX runner v1: expand native input-event typing to more controls and add a comparison baseline.
3. Finish MOUSEMAX parity references for `runs/real/run_1781193985`: Chrome and Safari URL-bar screenshots are complete; next add Firefox URL-bar screenshot and optional Chrome result screenshot.
4. Finish DEVMAX gauntlet evidence polish: screenshot crop per finding, multi-action click verification, live-worker/Chrome finding parity, and HTTP status awareness for resource loads.
5. Harden browser-backed MCP sessions: shared multi-tab process and FORMMAX integration with the live tab. Worker report/replay, live audit, screenshot policy, sensitive-value redaction, manual input forwarding, constrained agent fill, explicit non-sensitive inspect, and MCP wrappers for fill/inspect are in place.
6. Add replay metadata for masked status and user action boundaries without sensitive values.

## Parking Lot

Do not start these until DEVMAX gauntlet bar, FORMMAX runner, and safety selftest are green:

- real third-party website automation,
- Playwright comparison benchmark,
- public release push.

Chrome adapter work is allowed as a comparison/runtime gate, but not as a replacement for the Servo evidence layer.
