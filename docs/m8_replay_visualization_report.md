# Saccade M8 Replay Visualization Report

Date: 2026-06-11

## Result

M8 started with replay visualization.

`mousemax replay` now accepts `--render-summary <png>`. The command reads an existing `replay.jsonl` file and renders a PNG click map from logged click receipts and click verification results.

`mousemax validate-run <run_dir>` now validates a run artifact bundle against the MOUSEMAX acceptance checks.

## Artifact

Source replay:

`/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/replay.jsonl`

Rendered map:

`/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/click_map.png`

The PNG is 1920x1080. It shows 47 green hit points from the M7 pixel-only real-site run.

This image is a replay-derived map, not a browser screenshot. The renderer takes the canvas size from the replay `RunStarted` config, draws the last reported game area, then plots each click at its logged CSS coordinate. Green means `ClickVerified { outcome: Hit }`; red, amber, and purple represent miss, unknown, and stale outcomes.

## Command

```bash
cargo run -p mousemax -- replay \
  runs/real/run_1781193985/replay.jsonl \
  --summary \
  --render-summary runs/real/run_1781193985/click_map.png
```

Observed output:

```text
REPLAY SUMMARY verdict=PASS hits=47 misses=0 targets_seen=47 clicks_sent=47 detect_to_dispatch_p95_ms=0.200 first_visible_to_dispatch_p95_ms=16.000 replay=runs/real/run_1781193985/replay.jsonl
REPLAY RENDER summary=runs/real/run_1781193985/click_map.png
```

## Verification

```bash
cargo fmt
cargo check -p mousemax
file runs/real/run_1781193985/click_map.png
cargo run -q -p mousemax -- validate-run runs/real/run_1781193985 --require-click-map
```

`cargo check -p mousemax` passed. `file` reports `PNG image data, 1920 x 1080, 8-bit/color RGBA, non-interlaced`.

The validator reported:

```text
VALIDATE PASS run=runs/real/run_1781193985 verdict=PASS site=real instrumentation=none hits=47 misses=0 targets_seen=47 clicks_sent=47 detect_to_dispatch_p95_ms=0.200 first_visible_to_dispatch_p95_ms=16.000
```

## Use

Use this map next to `before.png`, `after.png`, and `result.json` in the article or demo video. It gives readers a fast way to see where Saccade clicked during the run without parsing JSONL.

For a public post, label it as a replay visualization. Do not present it as a captured browser frame.

Use the validator output as the artifact integrity line in launch material.
