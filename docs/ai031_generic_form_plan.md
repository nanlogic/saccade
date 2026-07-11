# AI-031 Generic Form Inventory and Plan

Date: 2026-07-11
Status: first slice passed

## Result

The official ServoShell bridge now exposes two value-free, non-writing control
methods:

- `form_inventory` discovers ordinary and sensitive fields on the current page;
- `form_compile_plan` validates proposed assignments against a fixed page
  revision and returns eligible/rejected field IDs without assignment values.

The inventory reports stable field ID, selector hash, type, label and confidence,
owner, sensitivity class, required/visible/enabled/readonly state, option count,
redacted completion state, eligibility, and block reasons. It does not return raw
field values or select option contents.

The plan rejects sensitive fields, human-owned fields, existing values, hidden or
disabled controls, unsupported types, ambiguous labels, unstable identities,
unknown field IDs, and stale page revisions. It performs no writes and cannot
submit the form.

## Adversarial gate

Fixture: `test_pages/form_plan/index.html`

The fixture contains 17 controls, including ordinary text/select/number/date/
checkbox/contenteditable fields, existing user values, human ownership, SSN,
password, hidden and disabled controls, a file input, duplicate names, an
ambiguous label, and a display-hidden field.

Command:

```text
python3 scripts/probe_generic_form_plan.py \
  --output-dir runs/formmax/generic_plan_ai031_20260711_final
```

Observed:

```text
GENERIC FORM PLAN PASS fields=17 eligible=6 rejected=12
```

The 12 rejections include the 11 ineligible fixture fields plus an explicit
unknown field request. The probe also proves stale revision rejection, explicit
safe-policy enforcement, scalar-only assignments, and scans the full output tree
for three assignment sentinels. No sentinel appeared.

Artifact:

- `runs/formmax/generic_plan_ai031_20260711_final/report.json`
- `runs/formmax/generic_plan_ai031_20260711_final/bridge/control/replay.jsonl`

## Regression

The existing high-volume path remains green:

```text
SACCADE_SERVOSHELL_FORMMAX PASS rows=96 pages=2 filled=672
blocked_sensitive=3 receipt_verified=true
```

Artifact:

- `runs/servoshell_adapter/formmax_ai031_regression_20260711/result.json`
- `runs/servoshell_adapter/formmax_ai031_regression_20260711/replay.jsonl`

## Next slice

Add a bounded executor that consumes the compiled plan on the same page revision,
writes only eligible controls, verifies each postcondition, preserves user values,
and returns a repair plan for failures. Then expose inventory, plan, execute, and
verify through the engine-neutral MCP surface.

This checkpoint does not claim generic third-party form filling yet.
