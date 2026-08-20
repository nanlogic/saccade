#!/usr/bin/env python3
"""Prove two no-window Saccade open/Truth/close/reconnect cycles on macOS."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path

from dev_probe import open_when_browser_ready, wait_for_mcp, wait_observation


def close_test_windows(application: str) -> None:
    completed = subprocess.run(
        ["/usr/bin/osascript", "-e", f'tell application "{application}" to close every window'],
        capture_output=True, text=True, check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"could not close isolated test windows: {completed.stderr.strip()}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--application", required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    cycles = []
    try:
        for cycle in range(1, 3):
            close_test_windows(args.application)
            time.sleep(0.5)
            started = time.perf_counter()
            opened = open_when_browser_ready(mcp, args.url, timeout=75)
            tab_id = str(opened["tab_id"])
            truth = wait_observation(mcp, tab_id, timeout=20)
            closed = mcp.tool("tabs.close", {"tab_id": tab_id})
            listed = mcp.tool("tabs.list", {}).get("tabs", [])
            absent = all(str(tab.get("tab_id")) != tab_id for tab in listed)
            cycles.append({
                "cycle": cycle,
                "started_without_normal_window": True,
                "tab_id": tab_id,
                "truth_revision": truth["revision"],
                "object_count": len(truth.get("objects", [])),
                "closed": closed.get("closed") is True,
                "absent_after_close": absent,
                "elapsed_ms": round((time.perf_counter() - started) * 1000, 3),
            })
            if not cycles[-1]["closed"] or not absent or not truth.get("objects"):
                raise RuntimeError(f"no-window recovery cycle {cycle} did not close truthfully")
            time.sleep(0.75)
        capabilities = mcp.tool("system.capabilities", {})
    finally:
        mcp.close()
    evidence = {
        "schema": "saccade.no-window-recovery-evidence/1",
        "setup_stimulus": "macOS closes only the isolated test-browser windows",
        "product_route": "Extension -> Native Host -> owner-only IPC -> MCP",
        "cycles": cycles,
        "extension_candidate": capabilities.get("extension_candidate"),
        "extension_connected_after_two_cycles": capabilities.get("extension_connected"),
        "passed": len(cycles) == 2 and all(row["closed"] and row["absent_after_close"] for row in cycles),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": evidence["passed"], "output": str(args.output)}))
    return 0 if evidence["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
