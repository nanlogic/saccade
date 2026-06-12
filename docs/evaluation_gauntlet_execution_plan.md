# Saccade Evaluation Gauntlet Execution Plan

Updated: 2026-06-12

Source spec: `docs/SACCADE_EVALUATION_GAUNTLET_v1.md`

## Decision

Use two lanes:

1. Demo lane: prove the MOUSEMAX story in a way a skeptical viewer can inspect.
2. Product lane: prove Saccade can handle practical browser work across forms, dev debugging, safety gates, replay, and eventually Chrome parity.

Do not treat one Mouse Accuracy demo as the product proof. It is the trust proof.

## Lane A: Demo Target

Target: `https://mouseaccuracy.com/classic/`

Current strongest Saccade run:

- Run: `runs/real/run_1781193985`
- Difficulty: Epic spawn speed, Tiny target size, 15 seconds
- Result: 47 hits, 0 misses, 0 stale clicks, 0 expired unclicked, 0 false positives
- Instrumentation: none
- LLM frame calls: 0
- Validator: PASS

Current artifact:

- `runs/real/run_1781193985/parity_review.html`

What this already proves:

- Saccade loaded the public Mouse Accuracy URL.
- Saccade used replayable browser actions, not a mocked score.
- The replay-derived click map matches the result count.
- The validator can independently reject stale, missed, or overclaimed runs.

What is still pending:

- Firefox URL-bar screenshot reference on a machine with Firefox installed.
- Optional Chrome result screenshot.
- Full Chrome-engine automated click comparison.

Important wording:

```text
The current comparison is Saccade run evidence against Chrome/Safari page references.
It is not yet a Chrome automated click-run baseline. That comes with the Chrome adapter gate.
```

## Lane B: Product Target

Use the gauntlet as the product scoreboard. The minimum public launch bundle is:

1. MOUSEMAX evidence freeze.
2. FORMMAX local runner pass.
3. DEVMAX fixture pass.
4. Trusted Tabs safety pass.
5. One Chrome adapter demo pass.

The strong bundle adds:

- UI Torture selected tasks.
- WebArena selected tasks.
- RealWorld or local Discourse thread workflow.
- PDF suite.
- Baseline comparison against a conventional browser automation stack.

## Attack Order

Use this order unless a blocker forces a swap:

1. Finish Safety truth v1 replay metadata for Human/Agent boundaries and sensitive-field masking.
2. Move Chrome/Firefox visual parity earlier so UI review and public demos are credible.
3. Finish MOUSEMAX parity pack references for `run_1781193985`. Chrome and Safari URL-bar references are now complete; Firefox is pending because this machine does not have Firefox installed.
4. Finish DEVMAX gauntlet evidence polish: screenshot crop/evidence per finding and replay.
5. Build FORMMAX Servo input runner for the local two-page scrolling table fixture.
6. Add MCP skeleton only after DEVMAX and FORMMAX have useful report shapes.
7. Build Chrome adapter v0 for reference runs and demo parity.
8. Run UI Torture selected public targets.
9. Add RealWorld/Discourse draft-only thread workflow.
10. Add selected WebArena tasks.
11. Add WorkArena-like enterprise tasks.
12. Add PDF suite.
13. Add baseline comparison.

## Done Means

A target is not conquered when Saccade clicks something. It is conquered only when:

- truth report exists,
- action map exists,
- action executed through an approved input path,
- result verified,
- replay saved,
- failure modes classified,
- unsafe actions gated,
- baseline comparison exists or is explicitly deferred.
