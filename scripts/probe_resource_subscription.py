#!/usr/bin/env python3
"""Prove unsolicited MCP Resource notification from a browser-pushed delta."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

from dev_probe import wait_for_mcp


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    mcp = wait_for_mcp(args.runtime, args.runtime_dir)
    try:
        opened = mcp.rpc("tools/call", {"name": "saccade.tabs.open", "arguments": {"url": args.url, "active": True}})["structuredContent"]
        tab_id = str(opened["tab_id"])
        uri = f"saccade://tabs/{tab_id}/truth"
        initial = mcp.rpc("resources/read", {"uri": uri})
        initial_view = json.loads(initial["contents"][0]["text"])
        mcp.rpc("resources/subscribe", {"uri": uri})
        started = time.monotonic()
        notification = mcp.wait_notification("notifications/resources/updated", timeout=6.0)
        notified_ms = round((time.monotonic() - started) * 1000, 3)
        updated = mcp.rpc("resources/read", {"uri": uri})
        updated_view = json.loads(updated["contents"][0]["text"])
        if notification.get("params", {}).get("uri") != uri:
            raise RuntimeError("resource notification URI did not match the subscription")
        if updated_view.get("mode") != "delta" or not updated_view.get("changes"):
            raise RuntimeError("notified resource did not contain a semantic delta")
        evidence = {
            "schema": "saccade.resource-subscription-evidence/1",
            "agent_requests_between_subscribe_and_notification": 0,
            "notification_wait_ms": notified_ms,
            "notification": notification,
            "initial": {"mode": initial_view.get("mode"), "revision": initial_view.get("revision")},
            "updated": updated_view,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2, ensure_ascii=False) + "\n")
        print(json.dumps({"ok": True, "evidence": str(args.output), "notification_wait_ms": notified_ms}))
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
