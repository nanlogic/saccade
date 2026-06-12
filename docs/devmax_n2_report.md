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
DEVMAX FIXTURES PASS total=16 detected=16 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/selftest_1781224856449
```

The live audit command also passed against a local fixture server:

```bash
cargo run -q -p devmax -- audit --url http://127.0.0.1:47892/modal_blocks_page/index.html --replay
```

Observed output:

```text
DEVMAX AUDIT PASS report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/audit_1781221056481/report.json replay=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/audit_1781221056481/replay.jsonl findings=1
```

Servo-backed probe gate now also passes:

```bash
cargo run -q -p devmax -- selftest-servo-fixtures
```

Observed output:

```text
DEVMAX SERVO FIXTURES PASS total=8 detected=8 false_positives=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/devmax/servo_selftest_1781224856481
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

The first gate is `engine=static-fixture-v0`; the new browser-backed path is `engine=servo-rendered-probe-v0`.

Together they prove the DEVMAX CLI/report contract, fixture corpus, replay artifact shape, browser-computed layout/style truth path, screenshot pixel checks for blank canvas regions, real click verification for enabled actions, and Servo delegate capture for console messages and resource-load requests.

Next DEVMAX step:

```text
Expand click verification from one action to multi-action smoke flows and add HTTP status awareness for resource loads.
```
