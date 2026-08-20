#!/usr/bin/env python3
"""Prove an unchanged target rebases across unrelated pushed revisions."""

from __future__ import annotations

import argparse
import json
import time
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
    parser.add_argument("--url", required=True)
    parser.add_argument("--wait-ms", type=int, default=750)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    tab_id: str | None = None
    try:
        opened = call(mcp, "tabs.open", {"url": args.url})
        tab_id = str(opened["tab_id"])
        view = call(mcp, "truth.read", {
            "tab_id": tab_id,
            "query": {
                "roles": ["button"],
                "affordances": ["click"],
                "visible_only": False,
                "frame_scope": "root",
                "min_objects": 1,
                "max_objects": 8,
            },
        })
        target = next(
            item for item in view["objects"]
            if "pressed" in item.get("state", {})
        )
        basis_revision = int(view["revision"])
        time.sleep(args.wait_ms / 1000)
        result = call(mcp, "act", {
            "tab_id": tab_id,
            "document_id": view["document_id"],
            "basis_revision": basis_revision,
            "object_id": target["object_id"],
            "operation": "click",
        })
        if result.get("verified") is not True:
            raise RuntimeError("rebased action was not verified")
        if int(result.get("rebased_from_revision", 0)) != basis_revision:
            raise RuntimeError("action did not report its original rebased revision")
        changed_target_rejected = False
        try:
            call(mcp, "act", {
                "tab_id": tab_id,
                "document_id": view["document_id"],
                "basis_revision": basis_revision,
                "object_id": target["object_id"],
                "operation": "click",
            })
        except Exception as error:  # This is the safety boundary under test.
            changed_target_rejected = "target changed after basis_revision" in str(error)
        if not changed_target_rejected:
            raise RuntimeError("a changed target accepted the stale action basis")
        print(json.dumps({
            "schema": "saccade.unrelated-action-rebase/1",
            "passed": True,
            "tab_id": tab_id,
            "basis_revision": basis_revision,
            "prepared_revision": result["basis_revision"],
            "rebased_from_revision": result["rebased_from_revision"],
            "changed_target_rejected": changed_target_rejected,
            "verification": result["verification"],
        }, indent=2))
    finally:
        if tab_id is not None:
            try:
                call(mcp, "tabs.close", {"tab_id": tab_id})
            except Exception:
                pass
        mcp.close()


if __name__ == "__main__":
    main()
