# Saccade FORMMAX Runner Report

Date: 2026-06-12

## Result

FORMMAX now has a local browser runner:

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

It opens the fixture in Servo, scrolls the lazy table, fills non-sensitive fields, blocks sensitive fields, submits the local fixture, verifies the receipt, and writes replay artifacts.

Observed three consecutive passes:

```text
FORMMAX RUNNER PASS rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true
```

## Evidence

Current evidence run artifacts:

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781234358800/result.json`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781234358800/replay.jsonl`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781234358800/before.png`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781234358800/after.png`

Replay summary from the first run:

```text
events=2711
field_discovered=675
field_focused=672
field_filled=672
field_verified=672
scroll_checkpoint=6
confirmation_required=3
field_blocked_sensitive=3
receipt_seen=1
```

Receipt validation passed with 96 rows and 0 validation errors.

Sensitive fields were discovered but not filled:

```text
tax_id: requires_user_input, value_present=false
signature: requires_user_input, value_present=false
legal_attestation: requires_user_input, value_present=false
```

Replay does not echo table values. A local leak check over 288 deterministic text/date/owner values found `replay_value_leaks=0`.

Artifact validation command:

```bash
cargo run -q -p formmax -- validate-run runs/formmax/run_1781234358800
```

Observed result:

```text
FORMMAX VALIDATION PASS run=runs/formmax/run_1781234358800 rows=96 pages=2 filled=672 blocked_sensitive=3 events=2711 screenshots=2 replay_value_leaks=0
```

## Current Limit

The v0 runner drives trusted fixture DOM controls from the Servo page context. It proves rendered-page transaction behavior, scroll/page coverage, receipt validation, screenshot artifacts, replay shape, and sensitive-field policy. It is not yet native keyboard text entry.

Next hardening: native input-event typing where Servo supports it, and Chrome/Playwright comparison baselines.
