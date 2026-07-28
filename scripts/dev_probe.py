#!/usr/bin/env python3
"""Exercise Saccade through its MCP stdio route and save value-free evidence."""

from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import time
from pathlib import Path
from typing import Any


TEXT_SENTINEL = "SACCADE-DEV-INPUT-SENTINEL"


class Mcp:
    def __init__(self, runtime: Path, runtime_dir: Path) -> None:
        environment = os.environ.copy()
        environment["SACCADE_RUNTIME_DIR"] = str(runtime_dir)
        self.process = subprocess.Popen(
            [str(runtime), "mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=environment,
        )
        self.next_id = 1
        try:
            self.initialize = self.rpc("initialize", {})
        except Exception:
            self.close()
            raise

    def rpc(self, method: str, params: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
        request_id = self.next_id
        self.next_id += 1
        assert self.process.stdin is not None
        assert self.process.stdout is not None
        self.process.stdin.write(
            json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
            + "\n"
        )
        self.process.stdin.flush()
        ready, _, _ = select.select([self.process.stdout], [], [], timeout)
        if not ready:
            raise RuntimeError(f"MCP timed out during {method}")
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr.strip()}")
        response = json.loads(line)
        if response.get("id") != request_id:
            raise RuntimeError(f"MCP returned the wrong response id during {method}")
        if "error" in response:
            raise RuntimeError(response["error"].get("message", str(response["error"])))
        return response["result"]

    def tool(self, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
        result = self.rpc(
            "tools/call",
            {"name": f"saccade.{name}", "arguments": arguments},
            timeout=timeout,
        )
        return result["structuredContent"]

    def close(self) -> None:
        if self.process.stdin:
            try:
                self.process.stdin.close()
            except BrokenPipeError:
                pass
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()


def wait_for_mcp(runtime: Path, runtime_dir: Path, timeout: float = 30.0) -> Mcp:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            return Mcp(runtime, runtime_dir)
        except Exception as error:  # noqa: BLE001
            last_error = error
            time.sleep(0.25)
    raise RuntimeError(f"Saccade MCP did not become ready: {last_error}")


def wait_observation(mcp: Mcp, tab_id: str, timeout: float = 20.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            observation = mcp.tool("web.observe", {"tab_id": tab_id})
            if observation.get("objects"):
                return observation
        except Exception as error:  # noqa: BLE001
            last_error = error
        time.sleep(0.1)
    raise RuntimeError(f"fixture observation did not arrive: {last_error}")


def named(observation: dict[str, Any], role: str, name: str) -> dict[str, Any]:
    for item in observation["objects"]:
        if item.get("role") == role and item.get("name") == name:
            return item
    raise RuntimeError(f"observation has no {role} named {name!r}")


def stable_observation(mcp: Mcp, tab_id: str, timeout: float = 5.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    previous = mcp.tool("web.observe", {"tab_id": tab_id})
    while time.monotonic() < deadline:
        time.sleep(0.25)
        current = mcp.tool("web.observe", {"tab_id": tab_id})
        if (
            current["document_id"] == previous["document_id"]
            and current["revision"] == previous["revision"]
        ):
            return current
        previous = current
    raise RuntimeError("observation did not reach a stable revision")


def act(
    mcp: Mcp,
    observation: dict[str, Any],
    role: str,
    name: str,
    operation: str,
    payload_for: Any,
) -> tuple[dict[str, Any], dict[str, Any]]:
    last_error: Exception | None = None
    for _attempt in range(4):
        observation = stable_observation(mcp, observation["tab_id"])
        target = named(observation, role, name)
        request = {
            "browser_instance_id": observation["browser_instance_id"],
            "tab_id": observation["tab_id"],
            "document_id": observation["document_id"],
            "basis_revision": observation["revision"],
            "action_token": target["action_token"],
            "operation": operation,
            "payload": payload_for(observation),
        }
        try:
            receipt = mcp.tool("web.act", request, timeout=40.0)
            break
        except Exception as error:
            last_error = error
            if "stale action basis" not in str(error) and "not current" not in str(error):
                raise
            observation = stable_observation(mcp, observation["tab_id"])
    else:
        raise RuntimeError(f"{operation} stayed stale after fresh observations: {last_error}")
    if receipt.get("dispatch_status") != "accepted_by_os":
        raise RuntimeError(f"{operation} native dispatch failed: {receipt.get('dispatch_status')}")
    if receipt.get("postcondition") != "verified":
        raise RuntimeError(f"{operation} postcondition failed: {receipt.get('postcondition')}")
    return receipt, receipt["post_action_observation"]


def open_fixture(mcp: Mcp, url: str) -> dict[str, Any]:
    opened = mcp.tool("tabs.open", {"url": url, "active": True})
    return wait_observation(mcp, opened["tab_id"])


def controls(mcp: Mcp, url: str, browser: str) -> dict[str, Any]:
    capabilities = mcp.tool("system.capabilities", {})
    observation = open_fixture(mcp, url)
    initial = observation
    receipts: list[dict[str, Any]] = []

    button_basis = observation
    button_target = named(observation, "button", "Save")
    receipt, observation = act(mcp, observation, "button", "Save", "click", lambda _: {"kind": "none"})
    receipts.append(receipt)
    stale_request = {
        "browser_instance_id": button_basis["browser_instance_id"],
        "tab_id": button_basis["tab_id"],
        "document_id": button_basis["document_id"],
        "basis_revision": button_basis["revision"],
        "action_token": button_target["action_token"],
        "operation": "click",
        "payload": {"kind": "none"},
    }
    try:
        mcp.tool("web.act", stale_request, timeout=10.0)
    except Exception:
        stale_token_rejected = True
    else:
        raise RuntimeError("a consumed action token was accepted twice")
    receipt, observation = act(
        mcp,
        observation,
        "text_field",
        "Email",
        "type",
        lambda _: {"kind": "text", "text": TEXT_SENTINEL},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "checkbox",
        "Remember me",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "select",
        "Color",
        "select",
        lambda current: {
            "kind": "select",
            "option_object_id": named(current, "option", "Blue")["object_id"],
        },
    )
    receipts.append(receipt)
    evidence = {
        "mode": "controls",
        "browser": browser,
        "capabilities": capabilities,
        "initial_observation": initial,
        "receipts": receipts,
        "stale_token_rejected": stale_token_rejected,
    }
    if TEXT_SENTINEL in json.dumps(evidence):
        raise RuntimeError("textfield contents leaked into evidence")
    return evidence


def profile(mcp: Mcp, url: str, browser: str) -> dict[str, Any]:
    instructions = mcp.initialize.get("instructions", "")
    if "Saccade profile integration test." not in instructions:
        raise RuntimeError("Profile behavior was not placed in MCP instructions")
    observation = open_fixture(mcp, url)
    if any(item.get("name") == "Save" for item in observation["objects"]):
        raise RuntimeError("Profile-banned Save control reached MCP")
    return {
        "mode": "profile",
        "browser": browser,
        "initialize": mcp.initialize,
        "observation": observation,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["controls", "profile"])
    parser.add_argument("--browser", choices=["chrome", "edge"], required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    try:
        mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
        try:
            evidence = (
                controls(mcp, args.url, args.browser)
                if args.mode == "controls"
                else profile(mcp, args.url, args.browser)
            )
        finally:
            mcp.close()
        result = {"ok": True, **evidence}
    except Exception as error:
        result = {
            "ok": False,
            "mode": args.mode,
            "browser": args.browser,
            "error": str(error).replace(TEXT_SENTINEL, "[textfield content removed]"),
        }
    encoded = json.dumps(result, indent=2)
    if TEXT_SENTINEL in encoded:
        raise RuntimeError("textfield contents leaked into evidence")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(encoded + "\n", encoding="utf-8")
    print(json.dumps({"ok": result["ok"], "mode": args.mode, "evidence": str(args.output)}))
    if not result["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
