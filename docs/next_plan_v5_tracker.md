# Saccade Next Plan v5 Tracker

Date: 2026-06-11
Updated: 2026-06-12

## Note

`SACCADE_NEXT_PLAN_v5.md` has a numbering conflict:

- Section 2 says `N2 = DEVMAX`.
- Section 4 says `N2 = Login Handoff`.
- Decision Summary says `N2 = DEVMAX`.

Use this tracker as the normalized execution map.

## Normalized Order

| Track | Status | Gate | Current Evidence |
| --- | --- | --- | --- |
| MOUSEMAX evidence freeze | Partial | M7 artifact validates, parity pack exists, 5 pure-pixel runs pending | `scripts/validate_m9_release.sh`, `parity_review.html` generated |
| N1 Trusted Tabs runtime | Minimal pass | `cargo run -q -p saccade-shell -- selftest-tabs` | PASS: `webviews=2 cookie_shared=true storage_shared=true input_isolated=true read_policy_enforced=true` |
| N1B Login handoff protocol | Minimal pass | `cargo run -q -p saccade-shell -- selftest-login-handoff` | PASS: `human_login=true agent_session=true password_exposed=false otp_exposed=false agent_input_to_human_tab_blocked=true` |
| N2 DEVMAX local self-test | Gauntlet corpus minimum + Servo truth pass | `cargo run -q -p devmax -- selftest-fixtures`; `cargo run -q -p devmax -- selftest-servo-fixtures` | PASS: static `total=20 detected=20 false_positives=0`; Servo `total=8 detected=8 false_positives=0` |
| N3 MCP skeleton | Minimal pass | `cargo run -q -p saccade-mcp -- selftest`; `cargo run -q -p saccade-mcp -- serve-stdio` | Tool registry exists for 17 tools; stdio JSON-RPC handles initialize/tools/list/tools/call; implemented `open_local`, `audit_page`, and `tabs.list`; selftest routes `audit_page` to DEVMAX and verifies tab scoping plus sensitive policy gate |
| N4 FORMMAX Servo input runner | Local pass | `cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay`; `cargo run -q -p formmax -- validate-run runs/formmax/run_1781266239027` | PASS: rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 receipt_verified=true; replay has 2712 events, screenshots=2, and no table-value echo; artifact validator passes |
| N4A Servo native input/dropdown probe | Local pass | `RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input` | PASS x3: focused=true, value_len=9, keydown=9, input=9, keyup=9, dispatch_failed=0; select_value=gamma, select_input=1, select_change=1; Servo does not emit beforeinput on text input path |
| N5 Safety truth v1 | Local pass | `cargo run -q -p saccade-shell -- selftest-safety` | PASS: agent sees agent-filled values; human can see all; agent truth masks sensitive values while preserving completed/requires-user status |
| N6 Chrome adapter v0 | Started | `scripts/capture_chrome_reference.sh <url> <output-dir>`; later `cargo run -q -p devmax -- audit --engine chrome --url ... --replay` | Chrome reference capture script exists; CDP action/truth adapter not started |
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

1. Move Chrome/Firefox visual parity earlier: Chrome reference capture now exists; next is Chrome adapter v0 or visual parity layer for UI review credibility.
2. Harden FORMMAX runner v1: expand native input-event typing to more controls and add a comparison baseline.
3. Finish MOUSEMAX parity references for `runs/real/run_1781193985`: add Chrome and Safari URL-bar screenshots, then regenerate `parity_review.html`.
4. Finish DEVMAX gauntlet evidence polish: screenshot crop per finding, multi-action click verification, and HTTP status awareness for resource loads.
5. Add persistent tab state to `saccade-mcp` and implement `saccade.web.truth/actions/act` on top of Saccade browser truth.
6. Add replay metadata for masked status and user action boundaries without sensitive values.

## Parking Lot

Do not start these until DEVMAX gauntlet bar, FORMMAX runner, and safety selftest are green:

- real third-party website automation,
- Playwright comparison benchmark,
- public release push.

Chrome adapter work is allowed as a comparison/runtime gate, but not as a replacement for the Servo evidence layer.
