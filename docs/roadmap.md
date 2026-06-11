# Saccade Roadmap

Date: 2026-06-11

## Formal v4 Plan

The checked-in build spec formally runs from M-1 through M8, then names Phase E as a separate post-benchmark effort.

- M-1: browser viability chat and kill gate
- M0: pinned Servo boots a blank WebView
- M1: real-site recon and go/no-go
- M2: `saccade_core` and `saccade_replay`
- M3: detector, motor, and verifier on synthetic data
- M4: calibration
- M5: synthetic pages
- M6: local arena defeated
- M7: real site defeated
- M8: agent API, replay visualization, and polish
- Phase E: Servo fork with engine taps

## Working Extension

We can keep the M numbers after M8 for project management, but those are extensions to the v4 spec.

- M8: finish artifact polish and agent API. Done so far: replay click map and `validate-run`.
- M9: repeat the headline gate on the target Linux/X11 benchmark machine and package reproducible release commands.
- M10: build a FORMMAX practical evaluation with scrolling tables, multi-page forms, PDF feasibility checks, and sensitive-field confirmation gates.
- M11: separate PDF filling feasibility and sensitive-field gates into offline artifacts that can pass without a live website.
- Phase E: add forked Servo engine taps after the stock-Servo product story is stable.

## Current Status

M7 passed on macOS arm64 with stock Servo `0.2.0`.

M8 through M11 now have local acceptance commands:

```text
M8: mousemax serve --port 0
M9: scripts/validate_m9_release.sh runs/real/run_1781193985
M10: scripts/formmax_fixture_smoke.js
M11: scripts/formmax_pdf_feasibility.py
```

FORMMAX planning lives in `docs/formmax_practical_eval_plan.md`.

## Next Plan v5

`docs/SACCADE_NEXT_PLAN_v5.md` reframes the project from MOUSEMAX expansion to an AI-first Playwright alternative:

```text
MOUSEMAX = trust proof
Trusted Tabs = safety and login handoff foundation
DEVMAX = developer usefulness proof
FORMMAX = practical workflow proof
```

Current active milestone:

```text
N1: Trusted Tabs Runtime
```

N1 minimal selftest now passes:

```text
cargo run -q -p saccade-shell -- selftest-tabs
```
