#!/usr/bin/env python3
"""Matched current Saccade vs official Playwright MCP open-and-read benchmark."""

from __future__ import annotations

import argparse
import json
import math
import os
import select
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any


def compact(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    return round(ordered[min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1)], 3)


class Tokens:
    def __init__(self) -> None:
        try:
            import tiktoken  # type: ignore
        except ImportError as error:
            raise RuntimeError("tiktoken is required on PYTHONPATH") from error
        self.encoding = tiktoken.get_encoding("o200k_base")

    def count(self, value: Any) -> int:
        return len(self.encoding.encode(compact(value)))


class AgentViews:
    def __init__(self) -> None:
        self.tabs: dict[str, dict[str, Any]] = {}

    def apply(self, view: dict[str, Any]) -> dict[str, Any]:
        if view.get("schema") != "saccade.agent-view/1":
            return view
        tab_id = str(view["tab_id"])
        if view["mode"] == "full":
            snapshot = {
                "schema": "saccade.agent-browser-state/1",
                **{key: value for key, value in view.items() if key not in {"schema", "mode"}},
                "changes": [],
            }
            self.tabs[tab_id] = snapshot
            return snapshot
        previous = self.tabs.get(tab_id)
        if previous is None or previous.get("document_id") != view.get("document_id"):
            raise RuntimeError("Saccade delta has no matching full Agent view")
        snapshot = dict(previous)
        objects = {item["object_id"]: dict(item) for item in previous.get("objects", [])}
        for change in view.get("changes", []):
            if change["kind"] == "disappeared":
                objects.pop(change["object_id"], None)
            else:
                item = dict(change["object"])
                objects[item["object_id"]] = item
        for authority in view.get("authorities", []):
            item = objects.get(authority["object_id"])
            if item is not None:
                item["action_token"] = authority["action_token"]
        snapshot.update({
            "revision": view["revision"],
            "viewport_revision": view["viewport_revision"],
            "objects": list(objects.values()),
            "changes": view.get("changes", []),
            "coverage": view["coverage"],
            "limitations": view["limitations"],
            "gap": view["gap"],
        })
        if view.get("frames") is not None:
            snapshot["frames"] = view["frames"]
        self.tabs[tab_id] = snapshot
        return snapshot


class Mcp:
    def __init__(self, command: list[str], environment: dict[str, str]) -> None:
        self.process = subprocess.Popen(
            command,
            cwd=tempfile.gettempdir(),
            env=environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method: str, params: dict[str, Any], timeout: float = 70.0) -> tuple[dict[str, Any], float]:
        request_id = self.next_id
        self.next_id += 1
        assert self.process.stdin is not None and self.process.stdout is not None
        started = time.perf_counter()
        self.process.stdin.write(compact({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n")
        self.process.stdin.flush()
        ready, _, _ = select.select([self.process.stdout], [], [], timeout)
        if not ready:
            raise RuntimeError(f"MCP timed out during {method}")
        line = self.process.stdout.readline()
        elapsed = round((time.perf_counter() - started) * 1000, 3)
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr[-2000:]}")
        response = json.loads(line)
        if response.get("id") != request_id:
            raise RuntimeError(f"MCP returned the wrong response id during {method}")
        return response, elapsed

    def initialize(self) -> dict[str, Any]:
        response, _ = self.request(
            "initialize",
            {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "saccade-playwright-parity", "version": "1"},
            },
        )
        if response.get("error"):
            raise RuntimeError(f"initialize failed: {response['error']}")
        assert self.process.stdin is not None
        self.process.stdin.write(compact({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}) + "\n")
        self.process.stdin.flush()
        return response["result"]

    def tools(self) -> list[dict[str, Any]]:
        response, _ = self.request("tools/list", {})
        if response.get("error"):
            raise RuntimeError(f"tools/list failed: {response['error']}")
        return response["result"]["tools"]

    def tool(self, name: str, arguments: dict[str, Any], timeout: float = 70.0) -> tuple[dict[str, Any], float]:
        response, elapsed = self.request(
            "tools/call", {"name": name, "arguments": arguments}, timeout=timeout
        )
        return response, elapsed

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)


