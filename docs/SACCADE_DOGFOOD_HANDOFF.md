# Saccade Dogfood Handoff

Date: 2026-06-15

## Purpose

This file keeps the game-building session and the Saccade browser session from
overlapping.

The game session owns the game. The Saccade session owns browser truth, safety,
input, replay, and dogfood diagnostics.

## Current Saccade Baseline

```text
Use the latest SACCADE main branch unless Wayne says otherwise.
```

The local reflex runner defaults to `--policy visual`, uses live
`semantic_object_seen` facts to choose motor commands, and writes a
human-readable `review.html` at the end of each successful run.

## Ownership

Game session:

- May edit only the game repository.
- Verifies gameplay in Chrome first.
- Uses Saccade as a dogfood browser and evidence source.
- Does not patch the game around Saccade bugs unless the game is relying on
  invalid or fragile browser behavior.

Saccade session:

- May edit only this repository.
- Maintains the browser fact stream, semantic classification, reflex motor,
  safety policy, replay, and report artifacts.
- Investigates pages where Chrome passes but Saccade fails.
- Keeps failures reproducible through local fixtures, reports, or run artifacts.

## Triage Rule

Use this first question:

```text
Does Chrome fail too?
```

- Chrome fails: treat it as a game bug and fix it in the game session.
- Chrome passes, human Saccade fails: treat it as a Saccade browser/render/input
  bug.
- Chrome passes, human Saccade passes, agent Saccade fails: treat it as a
  Saccade truth/detector/motor/replay bug.

## Issue Report Format

When handing a problem to the Saccade session, append a short report with this
shape:

```text
Title:
Game commit:
Game URL:
Saccade commit:
Browser tested:
Expected:
Actual:
Repro steps:
Evidence:
Classification guess:
Does Chrome pass:
Does human Saccade pass:
Does agent Saccade pass:
```

Allowed classification guesses:

```text
render
input
resize
canvas
performance
agent_facts
motor
safety
unknown
```

## Current Reflex Acceptance

For the local game reflex gate, the Saccade session should produce:

- a release ServoShell run,
- `report.json`,
- `replay.jsonl`,
- `facts.jsonl`,
- `semantic_facts.jsonl`,
- `review.html`,
- command receipts,
- a short summary of `fill_delta`, `hp_delta`, `drop_delta`, command count,
  semantic fact count, and dispatch latency.

The game session can keep iterating while Saccade bugs are being investigated,
as long as Chrome remains a valid gameplay reference.

## Command For The Game Session

When the local game is running at `http://127.0.0.1:4173/`, the game session can
ask the Saccade session to run:

```bash
node scripts/run_local_game_reflex_loop.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 15000 \
  --policy visual \
  --visual-fact-interval-ms 1000 \
  --output-dir runs/local_game_reflex/<run_name>
```

The command prints `report`, `review`, `replay`, `facts`, and
`semantic_facts` paths. `review.html` is the first artifact to open for a quick
human read.
