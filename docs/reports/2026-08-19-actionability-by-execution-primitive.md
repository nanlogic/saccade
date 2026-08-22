# Actionability by execution primitive

Status: Chrome development evidence for candidate
`2d8a877e3dc1b5c9a003aa3662ea9ddad506a7033aba286e1c48e21fe8af2612`
(`0.3.23`). This is not Playwright superiority evidence.

## Correction

The prior Collector applied one Playwright-style policy to every software
action: an animating target had to hold the same geometry for two consecutive
animation frames. That is appropriate for an ordinary physical-pointer-like
click, but a continuously moving `reflex_target` can never satisfy it. The
result was a prepare timeout before any software dispatch.

The corrected policy keeps the existing ordinary-control wait. A software
`reflex_target` click instead dispatches to the exact current authorized DOM
object without requiring stable geometry, browser focus, or coordinate
topmost. It still requires current document, identity, token, affordance,
visibility, enablement, and post-action advancement of the same loop class's
`reflex_occurrence`. Replacement remains stale and is never silently rebound.

## Verification

- Extension tests: 65/65 passed.
- Rust workspace: 101 tests passed across unit, wire, closed-loop, and doc
  suites.
- Setup: 16/16 passed; focused Python tests: 8/8 passed.
- Architecture, authority-integrity, formatting, and candidate checks passed.
- Ordinary Chrome, five iterations each:
  - settling animation 5/5;
  - temporary overlay 5/5;
  - delayed enablement 5/5;
  - DOM replacement 5/5 stale plus 5/5 fresh-object recovery;
  - continuously moving reflex 5/5, zero stale.
- Ordinary Chrome, public `saccade.act` moving-reflex fixture: 100/100 verified,
  zero stale, zero replacement recovery; 58.57 ms mean total local preparation
  accounting.
- MouseAccuracy `Insane + Tiny`, development Reference Actuator using the same
  Extension software dispatch path: 96/96 verified, zero failures, 14.16 ms
  p50 and 28.01 ms p95 observation-to-receipt latency.

The MouseAccuracy run has `reference_actuator` provenance and cannot be cited
as public `saccade.act` evidence. The 100-action moving fixture is the public
object-addressed product-route proof; the site run separately proves that the
shared low-level software dispatch path again handles the original dogfood.

Evidence:

- `~/Library/Application Support/Saccade Dev/evidence/20260819-actionability-policy/ordinary-and-reflex-smoke.json`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-actionability-policy/continuous-reflex-100.json`
- `~/Library/Application Support/Saccade Dev/evidence/20260819-actionability-policy/mouseaccuracy-reference-soft-96.json`
