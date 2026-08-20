#!/usr/bin/env python3
"""Aggregate alternating clean-profile Chrome/Edge Truth latency runs."""

from __future__ import annotations

import argparse
import json
import math
import re
from pathlib import Path
from typing import Any


NAME = re.compile(r"^round-(\d+)-(first|second)-(chrome|edge)\.json$")


def metrics(values: list[float]) -> dict[str, float | int | None]:
    ordered = sorted(values)
    rank = lambda fraction: ordered[max(0, math.ceil(len(ordered) * fraction) - 1)] if ordered else None
    return {
        "samples": len(values),
        "p50_ms": round(rank(0.50), 3) if values else None,
        "p95_ms": round(rank(0.95), 3) if values else None,
        "p99_ms": round(rank(0.99), 3) if values else None,
        "max_ms": round(max(values), 3) if values else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--iterations", required=True, type=int)
    args = parser.parse_args()
    rows: list[dict[str, Any]] = []
    for path in sorted(args.input.glob("round-*.json")):
        match = NAME.match(path.name)
        if not match:
            continue
        evidence = json.loads(path.read_text(encoding="utf-8"))
        rows.append({
            "round": int(match.group(1)), "position": match.group(2), "browser": match.group(3),
            "path": str(path), "passed": evidence.get("passed") is True,
            "initial_full_ms": evidence.get("initial_full_ms"),
            "latency_samples_ms": evidence.get("latency_samples_ms", {}),
            "completeness": evidence.get("completeness", {}),
        })
    expected_rows = args.iterations * 2
    if len(rows) != expected_rows:
        raise RuntimeError(f"expected {expected_rows} matrix rows, found {len(rows)}")

    aggregate: dict[str, Any] = {}
    for browser in ("chrome", "edge"):
        browser_rows = [row for row in rows if row["browser"] == browser]
        scenarios = {}
        for scenario in ("single", "batch10", "batch100", "remove", "replace", "reorder", "canvas", "webgl"):
            values = [float(value) for row in browser_rows for value in row["latency_samples_ms"].get(scenario, [])]
            scenarios[scenario] = metrics(values)
        by_position = {}
        for position in ("first", "second"):
            position_rows = [row for row in browser_rows if row["position"] == position]
            by_position[position] = {
                "runs": len(position_rows),
                "initial_full": metrics([float(row["initial_full_ms"]) for row in position_rows]),
                "single": metrics([float(value) for row in position_rows for value in row["latency_samples_ms"].get("single", [])]),
                "batch100": metrics([float(value) for row in position_rows for value in row["latency_samples_ms"].get("batch100", [])]),
            }
        aggregate[browser] = {
            "runs": len(browser_rows),
            "initial_full": metrics([float(row["initial_full_ms"]) for row in browser_rows]),
            "scenarios": scenarios,
            "by_position": by_position,
            "missing_total": sum(len(row["completeness"].get("missing", [])) for row in browser_rows),
            "duplicate_total": sum(len(row["completeness"].get("duplicates", [])) for row in browser_rows),
            "empty_delta_total": sum(int(row["completeness"].get("empty_deltas", 0)) for row in browser_rows),
        }
    passed = all(row["passed"] for row in rows) and all(
        aggregate[browser][key] == 0
        for browser in ("chrome", "edge") for key in ("missing_total", "duplicate_total", "empty_delta_total")
    )
    report = {
        "schema": "saccade.truth-latency-matrix/1", "iterations": args.iterations,
        "clean_profile_per_browser_per_round": True, "alternating_order": True,
        "rows": [{key: row[key] for key in ("round", "position", "browser", "path", "passed")} for row in rows],
        "aggregate": aggregate, "passed": passed,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": passed, "output": str(args.output), "aggregate": aggregate}))
    if not passed:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
