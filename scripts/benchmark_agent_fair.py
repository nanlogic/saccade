#!/usr/bin/env python3
"""Run the same unknown-page task through Codex with Saccade and Playwright MCP."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import time
import urllib.parse
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
RESULT_SCHEMA = ROOT / "benchmarks/agent_result.schema.json"
TOOL_ITEM_TYPES = {"mcp_tool_call", "tool_call", "function_call"}


def compact(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def load_task(path: Path) -> dict[str, Any]:
    task = json.loads(path.read_text(encoding="utf-8"))
    if task.get("schema") != "saccade-agent-benchmark-task/1":
        raise ValueError("unsupported benchmark task schema")
    required = {"name", "url", "task", "success", "redact", "timeout_seconds"}
    if not required.issubset(task):
        raise ValueError(f"benchmark task is missing {sorted(required - set(task))}")
    if not str(task["url"]).startswith(("http://", "https://")):
        raise ValueError("benchmark URL must use HTTP or HTTPS")
    evidence = task["success"].get("tool_output_contains")
    if not isinstance(evidence, list) or not evidence or not all(isinstance(item, str) and item for item in evidence):
        raise ValueError("success.tool_output_contains must be a non-empty string list")
    if not isinstance(task["redact"], list) or not all(isinstance(item, str) and item for item in task["redact"]):
        raise ValueError("redact must be a string list")
    return task


def prompt_for(task: dict[str, Any], lane: str) -> str:
    authorization = (
        "Wayne authorizes Saccade as the only browser route for this lane."
        if lane == "saccade"
        else "Wayne explicitly authorizes Playwright as the only browser route for this comparison lane. Saccade is intentionally unavailable in this lane."
    )
    return f"""You are one lane in a browser-agent benchmark. Start with no knowledge of the page. {authorization}

URL: {task['url']}
Task: {task['task']}

Use only the connected browser MCP tools. Do not use shell commands, web search, source inspection, selectors, XPath, DOM queries, JavaScript evaluation, coordinates, screenshots, or remembered site structure. Discover the page through the browser tool's normal semantic observation or snapshot, operate it, then inspect browser evidence after submission. Do not ask a human for help. Return completed=true only when browser tool output proves the requested task completed; otherwise return completed=false with a concise reason."""


def toml_string(value: str) -> str:
    return json.dumps(value)


def common_codex_command(model: str | None, workdir: Path) -> list[str]:
    command = [
        "codex", "exec", "--json", "--ephemeral", "--ignore-user-config", "--ignore-rules",
        "--skip-git-repo-check", "--sandbox", "read-only", "-C", str(workdir),
        "--output-schema", str(RESULT_SCHEMA), "--disable", "shell_tool",
        "-c", 'web_search="disabled"', "-c", "features.apps=false",
        "-c", "features.multi_agent=false", "-c", "agents.enabled=false",
    ]
    if model:
        command.extend(["--model", model])
    return command


def lane_command(
    lane: str,
    model: str | None,
    workdir: Path,
    runtime: Path,
    runtime_dir: Path,
    playwright_package: str,
) -> list[str]:
    command = common_codex_command(model, workdir)
    if lane == "saccade":
        command.extend([
            "-c", f"mcp_servers.saccade.command={toml_string(str(runtime))}",
            "-c", 'mcp_servers.saccade.args=["mcp"]',
            "-c", f'mcp_servers.saccade.env={{SACCADE_RUNTIME_DIR={toml_string(str(runtime_dir))}}}',
            "-c", 'mcp_servers.saccade.default_tools_approval_mode="approve"',
        ])
    elif lane == "playwright":
        playwright_args = [
            "-y", playwright_package, "--headless", "--browser", "chrome",
            "--isolated", "--output-mode", "stdout", "--image-responses", "omit",
        ]
        command.extend([
            "-c", 'mcp_servers.playwright.command="npx"',
            "-c", f"mcp_servers.playwright.args={compact(playwright_args)}",
            "-c", 'mcp_servers.playwright.default_tools_approval_mode="approve"',
        ])
    else:
        raise ValueError(f"unknown lane {lane}")
    return command


def parse_events(stdout: str) -> list[dict[str, Any]]:
    events = []
    for line in stdout.splitlines():
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError("Codex JSONL event must be an object")
        events.append(value)
    return events


def item_is_tool(item: dict[str, Any]) -> bool:
    kind = str(item.get("type") or "")
    return kind in TOOL_ITEM_TYPES or "tool" in kind or kind.startswith("mcp_")


