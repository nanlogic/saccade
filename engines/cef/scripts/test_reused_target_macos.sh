#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
STAMP=${SACCADE_REUSED_TARGET_STAMP:-$(date +%Y%m%d-%H%M%S)}
OUTPUT_ROOT=${SACCADE_REUSED_TARGET_OUTPUT:-$REPO_ROOT/runs/cef_truth_reflex/reused_target_$STAMP}
APP=${SACCADE_CEF_APP:-$REPO_ROOT/target/cef-release/Saccade.app}

if [ "${SACCADE_SKIP_CEF_BUILD:-0}" != "1" ]; then
  "$SCRIPT_DIR/build_macos.sh"
fi

python3 "$REPO_ROOT/scripts/probe_cef_truth_reflex.py" \
  --app "$APP" \
  --fixture "$REPO_ROOT/test_pages/chrome_truth_reflex/reused_target.html" \
  --output-dir "$OUTPUT_ROOT" \
  --targets 100

jq -e '
  .verdict == "PASS" and
  .targets_receipted == 100 and
  .hits == 100 and
  .misses == 0 and
  .finished == true and
  .redaction.pass == true and
  .route.cdp_used == false
' "$OUTPUT_ROOT/report.json" >/dev/null

printf 'REUSED_TARGET_REFLEX_GATE=PASS report=%s\n' \
  "$OUTPUT_ROOT/report.json"
