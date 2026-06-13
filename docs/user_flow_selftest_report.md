# Saccade User Flow Selftest Report

Date: 2026-06-13

## Result

The first full user-flow gate now passes.

Command:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-user-flow
```

Observed output:

```text
USER_FLOW PASS human_login=true handoff_done=true agent_session=true round1_agent_filled=4 user_can_see_agent_values=true round1_requires_user_input=3 user_page_change_seen=true user_normal_checked=true sensitive_status_checked_without_value=true agent_completed_remaining=2 preserved_user_values=true same_agent_tab_continued=true final_sensitive_completed_without_value=4 sensitive_values_exposed=false
```

## What It Covers

This gate stitches together the user flow that was previously split across smaller tests:

1. Human logs in in a Human-owned tab.
2. Human clicks explicit handoff.
3. Agent opens the same-origin workflow in an Agent-owned tab with the inherited session.
4. Agent fills four normal fields.
5. User can see the agent-filled values in the page.
6. Agent sees sensitive fields and their status, but not raw sensitive values.
7. User changes to the next page and fills part of the form.
8. Agent continues in the same tab, fills the remaining normal fields, preserves the user's values, and validates the non-sensitive user value.
9. Agent checks sensitive-field completion and format status without receiving the raw sensitive values.

## Fixture

The local fixture is:

```text
test_pages/login_handoff/user_flow.html
```

It reuses the login handoff fixture so the session and `Done` handoff path stay the same as `selftest-login-handoff`.

## Safety Contract

The user sees all values in the browser.

The agent receives mediated truth:

- agent-filled normal values are visible to the agent,
- user-filled non-sensitive values can be checked,
- sensitive values are masked,
- sensitive fields expose completion and format status only,
- agent input to the Human-owned login tab stays blocked.

## Manual Dogfood Readiness

The browser worker can now keep the same visible Servo tab alive while accepting real user input and constrained agent fill requests. A manual session can reproduce Wayne's flow:

1. Wayne inspects or edits the page directly in the worker window.
2. Agent fills only agent-owned, non-sensitive fields through `fill_agent_fields`.
3. Sensitive or human-owned fields are rejected with metadata only.
4. Wayne can navigate to the next page and fill part of the form.
5. Agent can continue in the same tab, preserve Wayne's values, and check sensitive completion status without raw sensitive values.

The first worker-level probe filled `task-1` and `task-2`, rejected `ssn` and `tax-id-empty`, skipped screenshots because sensitive fields were present, and logged `values_logged=false`.

## Remaining Product Work

- Manual Wayne-in-the-loop dogfood session.
- Product UI for Human/Agent tab badges and handoff.
- Replay events with `tab_id`, `owner`, `actor`, `basis_revision`, and masked user-action boundaries.
- Live FORMMAX integration into the browser-backed Agent tab instead of a separate runner.
