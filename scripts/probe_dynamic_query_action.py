#!/usr/bin/env python3
"""Prove an exact-label query, soft open, option query, and verified selection."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from dev_probe import wait_for_mcp


def tool(mcp: Any, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    response = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=timeout,
    )
    return response["structuredContent"]


def exact_object(view: dict[str, Any], name: str, role: str) -> dict[str, Any]:
    matches = [
        item
        for item in view.get("objects", [])
        if item.get("role") == role and item.get("name", "").casefold() == name.casefold()
    ]
    if len(matches) != 1:
        raise RuntimeError(f"expected one exact {role} named {name!r}, got {len(matches)}")
    return matches[0]


def query(mcp: Any, tab_id: str, text: str, role: str) -> dict[str, Any]:
    return tool(
        mcp,
        "truth.read",
        {
            "tab_id": tab_id,
            "timeout_ms": 10_000,
            "query": {
                "text": text,
                "roles": [role],
                "visible_only": False,
                "frame_scope": "root",
                "min_objects": 1,
                "max_objects": 8,
            },
        },
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--select-name", required=True)
    parser.add_argument("--select-query")
    parser.add_argument("--option-name", required=True)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    tab_id: str | None = None
    try:
        opened = tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        select_view = query(mcp, tab_id, args.select_query or args.select_name, "select")
        select = exact_object(select_view, args.select_name, "select")
        opened_select = tool(
            mcp,
            "act",
            {
                "tab_id": tab_id,
                "document_id": select_view["document_id"],
                "basis_revision": select_view["revision"],
                "object_id": select["object_id"],
                "operation": "click",
            },
        )
        if opened_select.get("verified") is not True:
            raise RuntimeError(f"select opening was not verified: {opened_select}")

        option_view = query(mcp, tab_id, args.option_name, "option")
        option = exact_object(option_view, args.option_name, "option")
        selected = tool(
            mcp,
            "act",
            {
                "tab_id": tab_id,
                "document_id": option_view["document_id"],
                "basis_revision": option_view["revision"],
                "object_id": option["object_id"],
                "operation": "click",
            },
        )
        if selected.get("verified") is not True:
            raise RuntimeError(f"option selection was not verified: {selected}")
        print(
            json.dumps(
                {
                    "schema": "saccade.dynamic-query-action-probe/1",
                    "passed": True,
                    "tab_id": tab_id,
                    "select_query_bytes": len(
                        json.dumps(select_view, separators=(",", ":")).encode()
                    ),
                    "option_query_bytes": len(
                        json.dumps(option_view, separators=(",", ":")).encode()
                    ),
                    "select_verification": opened_select.get("verification"),
                    "option_verification": selected.get("verification"),
                    "ambient_after_select": opened_select.get("ambient_changes_pending", 0),
                    "ambient_after_option": selected.get("ambient_changes_pending", 0),
                },
                indent=2,
            )
        )
    finally:
        if tab_id is not None:
            tool(mcp, "tabs.close", {"tab_id": tab_id})
        mcp.close()


if __name__ == "__main__":
    main()
