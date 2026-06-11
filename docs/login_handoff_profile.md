# Saccade N1B Login Handoff Profile

Date: 2026-06-11

## Result

N1B login handoff selftest passed on macOS arm64 with pinned Servo `0.2.0`.

Command:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-login-handoff
```

Observed output:

```text
LOGIN_HANDOFF PASS human_login=true agent_session=true password_exposed=false otp_exposed=false agent_input_to_human_tab_blocked=true
```

## What The Selftest Covers

The local fixture lives under:

`/Users/waynema/Documents/GitHub/SACCADE/test_pages/login_handoff/`

The shell creates two WebViews in one Servo instance:

- Human tab opens `login.html`.
- The shell simulates the human side entering username, password, and one-time code.
- The Human tab submits the form and lands on `dashboard.html`.
- The Human tab clicks the explicit `Done` handoff control.
- Agent tab opens `dashboard.html` on the same origin and verifies the inherited session.

## Gate

The gate passes only when:

- Human login reaches the logged-in dashboard.
- Agent tab sees the logged-in session.
- Password value is not exposed to the Agent-visible probe.
- OTP value is not exposed to the Agent-visible probe.
- Agent input to the Human-owned tab remains blocked by `TabInfo`.
- The explicit Done handoff was observed.

## Current Scope

This is a deterministic local fixture. It proves the core product rule:

```text
Human completes sensitive login, then explicitly hands off session state.
Agent can continue with the session, but cannot read or type into the Human tab.
```

Still pending for product UI:

- Visible tab strip and handoff affordance in the real shell UI.
- Replay events that include `tab_id`, `owner`, `actor`, and `page_revision`.
- Browser truth masking for arbitrary third-party sensitive fields.
