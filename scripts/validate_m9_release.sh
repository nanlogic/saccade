#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="${1:-runs/real/run_1781193985}"
ABS_RUN_DIR="$ROOT/$RUN_DIR"

cd "$ROOT"

if [[ ! -f "$ABS_RUN_DIR/result.json" ]]; then
  echo "missing result.json in $ABS_RUN_DIR" >&2
  exit 1
fi

if [[ ! -f "$ABS_RUN_DIR/replay.jsonl" ]]; then
  echo "missing replay.jsonl in $ABS_RUN_DIR" >&2
  exit 1
fi

cargo check -p mousemax

cargo run -q -p mousemax -- replay \
  "$RUN_DIR/replay.jsonl" \
  --summary \
  --render-summary "$RUN_DIR/click_map.png"

cargo run -q -p mousemax -- validate-run "$RUN_DIR" --require-click-map

file "$ABS_RUN_DIR/before.png" "$ABS_RUN_DIR/after.png" "$ABS_RUN_DIR/click_map.png"

echo "M9 RELEASE VALIDATION PASS run=$RUN_DIR"
