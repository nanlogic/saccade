# Saccade Next Plan v5 Tracker

Date: 2026-06-11

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
| N2 DEVMAX local self-test | Minimal pass + Servo pixel pass | `cargo run -q -p devmax -- selftest-fixtures`; `cargo run -q -p devmax -- selftest-servo-fixtures` | PASS: static `total=16 detected=16 false_positives=0`; Servo `total=5 detected=5 false_positives=0` |
| N3 MCP skeleton | Pending | `cargo run -q -p saccade-mcp -- selftest` | Not started |
| N4 FORMMAX Servo input runner | Pending | `cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay` | Fixture and smoke oracle exist; browser runner not built |
| N5 Safety truth v1 | Pending | `cargo run -q -p saccade-shell -- selftest-safety` | Policy docs exist; live UI not built |
| N6 Chrome adapter v0 | Pending | `cargo run -q -p devmax -- audit --engine chrome --url ... --replay` | Not started |
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
- Safety policy UI.
- Chrome adapter.
- Playwright alternative comparison.
- Public release package.

## Immediate Queue

Do these in order:

1. Finish MOUSEMAX parity screenshots: add Chrome and Safari URL-bar references.
2. Expand DEVMAX Servo truth to console/network capture and click verification.
3. Add MCP skeleton after DEVMAX has one useful report shape.
4. Build FORMMAX Servo input runner.
5. Add replay metadata for Trusted Tabs and Login Handoff actions.

## Parking Lot

Do not start these until N1B through N4 are green:

- real third-party website automation,
- Chrome adapter as primary runtime,
- Playwright comparison benchmark,
- public release push.
