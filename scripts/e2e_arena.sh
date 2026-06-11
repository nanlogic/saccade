#!/usr/bin/env bash
set -euo pipefail

cargo run -p mousemax -- run \
  --site arena \
  --spawn-speed epic \
  --target-size tiny \
  --duration 15 \
  --seed 42 \
  --replay

latest_run="$(ls -td runs/arena/run_* | head -n 1)"
cargo run -p mousemax -- replay "${latest_run}/replay.jsonl" --summary
