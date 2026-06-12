# Saccade FORMMAX Runner Report

Date: 2026-06-12

## Result

FORMMAX now has a local browser runner:

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

It opens the fixture in Servo, native-types one real text field, scrolls the lazy table, fills all non-sensitive fields, blocks sensitive fields, submits the local fixture, verifies the receipt, and writes replay artifacts.

Observed three consecutive passes:

```text
FORMMAX RUNNER PASS rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 receipt_verified=true
```

## Evidence

Current evidence run artifacts:

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781266239027/result.json`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781266239027/replay.jsonl`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781266239027/before.png`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781266239027/after.png`

Replay summary from the first run:

```text
events=2712
field_discovered=675
field_focused=672
field_filled=672
field_verified=672
scroll_checkpoint=6
confirmation_required=3
field_blocked_sensitive=3
native_input_verified=1
receipt_seen=1
```

Receipt validation passed with 96 rows and 0 validation errors.

Native input evidence from the same run:

```text
row_id=CAP-001 field=site_name value_matches=true keydown=19 input=19 keyup=19 dispatch_failed=0
```

Sensitive fields were discovered but not filled:

```text
tax_id: requires_user_input, value_present=false
signature: requires_user_input, value_present=false
legal_attestation: requires_user_input, value_present=false
```

Replay does not echo table values. A local leak check over 288 deterministic text/date/owner values found `replay_value_leaks=0`.

Artifact validation command:

```bash
cargo run -q -p formmax -- validate-run runs/formmax/run_1781266239027
```

Observed result:

```text
FORMMAX VALIDATION PASS run=runs/formmax/run_1781266239027 rows=96 pages=2 filled=672 native_typed=1 blocked_sensitive=3 events=2712 screenshots=2 replay_value_leaks=0
```

## Current Limit

The runner now proves native keyboard text entry for one real FORMMAX text field, then uses the trusted fixture DOM transaction path for the remaining full-table fill. It proves rendered-page transaction behavior, scroll/page coverage, receipt validation, screenshot artifacts, replay shape, sensitive-field policy, and a small native input bridge.

Next hardening: expand native input coverage to more control types, including number/date/select/checkbox, and add Chrome/Playwright comparison baselines.
