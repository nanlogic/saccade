#!/usr/bin/env python3
"""Prove a child of an Agent-owned tab is created but remains Agent Off."""

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


def named_view(
    mcp: Any, tab_id: str, *, name: str, role: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    view = call(
        mcp,
        "truth.read",
        {
            "tab_id": tab_id,
            "timeout_ms": 10_000,
            "query": {
                "text": name,
                "roles": [role],
                "visible_only": False,
                "frame_scope": "root",
                "min_objects": 1,
                "max_objects": 4,
            },
        },
    )
    matches = [
        item
        for item in view.get("objects", [])
        if item.get("role") == role and item.get("name") == name
    ]
    if len(matches) != 1:
        raise RuntimeError(f"expected one {role} named {name!r}, got {len(matches)}")
    return view, matches[0]


def click(mcp: Any, tab_id: str, view: dict[str, Any], target: dict[str, Any]) -> dict[str, Any]:
    return call(
        mcp,
        "act",
        {
            "tab_id": tab_id,
            "document_id": view["document_id"],
            "basis_revision": view["revision"],
            "object_id": target["object_id"],
            "operation": "click",
        },
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    parent_tab: str | None = None
    child_open = False
    try:
        parent_tab = str(call(mcp, "tabs.open", {"url": args.url})["tab_id"])
        initial, open_button = named_view(
            mcp, parent_tab, name="Open child tab", role="button"
        )
        opened = click(mcp, parent_tab, initial, open_button)
        if opened.get("verified") is not True:
            raise RuntimeError(f"child open dispatch was not verified: {opened}")
        current, button = named_view(
            mcp, parent_tab, name="Close child tab", role="button"
        )
        child_open = True
        visible_tabs = call(mcp, "tabs.list", {})["tabs"]
        visible_ids = [str(tab["tab_id"]) for tab in visible_tabs]
        if visible_ids != [parent_tab]:
            raise RuntimeError(
                f"child tab inherited Agent On: expected only {parent_tab}, got {visible_ids}"
            )

        closed = click(mcp, parent_tab, current, button)
        if closed.get("verified") is not True:
            raise RuntimeError(f"child close was not verified: {closed}")
        child_open = False
        print(
            json.dumps(
                {
                    "schema": "saccade.opener-isolation/1",
                    "passed": True,
                    "parent_tab": parent_tab,
                    "visible_agent_tabs_after_child_open": visible_ids,
                    "open_result": opened.get("result"),
                    "close_verified": closed.get("verified"),
                    "child_authorized": False,
                },
                indent=2,
            )
        )
    finally:
        if child_open and parent_tab is not None:
            try:
                current, button = named_view(
                    mcp, parent_tab, name="Close child tab", role="button"
                )
                click(mcp, parent_tab, current, button)
            except Exception:
                pass
        if parent_tab is not None:
            try:
                call(mcp, "tabs.close", {"tab_id": parent_tab})
            except Exception:
                pass
        mcp.close()


if __name__ == "__main__":
    main()
