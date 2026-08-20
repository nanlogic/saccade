#!/usr/bin/env python3
"""Prove concurrent MCP processes cannot see or operate each other's Agent tabs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from dev_probe import wait_for_mcp


def call(mcp: Any, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    response = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=35.0,
    )
    return response["structuredContent"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url-a", required=True)
    parser.add_argument("--url-b", required=True)
    args = parser.parse_args()

    first = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    second = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    first_tab: str | None = None
    second_tab: str | None = None
    try:
        first_tab = str(call(first, "tabs.open", {"url": args.url_a})["tab_id"])
        second_tab = str(call(second, "tabs.open", {"url": args.url_b})["tab_id"])
        first_ids = {str(tab["tab_id"]) for tab in call(first, "tabs.list", {})["tabs"]}
        second_ids = {str(tab["tab_id"]) for tab in call(second, "tabs.list", {})["tabs"]}
        if first_tab not in first_ids or second_tab in first_ids:
            raise RuntimeError("first MCP session tab projection is not isolated")
        if second_tab not in second_ids or first_tab in second_ids:
            raise RuntimeError("second MCP session tab projection is not isolated")
        cross_read_rejected = False
        try:
            call(first, "truth.read", {"tab_id": second_tab})
        except Exception as error:  # The probe needs the MCP error boundary.
            cross_read_rejected = "outside this MCP session" in str(error)
        if not cross_read_rejected:
            raise RuntimeError("cross-session Truth read was not rejected")
        cross_close_rejected = False
        try:
            call(first, "tabs.close", {"tab_id": second_tab})
        except Exception as error:  # The probe needs the MCP error boundary.
            cross_close_rejected = "outside this MCP session" in str(error)
        if not cross_close_rejected:
            raise RuntimeError("cross-session tab close was not rejected")
        print(json.dumps({
            "schema": "saccade.mcp-session-tab-isolation/1",
            "passed": True,
            "first_tab": first_tab,
            "second_tab": second_tab,
            "first_visible_tabs": sorted(first_ids),
            "second_visible_tabs": sorted(second_ids),
            "cross_read_rejected": cross_read_rejected,
            "cross_close_rejected": cross_close_rejected,
        }, indent=2))
    finally:
        if first_tab is not None:
            try:
                call(first, "tabs.close", {"tab_id": first_tab})
            except Exception:
                pass
        if second_tab is not None:
            try:
                call(second, "tabs.close", {"tab_id": second_tab})
            except Exception:
                pass
        first.close()
        second.close()


if __name__ == "__main__":
    main()