def result_value(response: dict[str, Any]) -> dict[str, Any]:
    if response.get("error"):
        raise RuntimeError(str(response["error"]))
    result = response.get("result") or {}
    value = result.get("structuredContent")
    return value if isinstance(value, dict) else {}


def result_text(response: dict[str, Any]) -> str:
    if response.get("error"):
        raise RuntimeError(str(response["error"]))
    blocks = (response.get("result") or {}).get("content") or []
    return "\n".join(
        str(block.get("text") or "")
        for block in blocks
        if isinstance(block, dict) and block.get("type") == "text"
    )


def summarize(runs: list[dict[str, Any]]) -> dict[str, Any]:
    warm = runs[1:]
    return {
        "iterations": len(runs),
        "cold": runs[0],
        "warm_p50_task_ms": round(statistics.median(run["task_ms"] for run in warm), 3),
        "p95_task_ms": percentile([run["task_ms"] for run in runs], 0.95),
        "median_model_facing_tokens": round(statistics.median(run["model_facing_tokens"] for run in runs), 3),
        "median_model_facing_bytes": round(statistics.median(run["model_facing_bytes"] for run in runs), 3),
        "all_task_ms": [run["task_ms"] for run in runs],
        "all_model_facing_tokens": [run["model_facing_tokens"] for run in runs],
    }


def run_saccade(
    runtime: Path,
    runtime_dir: Path,
    url: str,
    expected_text: str,
    iterations: int,
    tokens: Tokens,
) -> dict[str, Any]:
    environment = os.environ.copy()
    environment["SACCADE_RUNTIME_DIR"] = str(runtime_dir)
    client = Mcp([str(runtime), "mcp"], environment)
    views = AgentViews()
    try:
        initialized = client.initialize()
        tools = client.tools()
        runs = []
        for _ in range(iterations):
            started = time.perf_counter()
            opened_response, open_ms = client.tool("saccade.tabs.open", {"url": url, "active": True})
            tab_id = result_value(opened_response)["tab_id"]
            payloads = [opened_response.get("result") or opened_response.get("error")]
            observe_calls = 0
            observation: dict[str, Any] | None = None
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                observe_calls += 1
                observed_response, _ = client.tool("saccade.web.observe", {"tab_id": tab_id})
                payloads.append(observed_response.get("result") or observed_response.get("error"))
                if not observed_response.get("error"):
                    candidate = views.apply(result_value(observed_response))
                    visible_text = "\n".join(
                        str(item.get("text") or item.get("name") or "")
                        for item in candidate.get("objects", [])
                    )
                    if expected_text.casefold() in visible_text.casefold():
                        observation = candidate
                        break
                time.sleep(0.05)
            if observation is None:
                raise RuntimeError(f"Saccade did not return expected text {expected_text!r}")
            task_ms = round((time.perf_counter() - started) * 1000, 3)
            runs.append(
                {
                    "open_ms": open_ms,
                    "task_ms": task_ms,
                    "observe_calls": observe_calls,
                    "object_count": len(observation.get("objects", [])),
                    "model_facing_bytes": len(compact(payloads).encode()),
                    "model_facing_tokens": tokens.count(payloads),
                }
            )
        return {
            "server": initialized.get("serverInfo"),
            "tool_count": len(tools),
            "tool_schema_bytes": len(compact(tools).encode()),
            "tool_schema_tokens": tokens.count(tools),
            "runs": runs,
            "summary": summarize(runs),
        }
    finally:
        client.close()


PLAYWRIGHT_TEXT_FUNCTION = """() => {
  const root = document.querySelector('article, main, [role="main"]') || document.body;
  return { text: String(root?.innerText || root?.textContent || '').trim().slice(0, 20000) };
}"""


