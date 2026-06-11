#!/usr/bin/env bash
set -euo pipefail

duration="${SACCADE_RECORD_SECONDS:-32}"
output="${1:-runs/real/m7_pixel_demo_$(date +%Y%m%d_%H%M%S).mov}"
mkdir -p "$(dirname "$output")"

echo "Recording main display for ${duration}s to ${output}"
echo "If this fails with no video, grant Screen Recording permission to the terminal/Codex app and rerun."

screencapture -v -V "$duration" -x -D 1 "$output" &
record_pid=$!

sleep 2

RUST_LOG=error cargo run -q -p mousemax -- run \
  --site real \
  --spawn-speed epic \
  --target-size tiny \
  --duration 15 \
  --window-width 1920 \
  --window-height 1080 \
  --instrumentation none \
  --replay

wait "$record_pid"

if [[ ! -s "$output" ]]; then
  echo "Recording did not produce a video: ${output}" >&2
  exit 1
fi

ls -lh "$output"
