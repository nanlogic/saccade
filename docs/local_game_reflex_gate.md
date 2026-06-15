# Local Game Reflex Gate

Date: 2026-06-14

## North Star

The current launch-critical target is:

```text
An LLM can play our designed browser game through Saccade.
```

This does not mean the LLM thinks on every frame. The frame loop must be local
and deterministic:

```text
LLM strategy / high-level intent
  -> Saccade reflex runtime
  -> browser frame truth
  -> target/state detector
  -> motor decision
  -> low-latency browser/native input
  -> verification + replay
```

The LLM may choose goals, modes, or policies. It must not sit in the millisecond
control loop.

## Non-Negotiable Requirements

1. **Browser handles the game for humans**
   - The human-visible browser must render the local game correctly enough to
     inspect and trust the demo.
   - Official ServoShell is the current preferred human layer because it runs
     the local game better than the old embedded `servo=0.2.0` path.

2. **Agent has millisecond control**
   - The agent path cannot degrade to Chrome/Playwright-style DOM automation.
   - WebDriver may remain a product/safety adapter, but it is not the reflex
     runtime.
   - A passing reflex runtime must measure observe-to-input latency, not guess
     it.

3. **Truth layer stays safe**
   - General product truth defaults to redacted DOM/layout/action/form truth.
   - Screenshots are not the default agent truth channel for real user pages.
   - Pixel/frame truth is allowed only in explicitly non-sensitive reflex modes
     such as local games and benchmarks.

## Pass Gates

### R0: Browser Render Gate

Status: pass.

- `http://127.0.0.1:4173/` loads in official ServoShell.
- The game screenshot is nonblank and visually recognizable.
- Page truth reports title `Blend or Die - Prototype`.

Evidence:

```text
runs/servoshell_adapter/probe_1781484941056/report.json
runs/servoshell_adapter/probe_1781484941056/screenshot.png
```

Fast-path audit:

```text
docs/reflex_bridge_audit.md
```

### R1: Input Ownership Gate

Status: pending.

Required evidence:

- Saccade can move/control the player in the local game through a browser or
  native input path.
- The page state visibly changes and the replay records the input basis.
- WebDriver-only control is not enough unless measured latency and reliability
  meet the reflex target.

### R2: Frame Truth Gate

Status: pending.

Required evidence:

- Saccade receives game-frame truth at reflex speed.
- Preferred channel: in-process ServoShell frame/readback bridge.
- Temporary non-sensitive diagnostic channel may use game-page debug state or
  pixels only to validate local game progress, not as the product safety model.

### R3: Reflex Latency Gate

Status: pending.

Local game v0 pass:

- p95 observe-to-input dispatch <= 16 ms.
- 30+ seconds of stable control without freezing or losing the input path.
- Replay includes timestamps for observe, decision, dispatch, and verify.

MOUSEMAX/mouseaccuracy pass remains stricter:

- p95 detect-to-dispatch <= 5 ms.
- Zero misses / stale clicks under the benchmark acceptance table.

## Kill / Pivot Conditions

Saccade is not differentiated if the best available runtime is only:

```text
DOM observe -> WebDriver/Playwright action -> slow verification
```

If the official ServoShell route cannot expose ms-level frame truth and input
control, the next move is not more WebDriver glue. The next move is:

1. build/clone official ServoShell source,
2. add a thin in-process Saccade bridge around frame truth, input, safety, and
   replay,
3. keep official human rendering/UI behavior intact.

If that bridge cannot meet R1-R3, then the game/reflex demo is a kill/pivot
gate for the project.

## Immediate Work

1. Inspect the old Saccade/MOUSEMAX reflex code path and identify exactly which
   pieces must survive:
   - frame capture/readback,
   - target/state detector,
   - input dispatch,
   - replay timestamps.
2. Inspect official ServoShell source/build path and locate the minimum bridge
   insertion points.
3. Build a Local Game Reflex v0:
   - no LLM in frame loop,
   - no WebDriver hot loop,
   - local non-sensitive game only,
   - measured observe-to-input latency.
