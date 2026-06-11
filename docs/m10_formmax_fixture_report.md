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

## Acceptance

M10 passes when the fixture smoke script reports `FORMMAX FIXTURE PASS` and writes:

`/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/fixture_smoke/result.json`

This fixture does not yet run a browser automation filler. It creates the local benchmark surface and oracle that the filler will target.

Observed output:

```text
FORMMAX FIXTURE PASS rows=96 pages=2 sensitive_fields=3 result=runs/formmax/fixture_smoke/result.json
```