def run_playwright(
    command: list[str],
    url: str,
    expected_text: str,
    iterations: int,
    tokens: Tokens,
) -> dict[str, Any]:
    environment = os.environ.copy()
    client = Mcp(
        command
        + ["--headless", "--browser", "chrome", "--isolated", "--snapshot-mode", "none", "--output-mode", "stdout", "--image-responses", "omit"],
        environment,
    )
    try:
        initialized = client.initialize()
        tools = client.tools()
        runs = []
        for index in range(iterations):
            started = time.perf_counter()
            if index == 0:
                opened_response, open_ms = client.tool("browser_navigate", {"url": url})
            else:
                opened_response, open_ms = client.tool("browser_tabs", {"action": "new", "url": url})
            article_response, read_ms = client.tool(
                "browser_evaluate", {"function": PLAYWRIGHT_TEXT_FUNCTION}
            )
            if expected_text.casefold() not in result_text(article_response).casefold():
                raise RuntimeError(f"Playwright did not return expected text {expected_text!r}")
            payloads = [opened_response.get("result") or opened_response.get("error"), article_response.get("result") or article_response.get("error")]
            runs.append(
                {
                    "open_ms": open_ms,
                    "read_ms": read_ms,
                    "task_ms": round((time.perf_counter() - started) * 1000, 3),
                    "model_facing_bytes": len(compact(payloads).encode()),
                    "model_facing_tokens": tokens.count(payloads),
                }
            )
        return {
            "server": initialized.get("serverInfo"),
            "configuration": "--snapshot-mode none --image-responses omit",
            "tool_count": len(tools),
            "tool_schema_bytes": len(compact(tools).encode()),
            "tool_schema_tokens": tokens.count(tools),
            "runs": runs,
            "summary": summarize(runs),
        }
    finally:
        client.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--url", default="https://example.com")
    parser.add_argument("--expect", default="Example Domain")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--playwright-command", nargs="+", default=["npx", "-y", "@playwright/mcp@0.0.78"])
    args = parser.parse_args()
    if not 2 <= args.iterations <= 10:
        raise SystemExit("--iterations must be between 2 and 10")
    token_counter = Tokens()
    started = time.monotonic()
    try:
        saccade = run_saccade(
            args.runtime.resolve(),
            args.runtime_dir.resolve(),
            args.url,
            args.expect,
            args.iterations,
            token_counter,
        )
        playwright = run_playwright(
            args.playwright_command,
            args.url,
            args.expect,
            args.iterations,
            token_counter,
        )
        saccade_summary = saccade["summary"]
        playwright_summary = playwright["summary"]
        saccade_cold = saccade["tool_schema_tokens"] + saccade_summary["cold"]["model_facing_tokens"]
        playwright_cold = playwright["tool_schema_tokens"] + playwright_summary["cold"]["model_facing_tokens"]
        report = {
            "schema": "saccade-playwright-parity/2",
            "verdict": "PASS" if saccade_summary["warm_p50_task_ms"] < playwright_summary["warm_p50_task_ms"] and saccade_summary["median_model_facing_tokens"] < playwright_summary["median_model_facing_tokens"] and saccade_cold < playwright_cold else "MIXED",
            "url": args.url,
            "expected_text": args.expect,
            "iterations": args.iterations,
            "tokenizer": "o200k_base",
            "scope": "Complete MCP tool results and all advertised tool schemas; Playwright uses snapshot-mode none and browser_evaluate.",
            "saccade": saccade,
            "playwright": playwright,
            "comparison": {
                "warm_speed_ratio": round(saccade_summary["warm_p50_task_ms"] / playwright_summary["warm_p50_task_ms"], 3),
                "marginal_token_ratio": round(saccade_summary["median_model_facing_tokens"] / playwright_summary["median_model_facing_tokens"], 3),
                "cold_context_token_ratio": round(saccade_cold / playwright_cold, 3),
                "saccade_cold_context_tokens": saccade_cold,
                "playwright_cold_context_tokens": playwright_cold,
            },
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:  # noqa: BLE001
        report = {
            "schema": "saccade-playwright-parity/2",
            "verdict": "ERROR",
            "error": str(error),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"verdict": report["verdict"], "output": str(args.output)}))
    return 0 if report["verdict"] != "ERROR" else 1


if __name__ == "__main__":
    raise SystemExit(main())
