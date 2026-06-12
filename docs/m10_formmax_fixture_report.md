# Saccade M10 FORMMAX Fixture Report

Date: 2026-06-11

## Result

M10 adds a local FORMMAX practical fixture.

Fixture:

`/Users/waynema/Documents/GitHub/SACCADE/test_pages/formmax/index.html`

Smoke command:

```bash
scripts/formmax_fixture_smoke.js
```

The fixture covers:

- 96 deterministic capacity rows
- two pages
- lazy row rendering in chunks of 16
- sticky table header
- ordinary text, number, date, select, and checkbox controls
- receipt JSON
- confirmation-gated tax ID, signature, and legal attestation fields
- empty initial ordinary fields
- receipt generation from submitted DOM state, not from the expected oracle

## Acceptance

M10 passes when the fixture smoke script reports `FORMMAX FIXTURE PASS` and writes:

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/fixture_smoke/result.json`

The fixture smoke also rejects a blank submitted state. This prevents a runner from passing simply because the page already knows the answer.

Observed output:

```text
FORMMAX FIXTURE PASS rows=96 pages=2 sensitive_fields=3 result=runs/formmax/fixture_smoke/result.json
```

## Browser Runner

Runner command:

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

Observed three consecutive local passes:

```text
FORMMAX RUNNER PASS rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true
```

The first pass wrote:

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781233667392/result.json`

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781233667392/replay.jsonl`

Replay event counts from that run:

```text
field_discovered=675
field_focused=672
field_filled=672
field_verified=672
scroll_checkpoint=6
confirmation_required=3
field_blocked_sensitive=3
receipt_seen=1
```

Replay does not echo table values. A local leak check over 288 deterministic text/date/owner values found `replay_value_leaks=0`.
