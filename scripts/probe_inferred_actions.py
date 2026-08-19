#!/usr/bin/env python3
"""Prove click, type, and select execute without restating operation."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from dev_probe import wait_for_mcp


def tool(mcp: Any, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    response = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=35.0,
    )
    return response["structuredContent"]


def query(
    mcp: Any, tab_id: str, *, text_any: list[str], roles: list[str], minimum: int
) -> dict[str, Any]:
    return tool(
        mcp,
        "truth.read",
        {
            "tab_id": tab_id,
            "query": {
                "text_any": text_any,
                "roles": roles,
                "visible_only": False,
                "frame_scope": "root",
                "min_objects": minimum,
                "max_objects": 8,
            },
        },
    )


def exact(view: dict[str, Any], role: str, name: str) -> dict[str, Any]:
    matches = [
        item
        for item in view.get("objects", [])
        if item.get("role") == role and item.get("name") == name
    ]
    if len(matches) != 1:
        raise RuntimeError(f"expected one {role} named {name!r}, got {len(matches)}")
    return matches[0]


def act(
    mcp: Any, tab_id: str, view: dict[str, Any], target: dict[str, Any], **payload: Any
) -> dict[str, Any]:
    arguments = {
        "tab_id": tab_id,
        "document_id": view["document_id"],
        "basis_revision": view["revision"],
        "object_id": target["object_id"],
        **payload,
    }
    if "operation" in arguments:
        raise RuntimeError("probe must never send operation")
    result = tool(mcp, "act", arguments)
    if result.get("verified") is not True:
        raise RuntimeError(f"inferred action was not verified: {result}")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    tab_id: str | None = None
    try:
        tab_id = str(tool(mcp, "tabs.open", {"url": args.url})["tab_id"])

        button_view = query(
            mcp,
            tab_id,
            text_any=["Confirm choice"],
            roles=["button"],
            minimum=1,
        )
        clicked = act(
            mcp, tab_id, button_view, exact(button_view, "button", "Confirm choice")
        )

        field_view = query(
            mcp, tab_id, text_any=["Email"], roles=["text_field"], minimum=1
        )
        typed = act(
            mcp,
            tab_id,
            field_view,
            exact(field_view, "text_field", "Email"),
            value="ordinary@example.test",
        )

        select_view = query(
            mcp,
            tab_id,
            text_any=["Color", "Blue"],
            roles=["select", "option"],
            minimum=2,
        )
        selected = act(
            mcp,
            tab_id,
            select_view,
            exact(select_view, "select", "Color"),
            option_object_id=exact(select_view, "option", "Blue")["object_id"],
        )

        print(
            json.dumps(
                {
                    "schema": "saccade.inferred-actions/1",
                    "passed": True,
                    "operation_sent": False,
                    "verified": {
                        "click": clicked["verified"],
                        "type": typed["verified"],
                        "select": selected["verified"],
                    },
                },
                indent=2,
            )
        )
    finally:
        if tab_id is not None:
            try:
                tool(mcp, "tabs.close", {"tab_id": tab_id})
            except Exception:
                pass
        mcp.close()


if __name__ == "__main__":
    main()
