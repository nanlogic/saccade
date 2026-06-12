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
SAFETY PASS human_login=true agent_session=true human_can_see_agent_values=true agent_can_see_agent_values=true ssn_exposed=false government_id_exposed=false credit_card_exposed=false user_password_exposed=false masked_sensitive_fields=4
```

## Rule

The user can see everything in the browser, including values filled by the agent.

The agent receives mediated truth:

- It can see values it filled.
- It can use the inherited session after explicit login handoff.
- It cannot read human-owned sensitive values such as SSN, government ID, credit card, or password.
- It cannot type into the Human-owned tab.
- It cannot request full truth from Human-owned tabs without a read grant.

## Scope

This is a deterministic local fixture and policy gate. It proves the masking rule at the Saccade browser boundary, not the final product UI.

The fixture lives at:

`test_pages/login_handoff/safety.html`

## Still Needed

- Visible UI markers for Human and Agent tabs.
- Replay events for masked fields and user confirmations.
- A product confirmation dialog for sensitive writes.
- Chrome adapter parity for UI review and public demos.
