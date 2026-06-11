# Saccade N2 DEVMAX Local Self-Test Report

Date: 2026-06-11

## Result

N2 minimal DEVMAX fixture gate passed.

Command:

```bash
cargo run -q -p devmax -- selftest-fixtures
```

Observed output:

```text
DEVMAX FIXTURES PASS total=16 detected=16 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/selftest_1781221142060
```

The live audit command also passed against a local fixture server:

```bash
cargo run -q -p devmax -- audit --url http://127.0.0.1:47892/modal_blocks_page/index.html --replay
```

Observed output:

```text
DEVMAX AUDIT PASS report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/audit_1781221056481/report.json replay=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/audit_1781221056481/replay.jsonl findings=1
```

## What Was Built

Added a new `devmax` binary with:

- `devmax selftest-fixtures`
- `devmax audit --url <http://...> --replay`

Added 16 deterministic local fixtures under:

`/Users/waynema/Documents/GitHub/SACCADE/test_pages/devmax/`

Fixture coverage:

- blank page
- console error
- hydration error
- missing asset
- invisible text
- overlapping elements
- offscreen button
- button without handler
- broken form validation
- lazy route error
- submit hidden in scroll container
- mobile responsive break
- modal blocking page
- blank canvas chart
- z-index overlay bug
- wrong success state

## Report Shape

Each audit writes a compact JSON report with:

- `engine`
- `page_revision`
- `url`
- `title`
- `summary`
- `visual_health`
- `runtime_health`
- `actions`
- `findings`
- `recommendations`
- artifact paths

Each fixture also writes a replay JSONL with run-start, finding, and run-finished events.

## Scope

This is `engine=static-fixture-v0`.

It proves the DEVMAX CLI/report contract and fixture gate. It does not yet claim full rendered truth, screenshots, computed layout, browser console capture, or click verification through Servo.

Next DEVMAX step:

```text
Replace static fixture markers with Servo-backed rendered truth and actionability checks.
```
