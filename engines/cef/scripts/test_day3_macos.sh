#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
STAMP=${SACCADE_DAY3_STAMP:-$(date +%Y%m%d-%H%M%S)}
OUTPUT_ROOT=${SACCADE_DAY3_OUTPUT:-$REPO_ROOT/runs/cef_truth_reflex/day3_3x100_$STAMP}

if [ "${SACCADE_SKIP_CEF_BUILD:-0}" != "1" ]; then
  "$SCRIPT_DIR/build_macos.sh"
fi

mkdir -p "$OUTPUT_ROOT"
for run in 1 2 3; do
  python3 "$REPO_ROOT/scripts/probe_cef_truth_reflex.py" \
    --output-dir "$OUTPUT_ROOT/run$run" \
    --targets 100
done

jq -s '
  {
    schema: "saccade-cef-day3-aggregate-v1",
    verdict: (if all(.[]; .verdict == "PASS") then "PASS" else "FAIL" end),
    runs: [
      .[] | {
        verdict,
        targets_receipted,
        hits,
        misses,
        redaction_pass: .redaction.pass,
        full_loop_p95_ms: .latency.renderer_fact_to_input_receipt.p95_ms,
        cdp_used: .route.cdp_used
      }
    ]
  }
' "$OUTPUT_ROOT"/run*/report.json > "$OUTPUT_ROOT/aggregate.json"

jq -e '
  .verdict == "PASS" and
  all(.runs[];
    .targets_receipted == 100 and
    .hits == 100 and
    .misses == 0 and
    .redaction_pass == true and
    .full_loop_p95_ms <= 20 and
    .cdp_used == false)
' "$OUTPUT_ROOT/aggregate.json" >/dev/null

if rg -n '123-45-6789|correct-horse-battery' "$OUTPUT_ROOT" >/dev/null; then
  echo "Sensitive sentinel reached a Day 3 artifact" >&2
  exit 1
fi

printf 'DAY3_CEF_TRUTH_REFLEX_GATE=PASS report=%s\n' "$OUTPUT_ROOT/aggregate.json"
