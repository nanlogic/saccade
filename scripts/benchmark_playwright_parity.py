#!/usr/bin/env python3
"""Matched Saccade vs official Playwright MCP latency and context benchmark."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import math
import os
import pathlib
import statistics
import struct
import subprocess
import tempfile
import time
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--saccade-app", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--url", default="https://example.com")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument(
        "--playwright-command", default="/opt/homebrew/bin/npx"
    )
    return parser.parse_args()


def compact_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = min(len(ordered) - 1, math.ceil(fraction * len(ordered)) - 1)
    return round(ordered[index], 3)


class TokenCounter:
    def __init__(self) -> None:
        try:
            import tiktoken  # type: ignore
        except ImportError as error:
            raise RuntimeError(
                "tiktoken is required; install it into a temporary PYTHONPATH"
            ) from error
        self.encoding = tiktoken.get_encoding("o200k_base")

    def count_text(self, text: str) -> int:
        return len(self.encoding.encode(text))

    def count_json(self, value: Any) -> int:
        return self.count_text(compact_json(value))


class McpClient:
    def __init__(self, command: list[str], env: dict[str, str]) -> None:
        self.process = subprocess.Popen(
            command,
            cwd="/private/tmp",
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method: str, params: dict[str, Any]) -> tuple[dict[str, Any], float]:
        assert self.process.stdin is not None and self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        request = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        started = time.perf_counter()
        self.process.stdin.write(compact_json(request) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        elapsed_ms = (time.perf_counter() - started) * 1000
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr[-2000:]}")
        response = json.loads(line)
        if response.get("error"):
            raise RuntimeError(f"MCP {method} failed: {response['error']}")
        return response.get("result", {}), round(elapsed_ms, 3)

    def notify(self, method: str, params: dict[str, Any]) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(
            compact_json({"jsonrpc": "2.0", "method": method, "params": params}) + "\n"
        )
        self.process.stdin.flush()

    def initialize(self) -> dict[str, Any]:
        result, _ = self.request(
            "initialize",
            {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "saccade-playwright-parity", "version": "1"},
            },
        )
        self.notify("notifications/initialized", {})
        return result

    def tools(self) -> list[dict[str, Any]]:
        result, _ = self.request("tools/list", {})
        return result.get("tools", [])

    def tool(
        self, name: str, arguments: dict[str, Any]
    ) -> tuple[dict[str, Any], float]:
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)


def structured(result: dict[str, Any]) -> dict[str, Any]:
    value = result.get("structuredContent")
    if isinstance(value, dict):
        return value
    return {}


def result_text(result: dict[str, Any]) -> str:
    blocks = result.get("content") or []
    return "\n".join(
        str(block.get("text") or "")
        for block in blocks
        if isinstance(block, dict) and block.get("type") == "text"
    )


def png_dimensions(data: bytes) -> tuple[int, int] | None:
    if len(data) >= 24 and data.startswith(b"\x89PNG\r\n\x1a\n"):
        return struct.unpack(">II", data[16:24])
    return None


def image_metrics(result: dict[str, Any]) -> dict[str, Any]:
    for block in result.get("content") or []:
        if not isinstance(block, dict) or block.get("type") != "image":
            continue
        data = base64.b64decode(block.get("data") or "")
        dimensions = png_dimensions(data)
        if not dimensions:
            return {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        width, height = dimensions
        patches = math.ceil(width / 32) * math.ceil(height / 32)
        return {
            "bytes": len(data),
            "width": width,
            "height": height,
            "gpt_5_6_original_image_tokens": patches,
            "sha256": hashlib.sha256(data).hexdigest(),
        }
    return {"bytes": 0, "gpt_5_6_original_image_tokens": 0}


def image_result_text_metadata(result: dict[str, Any]) -> dict[str, Any]:
    """Return the model-facing result envelope without charging PNG base64 as text."""
    sanitized = json.loads(json.dumps(result))
    for block in sanitized.get("content") or []:
        if isinstance(block, dict) and block.get("type") == "image":
            block.pop("data", None)
    return sanitized


def summarize_runs(runs: list[dict[str, Any]]) -> dict[str, Any]:
    open_ms = [float(run["open_ms"]) for run in runs]
    read_ms = [float(run["read_ms"]) for run in runs]
    task_ms = [float(run["task_ms"]) for run in runs]
    task_tokens = [int(run["model_facing_tokens"]) for run in runs]
    warm = runs[1:]
    return {
        "iterations": len(runs),
        "cold": runs[0],
        "warm_p50_open_ms": round(statistics.median([run["open_ms"] for run in warm]), 3)
        if warm
        else None,
        "warm_p50_read_ms": round(statistics.median([run["read_ms"] for run in warm]), 3)
        if warm
        else None,
        "warm_p50_task_ms": round(statistics.median([run["task_ms"] for run in warm]), 3)
        if warm
        else None,
        "p95_task_ms": percentile(task_ms, 0.95),
        "median_model_facing_tokens": round(statistics.median(task_tokens), 3),
        "median_model_facing_bytes": round(
            statistics.median([run["model_facing_bytes"] for run in runs]), 3
        ),
        "all_open_ms": open_ms,
        "all_read_ms": read_ms,
        "all_task_ms": task_ms,
        "all_model_facing_tokens": task_tokens,
    }


def run_saccade(
    app: pathlib.Path, url: str, iterations: int, counter: TokenCounter
) -> dict[str, Any]:
    env = os.environ.copy()
    env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    env.setdefault("LANG", "en_US.UTF-8")
    env.setdefault("TMPDIR", tempfile.gettempdir())
    launcher = app / "Contents" / "MacOS" / "saccade-current-tab-mcp"
    client = McpClient([str(launcher)], env)
    tab_ids: list[int] = []
    try:
        initialized = client.initialize()
        tools = client.tools()
        runs: list[dict[str, Any]] = []
        for _ in range(iterations):
            opened_result, open_ms = client.tool("saccade.tabs.open_agent", {"url": url})
            opened = structured(opened_result)
            tab = opened.get("tab") or {}
            tab_id = int(tab["tab_id"])
            tab_ids.append(tab_id)
            revision = int(tab["page_revision"])
            article_result, read_ms = client.tool(
                "saccade.web.article_text",
                {"tab_id": tab_id, "basis_page_revision": revision},
            )
            article = structured(article_result)
            if "Example Domain" not in str(article.get("text") or ""):
                raise AssertionError(f"Saccade returned the wrong article: {article}")
            model_payload = [opened_result, article_result]
            runs.append(
                {
                    "open_ms": open_ms,
                    "read_ms": read_ms,
                    "task_ms": round(open_ms + read_ms, 3),
                    "model_facing_bytes": len(compact_json(model_payload).encode()),
                    "model_facing_tokens": counter.count_json(model_payload),
                    "article_bytes": len(compact_json(article).encode()),
                    "article_tokens": counter.count_json(article),
                    "text_chars": len(str(article.get("text") or "")),
                }
            )
        for tab_id in reversed(tab_ids):
            try:
                client.tool("saccade.tabs.close", {"tab_id": tab_id})
            except Exception:
                pass
        return {
            "server": initialized.get("serverInfo"),
            "tool_count": len(tools),
            "tool_schema_bytes": len(compact_json(tools).encode()),
            "tool_schema_tokens": counter.count_json(tools),
            "runs": runs,
            "summary": summarize_runs(runs),
        }
    finally:
        client.close()


def playwright_client(command: str, snapshot_mode: str) -> McpClient:
    env = os.environ.copy()
    env["PATH"] = "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    args = [
        command,
        "-y",
        "@playwright/mcp@latest",
        "--headless",
        "--browser",
        "chrome",
        "--isolated",
        "--snapshot-mode",
        snapshot_mode,
        "--output-mode",
        "stdout",
    ]
    return McpClient(args, env)


PLAYWRIGHT_TEXT_FUNCTION = """() => {
  const root = document.querySelector('article, main, [role="main"]') || document.body;
  return { text: String(root?.innerText || root?.textContent || '').trim().slice(0, 20000) };
}"""


def run_playwright(
    command: str,
    url: str,
    iterations: int,
    counter: TokenCounter,
    snapshot_mode: str,
    take_screenshot: bool,
) -> dict[str, Any]:
    client = playwright_client(command, snapshot_mode)
    try:
        initialized = client.initialize()
        tools = client.tools()
        names = {tool.get("name") for tool in tools}
        required = {"browser_navigate", "browser_evaluate", "browser_take_screenshot"}
        if not required.issubset(names):
            raise AssertionError(f"Playwright MCP missing tools: {required - names}")
        runs: list[dict[str, Any]] = []
        for index in range(iterations):
            if index == 0:
                opened_result, open_ms = client.tool("browser_navigate", {"url": url})
            else:
                opened_result, open_ms = client.tool(
                    "browser_tabs", {"action": "new", "url": url}
                )
            article_result, read_ms = client.tool(
                "browser_evaluate", {"function": PLAYWRIGHT_TEXT_FUNCTION}
            )
            article_text = result_text(article_result)
            if "Example Domain" not in article_text:
                raise AssertionError(f"Playwright returned the wrong article: {article_text}")
            model_payload = [opened_result, article_result]
            runs.append(
                {
                    "open_ms": open_ms,
                    "read_ms": read_ms,
                    "task_ms": round(open_ms + read_ms, 3),
                    "model_facing_bytes": len(compact_json(model_payload).encode()),
                    "model_facing_tokens": counter.count_json(model_payload),
                    "article_bytes": len(compact_json(article_result).encode()),
                    "article_tokens": counter.count_json(article_result),
                    "text_chars": len(article_text),
                }
            )
        screenshot = None
        if take_screenshot:
            screenshot_result, screenshot_ms = client.tool(
                "browser_take_screenshot",
                {"type": "png", "fullPage": False, "scale": "css"},
            )
            screenshot = image_metrics(screenshot_result)
            screenshot["latency_ms"] = screenshot_ms
            screenshot_metadata = image_result_text_metadata(screenshot_result)
            screenshot["model_facing_text_metadata_tokens"] = counter.count_json(
                screenshot_metadata
            )
            screenshot["estimated_total_model_tokens"] = (
                screenshot["gpt_5_6_original_image_tokens"]
                + screenshot["model_facing_text_metadata_tokens"]
            )
        try:
            client.tool("browser_close", {})
        except Exception:
            pass
        return {
            "server": initialized.get("serverInfo"),
            "snapshot_mode": snapshot_mode,
            "tool_count": len(tools),
            "tool_schema_bytes": len(compact_json(tools).encode()),
            "tool_schema_tokens": counter.count_json(tools),
            "runs": runs,
            "summary": summarize_runs(runs),
            "screenshot": screenshot,
        }
    finally:
        client.close()


def main() -> int:
    args = parse_args()
    if args.iterations < 2 or args.iterations > 10:
        raise SystemExit("--iterations must be between 2 and 10")
    app = args.saccade_app.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    counter = TokenCounter()
    started = time.monotonic()
    report: dict[str, Any]
    try:
        saccade = run_saccade(app, args.url, args.iterations, counter)
        playwright_optimized = run_playwright(
            args.playwright_command,
            args.url,
            args.iterations,
            counter,
            "none",
            True,
        )
        playwright_default = run_playwright(
            args.playwright_command,
            args.url,
            2,
            counter,
            "full",
            False,
        )
        saccade_summary = saccade["summary"]
        playwright_summary = playwright_optimized["summary"]
        saccade_cold_context_tokens = (
            saccade["tool_schema_tokens"]
            + saccade_summary["cold"]["model_facing_tokens"]
        )
        playwright_cold_context_tokens = (
            playwright_optimized["tool_schema_tokens"]
            + playwright_summary["cold"]["model_facing_tokens"]
        )
        marginal_token_ratio = round(
            saccade_summary["median_model_facing_tokens"]
            / playwright_summary["median_model_facing_tokens"],
            3,
        )
        warm_speed_ratio = round(
            saccade_summary["warm_p50_task_ms"]
            / playwright_summary["warm_p50_task_ms"],
            3,
        )
        cold_context_token_ratio = round(
            saccade_cold_context_tokens / playwright_cold_context_tokens, 3
        )
        screenshot_tokens = playwright_optimized["screenshot"][
            "estimated_total_model_tokens"
        ]
        playwright_visual_task_tokens = round(
            playwright_summary["median_model_facing_tokens"] + screenshot_tokens
        )
        visual_task_token_ratio = round(
            saccade_summary["median_model_facing_tokens"]
            / playwright_visual_task_tokens,
            3,
        )
        report = {
            "schema": "saccade-playwright-parity-v1",
            "verdict": "PASS"
            if marginal_token_ratio < 1.0
            and cold_context_token_ratio < 1.0
            and warm_speed_ratio < 1.0
            else "FAIL_TARGET",
            "url": args.url,
            "iterations": args.iterations,
            "tokenizer": "o200k_base",
            "token_scope": "MCP tool schemas and complete tool results; common user/model text omitted equally",
            "image_scope": "Playwright screenshot is charged as image tokens plus its non-image result metadata; GPT-5.6 original/auto estimate uses ceil(width/32)*ceil(height/32)",
            "saccade": saccade,
            "playwright_optimized": playwright_optimized,
            "playwright_default": playwright_default,
            "comparison": {
                "saccade_to_playwright_optimized_marginal_token_ratio": marginal_token_ratio,
                "saccade_to_playwright_optimized_cold_context_token_ratio": cold_context_token_ratio,
                "saccade_to_playwright_optimized_warm_speed_ratio": warm_speed_ratio,
                "saccade_to_playwright_visual_task_token_ratio": visual_task_token_ratio,
                "saccade_cold_context_tokens": saccade_cold_context_tokens,
                "playwright_optimized_cold_context_tokens": playwright_cold_context_tokens,
                "playwright_visual_task_tokens": playwright_visual_task_tokens,
                "saccade_faster": warm_speed_ratio < 1.0,
                "saccade_fewer_marginal_tokens": marginal_token_ratio < 1.0,
                "saccade_fewer_cold_context_tokens": cold_context_token_ratio < 1.0,
                "saccade_fewer_visual_task_tokens": visual_task_token_ratio < 1.0,
            },
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-playwright-parity-v1",
            "verdict": "ERROR",
            "error": str(error),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    report_path = output / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"PLAYWRIGHT_PARITY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
