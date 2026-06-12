#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="selftest_$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)"
OUT="$ROOT/runs/chrome_reference/$RUN_ID"

"$ROOT/scripts/capture_chrome_reference.sh" \
  "file://$ROOT/test_pages/browser_session/index.html" \
  "$OUT/browser_session" \
  1280 \
  800 >/dev/null

"$ROOT/scripts/capture_chrome_reference.sh" \
  "file://$ROOT/test_pages/chrome_reference_blocking/index.html" \
  "$OUT/blocking_fixture" \
  1280 \
  800 >/dev/null

normal_actions="$(jq -r '.page.actions' "$OUT/browser_session/chrome_reference_manifest.json")"
normal_blocked="$(jq -r '.network.blocked_requests' "$OUT/browser_session/chrome_reference_manifest.json")"
blocking_actions="$(jq -r '.page.actions' "$OUT/blocking_fixture/chrome_reference_manifest.json")"
blocking_blocked="$(jq -r '.network.blocked_requests' "$OUT/blocking_fixture/chrome_reference_manifest.json")"

if [[ "$normal_actions" -lt 1 || "$normal_blocked" -ne 0 ]]; then
  echo "CHROME REFERENCE FAIL normal_actions=$normal_actions normal_blocked=$normal_blocked report=$OUT" >&2
  exit 1
fi

if [[ "$blocking_actions" -lt 1 || "$blocking_blocked" -lt 3 ]]; then
  echo "CHROME REFERENCE FAIL blocking_actions=$blocking_actions blocking_blocked=$blocking_blocked report=$OUT" >&2
  exit 1
fi

echo "CHROME REFERENCE PASS normal_actions=$normal_actions blocking_blocked=$blocking_blocked report=$OUT"
