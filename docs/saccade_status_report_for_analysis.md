# Saccade Status Report For Analysis

Date: 2026-06-11

## Current State

Saccade has passed the original MOUSEMAX benchmark on macOS arm64 with stock Servo `0.2.0`.

The strongest run is:

`/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985`

Result:

- Site: `https://mouseaccuracy.com/classic/`
- Difficulty: Epic spawn speed, Tiny target size, 15 seconds
- Instrumentation: `none`
- Hits: 47
- Misses: 0
- Targets seen: 47
- Clicks sent: 47
- False positives: 0
- Stale clicks: 0
- Unknown verifications: 0
- p95 detect-to-dispatch: 0.200 ms
- p95 first-visible-to-dispatch: 16.000 ms

This run used rendered RGBA pixels for target detection. It did not read target DOM state. The real-time loop made no LLM calls.

## What We Built

### M7: Real Site Benchmark

Saccade can run the real Mouse Accuracy site in Servo, select Epic + Tiny, detect targets, click through Servo input events, and verify the site's own hit/miss counters.

Evidence:

- Five consecutive real-site `observe_only` PASS runs at 1920x1080.
- One pure-pixel `instrumentation=none` PASS run at 1920x1080.
- Replay logs, result JSON, before/after screenshots, and a replay click map.

### M8: API And Artifact Polish

We added:

- `mousemax replay --render-summary <png>`
- `mousemax validate-run <run_dir> --require-click-map`
- `mousemax serve --port <port>`
- HTTP endpoints:
  - `/bench/mouseaccuracy/start`
  - `/bench/mouseaccuracy/status`
  - `/bench/mouseaccuracy/result`

Status/result smoke tests passed. `/start` launches benchmark runs through a child `mousemax run` process so Servo's window loop stays isolated.

### M9: Release Validation

We added a release artifact check:

```bash
scripts/validate_m9_release.sh runs/real/run_1781193985
```

It recomputes replay summary, regenerates `click_map.png`, validates the run bundle, and checks PNG headers.

Observed result:

```text
M9 RELEASE VALIDATION PASS run=runs/real/run_1781193985
```

### M10: FORMMAX Practical Fixture

We added a local practical form benchmark:

`/Users/waynema/Documents/GitHub/SACCADE/test_pages/formmax/index.html`

It includes:

- 96 deterministic capacity rows
- two-page flow
- scroll container with lazy row rendering
- text, number, date, select, and checkbox controls
- receipt JSON
- sensitive fields: tax ID, signature, legal attestation

Smoke test:

```bash
scripts/formmax_fixture_smoke.js
```

Observed result:

```text
FORMMAX FIXTURE PASS rows=96 pages=2 sensitive_fields=3
```

Browser runner:

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

Observed result:

```text
FORMMAX RUNNER PASS rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true
```

### M11: PDF And Sensitive Field Feasibility

We added an offline PDF feasibility harness:

```bash
scripts/formmax_pdf_feasibility.py
```

It generates:

- a fillable AcroForm PDF,
- a flat PDF with no form fields,
- a completed PDF with only non-sensitive fields filled,
- a result JSON report.

It classifies tax ID, signature, and legal attestation as sensitive and verifies they stay empty without user confirmation.

Observed result:

```text
FORMMAX PDF FEASIBILITY PASS acroform_fields=5 sensitive_fields=3
```

## What Is Proven

- Stock Servo can provide enough rendered-state access and input control to beat the real Mouse Accuracy benchmark on the tested macOS setup.
- The pixel-only path can solve the benchmark without target DOM reads.
- Replay logs can reproduce the run summary and generate a click map.
- A local HTTP shell can expose benchmark start/status/result for an agent.
- The project now has a local practical-form fixture and browser runner for long scrolling forms and multi-page submission.
- The FORMMAX runner can fill non-sensitive table fields, block sensitive fields, verify the receipt, and produce replay JSONL without echoing table values.
- The project can detect sensitive PDF/form fields and keep them gated in offline tests.

## What Is Not Proven Yet

- Linux/X11 has not yet repeated the M7 real-site gate.
- FORMMAX v0 drives trusted fixture DOM controls inside a Servo-loaded page; it is not yet native keyboard text entry.
- M11 does not yet fill PDFs through the browser PDF viewer. It uses a programmatic AcroForm path.
- Sensitive-field confirmation is represented by policy and offline tests. It does not yet have a live user confirmation UI.
- Replay timestamps measure when Servo input dispatch returned, not when page JavaScript processed the event.

## Recommended Next Work

1. Run the M7 gate on Linux/X11.
2. Continue N2 login handoff: Human tab logs in, Agent tab inherits the session without seeing passwords or OTP.
3. Start DEVMAX local agent self-test fixtures.
4. Harden FORMMAX with screenshots, `validate-run`, native input-event typing where Servo supports it, and comparison baselines.
5. Add a user confirmation UI for sensitive fields.
6. Add a public-facing report page that links the M7 artifact, click map, validator output, and caveats.

## Next Plan v5 Update

The next plan reframes Saccade as an AI-first Playwright alternative:

```text
Browser truth -> verified actions
```

MOUSEMAX is now evidence. Product work starts with Trusted Tabs and DEVMAX.

N1 minimal Trusted Tabs runtime now passes:

```text
TABS PASS webviews=2 cookie_shared=true storage_shared=true input_isolated=true read_policy_enforced=true
```

## Useful Commands

```bash
cargo check -p mousemax
scripts/validate_m9_release.sh runs/real/run_1781193985
scripts/formmax_fixture_smoke.js
scripts/formmax_pdf_feasibility.py
cargo run -q -p mousemax -- validate-run runs/real/run_1781193985 --require-click-map
cargo run -q -p mousemax -- serve --port 47891
```

## Latest Commits

- `d236bab complete m8 m11 local gates`
- `a0ae81b plan formmax practical evaluation`
- `763ff8f add run artifact validator`
- `9b0aba9 add replay click map rendering`
- `5441ce2 document m7 launch artifacts`
