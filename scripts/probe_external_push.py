#!/usr/bin/env python3
"""Observe browser-pushed deltas from a public page without taking actions."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any

from dev_probe import Mcp, wait_for_mcp


def raw_tool(mcp: Mcp, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    result = mcp.rpc(
        "tools/call", {"name": f"saccade.{name}", "arguments": arguments}, timeout=timeout
    )
    return result["structuredContent"]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--timeout-ms", type=int, default=10000)
    parser.add_argument("--stable-timeouts", type=int, default=1, help="consecutive empty waits before stopping")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not 1 <= args.stable_timeouts <= 100:
        parser.error("--stable-timeouts must be between 1 and 100")

    mcp = wait_for_mcp(args.runtime, args.runtime_dir)
    try:
        opened = raw_tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        initial = raw_tool(mcp, "truth.read", {"tab_id": tab_id})
        revision = int(initial["revision"])
        pushes = []
        empty_waits = 0
        stopped = "sample_limit"
        while len(pushes) < args.samples:
            started = time.monotonic()
            try:
                view = raw_tool(
                    mcp,
                    "truth.read",
                    {"tab_id": tab_id, "after_revision": revision, "timeout_ms": args.timeout_ms},
                    timeout=args.timeout_ms / 1000 + 2,
                )
            except RuntimeError as error:
                if "no observation after revision" not in str(error):
                    raise
                empty_waits += 1
                if empty_waits >= args.stable_timeouts:
                    stopped = "page_stable_timeout"
                    break
                continue
            empty_waits = 0
            pushes.append({
                "wait_ms": round((time.monotonic() - started) * 1000, 3),
                "mode": view.get("mode"),
                "revision": view.get("revision"),
                "changes": view.get("changes", []),
                "objects": [
                    {key: item[key] for key in ("object_id", "role", "name", "text", "state") if key in item}
                    for item in view.get("objects", [])
                ],
                "authority_refresh_count": len(view.get("authorities", [])),
                "gap": view.get("gap"),
            })
            revision = int(view["revision"])
        evidence = {
            "schema": "saccade.external-push-evidence/1",
            "actions_taken": 0,
            "polling": False,
            "extension_observe_request_during_wait": False,
            "stopped": stopped,
            "initial": {
                "mode": initial.get("mode"),
                "revision": initial.get("revision"),
                "objects": [
                    {key: item[key] for key in ("object_id", "role", "name", "text", "state") if key in item}
                    for item in initial.get("objects", [])
                ],
                "limitations": initial.get("limitations", []),
            },
            "pushes": pushes,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({"ok": True, "evidence": str(args.output), "pushes": len(pushes)}))
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
