#!/usr/bin/env python3
"""Minimal standard-library host for the Saccade MCP contract v1."""
import json
import os
import shlex
import subprocess
import time
from typing import Any


class McpClient:
    def __init__(self, child: subprocess.Popen[str]) -> None:
        self.child = child
        self.next_id = 0

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self.next_id += 1
        assert self.child.stdin and self.child.stdout
        self.child.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params or {}}) + "\n")
        self.child.stdin.flush()
        while True:
            response = json.loads(self.child.stdout.readline())
            if response.get("id") != self.next_id:
                continue
            if "error" in response:
                data = response["error"].get("data", {})
                raise RuntimeError(f"{data.get('saccade_code', 'MCP_ERROR')}: {data.get('detail', 'request failed')}")
            return response["result"]

    def tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        return self.request("tools/call", {"name": name, "arguments": arguments or {}}).get("structuredContent", {})


def main() -> None:
    grant_path = os.environ["SACCADE_GRANT_PATH"]
    lifecycle_only = os.environ.get("SACCADE_LIFECYCLE_ONLY") == "1"
    assignments = json.loads(os.environ.get("SACCADE_ASSIGNMENTS_JSON", "{}"))
    command = os.environ.get("SACCADE_MCP_COMMAND")
    argv = shlex.split(command) if command else ["cargo", "run", "-q", "-p", "saccade-mcp", "--", "serve-stdio"]
    child = subprocess.Popen(argv, text=True, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    mcp = McpClient(child)
    tab_id: int | None = None
    try:
        mcp.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "vendor-host-example", "version": "0.1"}})
        contract = mcp.tool("saccade.system.capabilities")["saccade"]["contract_version"]
        if not contract.startswith("1."):
            raise RuntimeError(f"unsupported Saccade contract {contract}")
        grant = mcp.tool("saccade.tabs.grant_current", {"grant_path": grant_path, "reason": "user requested ordinary-field draft assistance", "policy": {"explicit_user_grant": True, "local_dev_only": True}})
        tab_id = grant["tab"]["tab_id"]
        if lifecycle_only:
            target = os.environ["SACCADE_NAVIGATE_URL"]
            navigation = mcp.tool("saccade.browser.navigate", {
                "tab_id": tab_id,
                "action": "navigate",
                "url": target,
                "policy": {"same_webview_only": True, "user_granted_tab_only": True},
            })
            time.sleep(0.25)
            status = mcp.tool("saccade.browser.navigate", {
                "tab_id": tab_id,
                "action": "status",
                "policy": {"same_webview_only": True, "user_granted_tab_only": True},
            })
            paused = mcp.tool("saccade.tabs.pause_agent", {"tab_id": tab_id})
            print(json.dumps({
                "contract": contract,
                "attached": grant.get("same_webview_attached"),
                "capabilities": grant.get("same_webview_capabilities"),
                "navigate_status": navigation.get("status"),
                "current_url": status.get("url"),
                "pause_status": paused.get("status"),
            }, indent=2))
            return
        inventory = mcp.tool("saccade.web.form_inventory", {"tab_id": tab_id, "mode": "actionable"})
        policy = {"block_sensitive": True, "preserve_existing": True, "no_submit": True}
        plan = mcp.tool("saccade.web.form_compile_plan", {"tab_id": tab_id, "basis_page_revision": inventory["page_revision"], "assignments": assignments, "policy": policy})
        result = mcp.tool("saccade.web.form_execute_plan", {"tab_id": tab_id, "basis_page_revision": inventory["page_revision"], "expected_plan_id": plan["plan_id"], "assignments": assignments, "policy": policy})
        print(json.dumps({key: result.get(key) for key in ("status", "summary", "replay_path")}, indent=2))
        mcp.tool("saccade.tabs.pause_agent", {"tab_id": tab_id})
    finally:
        if tab_id is not None:
            try:
                mcp.tool("saccade.tabs.close", {"tab_id": tab_id})
            except Exception:
                pass
        child.kill()


if __name__ == "__main__":
    main()
