#!/usr/bin/env python3
"""Prove one semantic Truth query keeps a complete page local and returns a bounded working set."""

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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--roles", nargs="+", required=True)
    parser.add_argument("--text")
    parser.add_argument("--text-any", action="append", default=[])
    parser.add_argument("--frame-scope", choices=("root", "all"), default="root")
    parser.add_argument("--min-objects", type=int, default=1)
    parser.add_argument("--max-objects", type=int, default=32)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    tab_id: str | None = None
    try:
        opened = tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        query = {
            "roles": args.roles,
            "visible_only": False,
            "frame_scope": args.frame_scope,
            "min_objects": args.min_objects,
            "max_objects": args.max_objects,
        }
        if args.text:
            query["text"] = args.text
        if args.text_any:
            query["text_any"] = args.text_any
        view = tool(
            mcp,
            "truth.read",
            {
                "tab_id": tab_id,
                "query": query,
            },
        )
        if view.get("mode") != "working_set":
            raise RuntimeError(f"expected working_set, got {view.get('mode')!r}")
        if len(view.get("objects", [])) > args.max_objects:
            raise RuntimeError("working set exceeded its declared object bound")
        print(json.dumps({
            "schema": "saccade.working-set-probe/1",
            "passed": True,
            "tab_id": tab_id,
            "document_id": view["document_id"],
            "revision": view["revision"],
            "response_bytes": len(json.dumps(view, separators=(",", ":")).encode()),
            "matches": view["match_count"],
            "requested_min_objects": view["requested_min_objects"],
            "settled": view["settled"],
            "returned": len(view["objects"]),
            "truncated": view["truncated"],
            "frames": view["frame_summaries"],
            "roles": sorted({item["role"] for item in view["objects"]}),
            "objects": [
                {
                    "object_id": item["object_id"],
                    "role": item["role"],
                    "name": item.get("name"),
                    "description": item.get("description"),
                    "visibility": item.get("visibility", "visible"),
                    "document_y": item.get("document_bounds", {}).get("y"),
                }
                for item in view["objects"]
            ],
        }, indent=2))
    finally:
        if tab_id is not None:
            tool(mcp, "tabs.close", {"tab_id": tab_id})
        mcp.close()


if __name__ == "__main__":
    main()
