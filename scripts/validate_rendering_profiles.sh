#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

extract_report_dir() {
  awk -F'report=' '/VISUAL PARITY PASS/ {print $2}' "$1" | tail -n 1 | sed 's#/index.html$##'
}

run_profile() {
  local profile="$1"
  local log_file="$2"
  "$ROOT/scripts/visual_parity_compare.py" \
    --timeout-sec 60 \
    --rendering-profile "$profile" \
    layout_probe | tee "$log_file"
}

safe_log="$(mktemp)"
modern_log="$(mktemp)"
default_worker_log="$(mktemp)"
trap 'rm -f "$safe_log" "$modern_log" "$default_worker_log"' EXIT

run_profile servo-safe "$safe_log"
run_profile servo-modern "$modern_log"

safe_dir="$(extract_report_dir "$safe_log")"
modern_dir="$(extract_report_dir "$modern_log")"
layout_url="$(python3 -c 'import pathlib, sys; print(pathlib.Path(sys.argv[1]).resolve().as_uri())' "$ROOT/test_pages/visual_parity/layout_probe/index.html")"

printf '{"id":1,"method":"ping"}\n{"id":2,"method":"close"}\n' | \
  RUST_LOG=error cargo run -q -p saccade-shell -- \
    browser-session-worker --url "$layout_url" > "$default_worker_log"

python3 - "$safe_dir/visual_parity_manifest.json" "$modern_dir/visual_parity_manifest.json" "$default_worker_log" <<'PY'
import json
import pathlib
import sys

safe = json.loads(pathlib.Path(sys.argv[1]).read_text())
modern = json.loads(pathlib.Path(sys.argv[2]).read_text())
default_worker_log = pathlib.Path(sys.argv[3])

safe_fixture = safe["fixtures"][0]
modern_fixture = modern["fixtures"][0]

safe_recorded = safe.get("rendering_profile") == "servo-safe"
modern_recorded = modern.get("rendering_profile") == "servo-modern"
modern_grid = (
    modern_fixture.get("layout_probe_metrics", {}).get("display_mismatches") == 0
    and modern_fixture.get("layout_probe_metrics", {}).get("grid_template_mismatches") == 0
)
modern_max_delta = modern_fixture.get("layout_probe_metrics", {}).get("max_rect_delta", 999999)

if not safe_recorded:
    raise SystemExit("servo-safe profile was not recorded in the manifest")
if not modern_recorded:
    raise SystemExit("servo-modern profile was not recorded in the manifest")
if not modern_grid:
    raise SystemExit("servo-modern did not match Chrome Grid display/template probes")
if modern_max_delta > 8:
    raise SystemExit(f"servo-modern layout probe rect delta too high: {modern_max_delta}")

default_profile = None
for line in default_worker_log.read_text().splitlines():
    if not line.startswith("{"):
        continue
    value = json.loads(line)
    if value.get("id") == 1:
        default_profile = value.get("result", {}).get("rendering_profile")
        break
if default_profile != "servo-modern":
    raise SystemExit(f"default browser-session worker profile was {default_profile!r}, expected servo-modern")

print(
    "RENDERING_PROFILE PASS "
    f"servo_safe_recorded={str(safe_recorded).lower()} "
    f"servo_modern_grid={str(modern_grid).lower()} "
    f"layout_probe_modern_max_delta_px={modern_max_delta} "
    f"default_worker_profile={default_profile}"
)
PY
