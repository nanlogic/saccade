#!/usr/bin/env python3
"""Run real-Chrome software actionability and replacement loops."""

from __future__ import annotations

import argparse
import json
import time
import urllib.parse
from pathlib import Path
from typing import Any

from dev_probe import wait_for_mcp


SCENARIOS = ("animation", "overlay", "delayed_enable", "replacement", "continuous_reflex")


def tool(mcp: Any, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    response = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=timeout,
    )
    return response["structuredContent"]


def target_view(mcp: Any, tab_id: str, scenario: str) -> tuple[dict[str, Any], dict[str, Any]]:
    role = "reflex_target" if scenario == "continuous_reflex" else "button"
    view = tool(mcp, "truth.read", {
        "tab_id": tab_id,
        "timeout_ms": 5_000,
        "query": {
            "text": "Target action",
            "roles": [role],
            "frame_scope": "root",
            "min_objects": 1,
            "max_objects": 4,
        },
    })
    matches = [item for item in view.get("objects", [])
               if item.get("role") == role and item.get("name") == "Target action"]
    if len(matches) != 1:
        observed = [(item.get("role"), item.get("name")) for item in view.get("objects", [])]
        raise RuntimeError(
            f"{scenario}: expected one {role} target, got {len(matches)}; observed={observed}"
        )
    return view, matches[0]


def act(mcp: Any, tab_id: str, view: dict[str, Any], target: dict[str, Any]) -> dict[str, Any]:
    return tool(mcp, "act", {
        "tab_id": tab_id,
        "document_id": view["document_id"],
        "basis_revision": view["revision"],
        "object_id": target["object_id"],
        "operation": "click",
        "timeout_ms": 3_000,
    })


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--iterations", type=int, default=100)
    parser.add_argument("--scenario", action="append", choices=SCENARIOS)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 1000:
        raise SystemExit("iterations must be between 1 and 1000")

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    report: dict[str, Any] = {
        "schema": "saccade.actionability-wait/1",
        "iterations_per_scenario": args.iterations,
        "selected_scenarios": args.scenario or list(SCENARIOS),
        "scenarios": {},
    }
    started = time.monotonic()
    try:
        capabilities = tool(mcp, "system.capabilities", {})
        report["browser"] = capabilities.get("attached_browser")
        report["extension_candidate"] = capabilities.get("extension_candidate")
        for scenario in args.scenario or SCENARIOS:
            waits: list[int] = []
            stale = 0
            recovered = 0
            for iteration in range(args.iterations):
                separator = "&" if "?" in args.url else "?"
                url = f"{args.url}{separator}{urllib.parse.urlencode({'scenario': scenario, 'run': iteration})}"
                tab_id: str | None = None
                try:
                    tab_id = str(tool(mcp, "tabs.open", {"url": url, "active": True})["tab_id"])
                    view, target = target_view(mcp, tab_id, scenario)
                    try:
                        result = act(mcp, tab_id, view, target)
                    except Exception as error:
                        raise RuntimeError(
                            f"{scenario}[{iteration}] initial action failed: {error}"
                        ) from error
                    if scenario == "replacement" and result.get("verified") is not True:
                        code = str(result.get("failure_code") or "")
                        if "stale" not in code and result.get("retry_safe") is not True:
                            raise RuntimeError(f"replacement did not fail stale: {result}")
                        stale += 1
                        fresh_view, fresh_target = target_view(mcp, tab_id, scenario)
                        result = act(mcp, tab_id, fresh_view, fresh_target)
                        recovered += 1
                    if result.get("verified") is not True:
                        raise RuntimeError(f"{scenario} action was not verified: {result}")
                    waits.append(int(result.get("local_wait_ms") or 0))
                finally:
                    if tab_id is not None:
                        tool(mcp, "tabs.close", {"tab_id": tab_id})
            if scenario not in {"replacement", "continuous_reflex"} and any(wait <= 0 for wait in waits):
                raise RuntimeError(f"{scenario} did not prove a local wait on every run")
            report["scenarios"][scenario] = {
                "passed": len(waits),
                "stale": stale,
                "replacement_recoveries": recovered,
                "local_wait_ms": {
                    "min": min(waits),
                    "max": max(waits),
                    "mean": round(sum(waits) / len(waits), 3),
                },
            }
    finally:
        mcp.close()
    report["duration_ms"] = round((time.monotonic() - started) * 1000, 3)
    report["passed"] = all(value["passed"] == args.iterations for value in report["scenarios"].values())
    encoded = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    print(encoded, end="")


if __name__ == "__main__":
    main()
