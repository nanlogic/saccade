# Saccade Safety Truth Profile

Date: 2026-06-12

## Result

Safety truth v1 is now a local gate.

Command:

```bash
cargo run -q -p saccade-shell -- selftest-safety
```

Observed output:

```text
SAFETY PASS human_login=true agent_session=true human_can_see_agent_values=true agent_can_see_agent_values=true ssn_exposed=false government_id_exposed=false credit_card_exposed=false user_password_exposed=false masked_sensitive_fields=5 completed_without_value=4 requires_user_input=1 status_known=true
```

## Rule

The user sees the real browser. Values remain visible to the user, including values filled by the agent.

The agent receives redacted actionable truth:

- It can see values it filled.
- It can use the inherited session after explicit login handoff.
- It can see that sensitive fields exist.
- It can see whether a sensitive field is empty or completed.
- It cannot read human-owned sensitive values such as SSN, government ID, credit card, or password.
- It cannot type into the Human-owned tab.
- It cannot request full truth from Human-owned tabs without a read grant.

Sensitive field contract:

```json
{
  "id": "ssn",
  "label": "SSN",
  "owner": "human",
  "sensitivity": "ssn",
  "value": null,
  "masked": true,
  "value_state": "completed_without_value",
  "user_action_required": false
}
```

For an empty sensitive field:

```json
{
  "id": "tax-id-empty",
  "label": "Tax ID",
  "owner": "human",
  "sensitivity": "tax_id",
  "value": null,
  "masked": true,
  "value_state": "requires_user_input",
  "user_action_required": true
}
```

## UX Principle

Do not make safety feel like a wall of confirmation dialogs.

The preferred flow is:

1. Agent fills all non-sensitive fields quickly.
2. User sees the page already filled.
3. Agent reports the small set of sensitive fields that require the user.
4. User fills sensitive fields in the real browser or clicks submit.
5. Agent sees status changes, not sensitive values.

Security should behave like a guardrail, not a toll booth.

The full local product flow now has a separate gate:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-user-flow
```

That gate proves login handoff, agent fill, user review, user page change, user partial fill, agent continuation, and masked sensitive-status checking in one run.

## Scope

This is a deterministic local fixture and policy gate. It proves the masking rule at the Saccade browser boundary, not the final product UI.

The local mediator may inspect browser state to classify and mask sensitive fields. Raw sensitive values must not be shown to the LLM, written to replay, or printed to logs.

The fixture lives at:

`test_pages/login_handoff/safety.html`

Browser Fact Stream v0 now applies the same rule to live page facts:

```text
docs/browser_fact_stream.md
scripts/lib/browser_fact_stream.js
```

It reports sensitive field presence and value status while redacting raw values.

## Still Needed

- Visible UI markers for Human and Agent tabs.
- Product UI that shows which sensitive fields need the user without creating confirmation fatigue.
- Replay events that record masked status and user action boundaries without sensitive values.
- Chrome adapter parity for UI review and public demos.
