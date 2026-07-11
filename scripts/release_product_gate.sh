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
run_step generic_form_boundary python3 scripts/probe_generic_form_plan.py --output-dir "$OUT/generic_form_boundary"

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

form_report_path = out / "generic_form_boundary" / "report.json"
form_report = json.loads(form_report_path.read_text())
form_required = {
    "ok": True,
    "receipt_verified": True,
    "mcp_attached": True,
    "values_logged": False,
    "stale_plan_blocked": True,
    "stale_execution_blocked": True,
    "unsafe_policy_blocked": True,
    "wrong_plan_blocked": True,
    "structured_assignment_blocked": True,
}
form_missing = [key for key, expected in form_required.items() if form_report.get(key) != expected]
if form_report.get("sensitive_count", 0) < 1:
    form_missing.append("sensitive_count")
if form_report.get("preserved_count", 0) < 1:
    form_missing.append("preserved_count")
if form_missing:
    raise SystemExit(
        "PRODUCT GATE FAIL: generic form safety boundary missing/failed "
        + ", ".join(form_missing)
    )

summary = {
    "status": "PASS",
    "gate": "release_product_gate",
    "mcp_report": str(report_path),
    "tools_registered": report["tools_registered"],
    "tab_scoping": report["tab_scoping"],
    "local_dev_audit": report["local_dev_audit"],
    "policy_gate": report["policy_gate"],
    "generic_form_boundary_report": str(form_report_path),
    "generic_form_boundary": {
        "receipt_verified": form_report["receipt_verified"],
        "sensitive_count": form_report["sensitive_count"],
        "preserved_count": form_report["preserved_count"],
        "values_logged": form_report["values_logged"],
    },
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
    f"form_boundary=pass "
    f"summary={out / 'summary.json'}"
)
PY
