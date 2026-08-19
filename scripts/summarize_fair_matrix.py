#!/usr/bin/env python3
"""Validate the complete 3-task x 2-order Saccade/Playwright evidence matrix."""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path
from typing import Any


EXPECTED_TASKS = {
    "selenium-official-web-form",
    "demoqa-react-practice-form",
    "angular-material-public-select",
}
EXPECTED_ORDERS = {
    ("saccade", "playwright"),
    ("playwright", "saccade"),
}


def summarize(reports: list[dict[str, Any]]) -> dict[str, Any]:
    cells: dict[tuple[str, tuple[str, ...]], dict[str, Any]] = {}
    errors: list[str] = []
    for report in reports:
        task = str((report.get("task") or {}).get("name") or "")
        order = tuple(report.get("order") or [])
        key = (task, order)
        if task not in EXPECTED_TASKS or order not in EXPECTED_ORDERS:
            errors.append(f"unexpected_cell:{task}:{'/'.join(order)}")
        elif key in cells:
            errors.append(f"duplicate_cell:{task}:{'/'.join(order)}")
        else:
            cells[key] = report
    for task in sorted(EXPECTED_TASKS):
        for order in sorted(EXPECTED_ORDERS):
            if (task, order) not in cells:
                errors.append(f"missing_cell:{task}:{'/'.join(order)}")
    if any(report.get("verdict") != "PASS" for report in cells.values()):
        errors.append("all_six_reports_must_pass")
    locks = {
        json.dumps(report.get("playwright_mcp"), sort_keys=True)
        for report in cells.values()
    }
    if len(locks) != 1:
        errors.append("playwright_lock_differs_between_cells")
    elif cells:
        lock = next(iter(cells.values())).get("playwright_mcp") or {}
        if lock.get("online_latest_verified") is not True:
            errors.append("playwright_freeze_version_not_verified")
    models = {json.dumps(report.get("agent"), sort_keys=True) for report in cells.values()}
    if len(models) != 1:
        errors.append("agent_model_differs_between_cells")
    metrics: dict[str, Any] = {}
    if not errors:
        for lane in ("saccade", "playwright"):
            lane_rows = [report["lanes"][lane] for report in cells.values()]
            metrics[lane] = {
                "median_elapsed_ms": round(statistics.median(row["timing"]["elapsed_ms"] for row in lane_rows), 3),
                "median_input_tokens": round(statistics.median(row["usage"]["input_tokens"] for row in lane_rows), 3),
                "median_tool_calls": round(statistics.median(row["tool_calls"] for row in lane_rows), 3),
                "all_passed": all(row["passed"] for row in lane_rows),
            }
    return {
        "schema": "saccade-agent-benchmark-matrix/1",
        "required_cells": 6,
        "valid_cells": len(cells),
        "status": "COMPLETE" if not errors else "BLOCKED",
        "public_comparison_claims_authorized": not errors,
        "errors": errors,
        "metrics": metrics,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="+", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    reports = [json.loads(path.read_text(encoding="utf-8")) for path in args.reports]
    result = summarize(reports)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": result["status"], "output": str(args.output)}))
    return 0 if result["status"] == "COMPLETE" else 2


if __name__ == "__main__":
    raise SystemExit(main())
