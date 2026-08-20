#!/usr/bin/env python3
"""Measure page-mutation-to-MCP latency and semantic delta completeness."""

from __future__ import annotations

import argparse
import json
import math
import re
import time
from pathlib import Path
from typing import Any

from dev_probe import Mcp, fold_truth_change, wait_for_mcp


MARKER = re.compile(r"^LT\|([^|]+)\|([^|]+)\|(\d+(?:\.\d+)?)$")


def raw_tool(mcp: Mcp, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    result = mcp.rpc("tools/call", {"name": f"saccade.{name}", "arguments": arguments}, timeout=timeout)
    return result["structuredContent"]


def percentile(values: list[float], fraction: float) -> float:
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def latency_metrics(values: list[float]) -> dict[str, float | int | None]:
    return {
        "samples": len(values),
        "min_ms": round(min(values), 3) if values else None,
        "p50_ms": round(percentile(values, 0.50), 3) if values else None,
        "p95_ms": round(percentile(values, 0.95), 3) if values else None,
        "p99_ms": round(percentile(values, 0.99), 3) if values else None,
        "max_ms": round(max(values), 3) if values else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--single-p95-limit-ms", type=float, default=50.0)
    parser.add_argument("--batch10-p95-limit-ms", type=float, default=100.0)
    parser.add_argument("--batch100-p95-limit-ms", type=float, default=500.0)
    parser.add_argument("--lifecycle-max-limit-ms", type=float, default=250.0)
    parser.add_argument("--initial-full-limit-ms", type=float, default=500.0)
    args = parser.parse_args()

    expected = ({f"single:{i}" for i in range(1, 21)}
                | {f"batch10:{i}" for i in range(10)}
                | {f"batch100:{i}" for i in range(100)}
                | {"remove:event", "replace:new", "reorder:event", "canvas:semantic", "webgl:semantic",
                   "dialog:text", "live:status", "done:1"})
    mcp = wait_for_mcp(args.runtime, args.runtime_dir)
    try:
        opened = raw_tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        initial = mcp.tool("truth.read", {"tab_id": tab_id})
        initial_received_at = time.time() * 1000
        revision = int(initial["revision"])
        current = {item["object_id"]: item for item in initial.get("objects", [])}
        start = next((item for item in current.values()
                      if item.get("role") == "button" and item.get("name") == "Start latency run"), None)
        remove_id = next((key for key, item in current.items() if item.get("text") == "Remove target"), None)
        replace_id = next((key for key, item in current.items() if item.get("text") == "Replace target old"), None)
        reorder_ids = {key for key, item in current.items() if str(item.get("text", "")).startswith("Stable reorder item ")}
        surface_roles = {item.get("name"): item.get("role") for item in current.values() if item.get("role") == "opaque_surface"}
        if not start or not remove_id or not replace_id or len(reorder_ids) != 100:
            raise RuntimeError("initial lifecycle oracle is incomplete")
        if surface_roles.get("Canvas semantic companion idle") != "opaque_surface" or surface_roles.get("WebGL semantic companion idle") != "opaque_surface":
            raise RuntimeError("initial Canvas/WebGL opaque surfaces are incomplete")
        initial_marker = next((MARKER.match(str(item.get("text", ""))) for item in current.values()
                               if str(item.get("text", "")).startswith("LT|initial|1|")), None)
        if not initial_marker:
            raise RuntimeError("initial full timing marker is absent")
        initial_full_ms = round(initial_received_at - float(initial_marker.group(3)), 3)

        started = raw_tool(mcp, "act", {
            "tab_id": tab_id,
            "document_id": initial["document_id"],
            "basis_revision": revision,
            "object_id": start["object_id"],
            "operation": "click",
        })
        if started.get("verified") is not True:
            raise RuntimeError("latency fixture did not verify its explicit start action")
        revision = int(started["revision"])

        seen: dict[str, float] = {}
        duplicates: list[str] = []
        delivery_batches: list[dict[str, Any]] = []
        empty_views: list[dict[str, Any]] = []
        empty_deltas = 0
        remove_disappeared = False
        replace_disappeared = False
        reorder_identity_changes = 0
        pending_page = False
        deadline = time.monotonic() + 20
        while "done:1" not in seen and time.monotonic() < deadline:
            read_arguments = {"tab_id": tab_id}
            if not pending_page:
                read_arguments.update({"after_revision": revision, "timeout_ms": 5000})
            try:
                view = raw_tool(mcp, "truth.read", read_arguments, timeout=7)
            except RuntimeError as error:
                if "no observation after revision" in str(error):
                    break
                raise
            revision = int(view["revision"])
            pending_page = (view.get("page") or {}).get("complete") is False
            changes = view.get("changes", [])
            if not changes:
                empty_deltas += 1
                empty_views.append({
                    "mode": view.get("mode"),
                    "revision": revision,
                    "object_count": len(view.get("objects", [])),
                    "gap": view.get("gap"),
                })
            received_at = time.time() * 1000
            delivered_markers: list[dict[str, Any]] = []
            for change in changes:
                object_id = change.get("object_id") or change.get("object", {}).get("object_id")
                kind = change.get("kind")
                if kind == "disappeared":
                    remove_disappeared |= object_id == remove_id
                    replace_disappeared |= object_id == replace_id
                    reorder_identity_changes += int(object_id in reorder_ids)
                    current.pop(object_id, None)
                    continue
                item = fold_truth_change(current, change, view.get("object_defaults")) or {}
                reorder_identity_changes += int(kind == "appeared" and str(item.get("text", "")).startswith("Stable reorder item "))
                match = MARKER.match(str(item.get("text") or item.get("name") or ""))
                if not match:
                    continue
                key = f"{match.group(1)}:{match.group(2)}"
                latency = received_at - float(match.group(3))
                delivered_markers.append({"key": key, "latency_ms": round(latency, 3)})
                if key in seen:
                    duplicates.append(key)
                else:
                    seen[key] = round(latency, 3)
            if delivered_markers:
                delivery_batches.append({
                    "revision": revision,
                    "change_count": len(changes),
                    "markers": delivered_markers,
                })

        missing = sorted(expected - set(seen))
        latencies = [value for key, value in seen.items() if key != "done:1"]
        metrics = latency_metrics(latencies)
        by_scenario = {
            scenario: latency_metrics([value for key, value in seen.items() if key.startswith(f"{scenario}:")])
            for scenario in ("single", "batch10", "batch100", "remove", "replace", "reorder", "canvas", "webgl", "dialog", "live")
        }
        samples_by_scenario = {
            scenario: [value for key, value in seen.items() if key.startswith(f"{scenario}:")]
            for scenario in ("single", "batch10", "batch100", "remove", "replace", "reorder", "canvas", "webgl", "dialog", "live")
        }
        lifecycle_max = max(by_scenario[name]["max_ms"] or 0 for name in ("remove", "replace", "reorder", "canvas", "webgl", "dialog", "live"))
        latency_passed = (
            (by_scenario["single"]["p95_ms"] or float("inf")) <= args.single_p95_limit_ms
            and (by_scenario["batch10"]["p95_ms"] or float("inf")) <= args.batch10_p95_limit_ms
            and (by_scenario["batch100"]["p95_ms"] or float("inf")) <= args.batch100_p95_limit_ms
            and lifecycle_max <= args.lifecycle_max_limit_ms
            and initial_full_ms <= args.initial_full_limit_ms
        )
        passed = (not missing and not duplicates and empty_deltas == 0 and remove_disappeared
                  and replace_disappeared and reorder_identity_changes == 0
                  and latency_passed)
        evidence = {
            "schema": "saccade.truth-latency-evidence/1",
            "browser": opened.get("browser"),
            "tab_id": tab_id,
            "clock": "same-machine epoch milliseconds embedded by fixture at mutation and sampled after MCP return",
            "thresholds": {
                "initial_full_ms": args.initial_full_limit_ms,
                "single_p95_ms": args.single_p95_limit_ms,
                "batch10_p95_ms": args.batch10_p95_limit_ms,
                "batch100_p95_ms": args.batch100_p95_limit_ms,
                "lifecycle_max_ms": args.lifecycle_max_limit_ms,
                "missing": 0, "duplicates": 0, "empty_deltas": 0,
            },
            "initial_full_ms": initial_full_ms,
            "latency": metrics,
            "latency_by_scenario": by_scenario,
            "latency_samples_ms": samples_by_scenario,
            "delivery_batches": delivery_batches,
            "empty_views": empty_views,
            "completeness": {
                "expected_markers": len(expected), "seen_markers": len(seen), "missing": missing,
                "duplicates": duplicates, "empty_deltas": empty_deltas,
                "remove_disappeared": remove_disappeared, "replace_disappeared": replace_disappeared,
                "reorder_identity_changes": reorder_identity_changes,
            },
            "passed": passed,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({
            "ok": passed, "evidence": str(args.output), "initial_full_ms": initial_full_ms,
            "latency": metrics, "latency_by_scenario": by_scenario, "missing": len(missing),
        }))
        if not passed:
            raise SystemExit(1)
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
