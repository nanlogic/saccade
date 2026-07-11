#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${SACCADE_PRODUCT_GATE_OUT:-$ROOT/runs/product_gate/release_${STAMP}}"
mkdir -p "$OUT"

log() {
  printf '[product-gate] %s\n' "$*"
}

run_step() {
  local name="$1"
  shift
  log "RUN $name"
  "$@" >"$OUT/$name.stdout.log" 2>"$OUT/$name.stderr.log"
}

cd "$ROOT"

run_step mcp_tests cargo test -p saccade-mcp
run_step mcp_selftest cargo run -q -p saccade-mcp -- selftest

python3 - "$OUT" <<'PY'
import json
import pathlib
import re
import sys

out = pathlib.Path(sys.argv[1])
selftest = (out / "mcp_selftest.stdout.log").read_text()
match = re.search(r"report=(\S+)", selftest)
if not match:
    raise SystemExit("PRODUCT GATE FAIL: MCP selftest did not report a JSON artifact")

report_path = pathlib.Path(match.group(1))
if not report_path.is_absolute():
    report_path = pathlib.Path.cwd() / report_path
report = json.loads(report_path.read_text())
required = {
    "tab_scoping": True,
    "local_dev_audit": True,
    "policy_gate": True,
}
missing = [key for key, expected in required.items() if report.get(key) != expected]
if not isinstance(report.get("tools_registered"), int) or report["tools_registered"] < 1:
    missing.append("tools_registered")
if missing:
    raise SystemExit(f"PRODUCT GATE FAIL: MCP report missing/failed {', '.join(missing)}")

summary = {
    "status": "PASS",
    "gate": "release_product_gate",
    "mcp_report": str(report_path),
    "tools_registered": report["tools_registered"],
    "tab_scoping": report["tab_scoping"],
    "local_dev_audit": report["local_dev_audit"],
    "policy_gate": report["policy_gate"],
    "known_runtime_warnings": [
        "GLD_TEXTURE_INDEX_2D may appear on macOS; it is recorded in stderr and is not a pass criterion."
    ],
}
(out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(
    "PRODUCT GATE PASS "
    f"tools_registered={summary['tools_registered']} "
    f"tab_scoping={summary['tab_scoping']} "
    f"local_dev_audit={summary['local_dev_audit']} "
    f"policy_gate={summary['policy_gate']} "
    f"summary={out / 'summary.json'}"
)
PY
