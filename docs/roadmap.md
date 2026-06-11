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
- Phase E: add forked Servo engine taps after the stock-Servo product story is stable.

## Current Status

M7 passed on macOS arm64 with stock Servo `0.2.0`.

M8 is in progress. The next high-value item is the thin HTTP layer over the existing CLI orchestrator:

```text
/bench/mouseaccuracy/start
/bench/mouseaccuracy/status
/bench/mouseaccuracy/result
```

FORMMAX planning lives in `docs/formmax_practical_eval_plan.md`.