def lane_summary(lane: str, elapsed_ms: float, returncode: int, events: list[dict[str, Any]], stderr: str, task: dict[str, Any]) -> dict[str, Any]:
    completed_items = [
        event.get("item") for event in events
        if event.get("type") == "item.completed" and isinstance(event.get("item"), dict)
    ]
    tool_items = [item for item in completed_items if item_is_tool(item)]
    agent_messages = [item for item in completed_items if item.get("type") == "agent_message"]
    final_value: dict[str, Any] = {}
    if agent_messages:
        text = str(agent_messages[-1].get("text") or "")
        try:
            parsed = json.loads(text)
            if isinstance(parsed, dict):
                final_value = parsed
        except json.JSONDecodeError:
            final_value = {"completed": False, "summary": text}
    tool_text = compact(tool_items)
    required_evidence = task["success"]["tool_output_contains"]
    evidence = {needle: needle.casefold() in tool_text.casefold() for needle in required_evidence}
    usage_events = [event.get("usage") for event in events if event.get("type") == "turn.completed"]
    usage = usage_events[-1] if usage_events and isinstance(usage_events[-1], dict) else {}
    passed = returncode == 0 and final_value.get("completed") is True and all(evidence.values())
    return {
        "lane": lane,
        "passed": passed,
        "elapsed_ms": round(elapsed_ms, 3),
        "returncode": returncode,
        "usage": usage,
        "tool_calls": len(tool_items),
        "model_messages": len(agent_messages),
        "success_evidence": evidence,
        "final": final_value,
        "stderr_tail": stderr[-2000:],
    }


def redact_text(text: str, values: list[str]) -> str:
    for value in sorted(values, key=len, reverse=True):
        variants = {
            value,
            value.replace("\n", "\\n"),
            value.replace("\n", "\r\n"),
            urllib.parse.quote(value, safe=""),
            urllib.parse.quote_plus(value, safe=""),
            urllib.parse.quote(value.replace("\n", "\r\n"), safe=""),
            urllib.parse.quote_plus(value.replace("\n", "\r\n"), safe=""),
        }
        for variant in sorted(variants, key=len, reverse=True):
            text = text.replace(variant, "[REDACTED_EDITABLE]")
    return text


def run_lane(
    lane: str,
    task: dict[str, Any],
    model: str | None,
    runtime: Path,
    runtime_dir: Path,
    playwright_package: str,
    output_dir: Path,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix=f"saccade-fair-{lane}-") as temporary:
        workdir = Path(temporary)
        command = lane_command(lane, model, workdir, runtime, runtime_dir, playwright_package)
        started = time.perf_counter()
        completed = subprocess.run(
            [*command, prompt_for(task, lane)],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=int(task["timeout_seconds"]),
            check=False,
            env=os.environ.copy(),
        )
        elapsed_ms = (time.perf_counter() - started) * 1000
    events = parse_events(completed.stdout)
    redactions = task["redact"]
    (output_dir / f"{lane}.jsonl").write_text(
        redact_text(completed.stdout, redactions), encoding="utf-8"
    )
    (output_dir / f"{lane}.stderr.log").write_text(
        redact_text(completed.stderr, redactions), encoding="utf-8"
    )
    return lane_summary(lane, elapsed_ms, completed.returncode, events, completed.stderr, task)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model")
    parser.add_argument("--playwright-package", default="@playwright/mcp@0.0.78")
    parser.add_argument("--order", choices=("saccade-first", "playwright-first"), default="saccade-first")
    args = parser.parse_args()
    task = load_task(args.task.resolve())
    output_dir = args.output.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    order = ["saccade", "playwright"] if args.order == "saccade-first" else ["playwright", "saccade"]
    lanes: dict[str, Any] = {}
    started = time.monotonic()
    for lane in order:
        lanes[lane] = run_lane(
            lane, task, args.model, args.runtime.resolve(), args.runtime_dir.resolve(),
            args.playwright_package, output_dir,
        )
    report = {
        "schema": "saccade-agent-benchmark/1",
        "task": {key: task[key] for key in ("name", "url", "success")},
        "agent": {"driver": "codex exec", "model": args.model or "codex-default-recommended"},
        "order": order,
        "selector_or_site_execution_logic": False,
        "lanes": lanes,
        "verdict": "PASS" if all(lane["passed"] for lane in lanes.values()) else "FAIL",
        "duration_seconds": round(time.monotonic() - started, 3),
    }
    report_text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    (output_dir / "report.json").write_text(
        redact_text(report_text, task["redact"]), encoding="utf-8"
    )
    print(json.dumps({"verdict": report["verdict"], "output": str(output_dir / "report.json")}))
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
