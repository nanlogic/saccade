#!/usr/bin/env python3
"""Prove page-driven Extension pushes without polling or browser-side test APIs."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any

from dev_probe import Mcp, wait_for_mcp


def raw_tool(mcp: Mcp, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    result = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=timeout,
    )
    return result["structuredContent"]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime, args.runtime_dir)
    try:
        opened = raw_tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        initial = raw_tool(mcp, "truth.read", {"tab_id": tab_id})
        if initial.get("mode") != "full":
            raise RuntimeError("first Agent view was not full")

        started = time.monotonic()
        revision = int(initial["revision"])
        passive_views = []
        status_view = None
        modal_object_id = None
        modal_appeared = False
        modal_disappeared = False
        for _ in range(8):
            passive = raw_tool(
                mcp,
                "truth.read",
                {"tab_id": tab_id, "after_revision": revision, "timeout_ms": 5000},
                timeout=7.0,
            )
            revision = int(passive["revision"])
            passive_views.append(passive)
            for change in passive.get("changes", []):
                observed = change.get("object", {})
                if (
                    change.get("kind") == "appeared"
                    and observed.get("role") == "heading"
                    and observed.get("text") == "Passive modal"
                    and observed.get("state", {}).get("modal") == "true"
                ):
                    modal_object_id = observed.get("object_id")
                    modal_appeared = True
                if change.get("kind") == "disappeared" and change.get("object_id") == modal_object_id:
                    modal_disappeared = True
                if change.get("kind") == "updated" and observed.get("role") == "status":
                    status_view = passive
            if status_view is not None and modal_appeared and modal_disappeared:
                break
        passive_wait_ms = round((time.monotonic() - started) * 1000, 3)
        if status_view is None:
            raise RuntimeError(
                "page-driven status mutation did not arrive as an Extension delta: "
                + json.dumps(passive_views, ensure_ascii=False)
            )
        if not modal_appeared or not modal_disappeared:
            raise RuntimeError(
                "aria-modal lifecycle did not arrive as appeared/disappeared semantic deltas: "
                + json.dumps(passive_views, ensure_ascii=False)
            )

        evidence = {
            "schema": "saccade.push-delta-evidence/1",
            "route": "page mutation -> Extension compiler -> Native Host -> Runtime wait -> MCP Agent delta",
            "polling": False,
            "extension_observe_request_during_wait": False,
            "tab_id": tab_id,
            "initial": {
                "mode": initial["mode"],
                "revision": initial["revision"],
                "object_count": len(initial.get("objects", [])),
            },
            "passive_push": {
                "mode": status_view["mode"],
                "revision": status_view["revision"],
                "wait_ms": passive_wait_ms,
                "changes": status_view.get("changes", []),
                "view_count": len(passive_views),
                "modal": {
                    "object_id": modal_object_id,
                    "appeared_with_modal_true": modal_appeared,
                    "disappeared": modal_disappeared,
                },
            },
            "execution_owner": "agent_client",
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({"ok": True, "evidence": str(args.output), "passive_wait_ms": passive_wait_ms}))
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
