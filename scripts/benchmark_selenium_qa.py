#!/usr/bin/env python3
"""Matched form-control benchmark on Selenium's official public QA fixture."""

from __future__ import annotations

import argparse
import json
import os
import statistics
import time
from pathlib import Path
from typing import Any

from benchmark_playwright_parity import AgentViews, Mcp, Tokens, compact, result_text, result_value


URL = "https://www.selenium.dev/selenium/web/web-form.html"
TEXT_VALUE = "SACCADE-QA-TEXT-Ω"
TEXTAREA_VALUE = "SACCADE QA LINE ONE\nLINE TWO Ω"


def call_payload(response: dict[str, Any]) -> Any:
    return response.get("result") or response.get("error")


def summarize(runs: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "iterations": len(runs),
        "passed": sum(1 for run in runs if run["passed"]),
        "median_task_ms": round(statistics.median(run["task_ms"] for run in runs), 3),
        "median_model_facing_tokens": round(
            statistics.median(run["model_facing_tokens"] for run in runs), 3
        ),
        "all_task_ms": [run["task_ms"] for run in runs],
        "all_model_facing_tokens": [run["model_facing_tokens"] for run in runs],
    }


def current_target(observation: dict[str, Any], role: str, name: str) -> dict[str, Any]:
    for item in observation.get("objects", []):
        if item.get("role") == role and item.get("name") == name:
            return item
    raise RuntimeError(f"missing {role} named {name!r}")


def saccade_observe(
    client: Mcp,
    views: AgentViews,
    tab_id: str,
    payloads: list[Any],
    timeout: float = 30.0,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    after_revision: int | None = None
    while time.monotonic() < deadline:
        arguments: dict[str, Any] = {"tab_id": tab_id}
        if after_revision is not None:
            arguments.update({
                "after_revision": after_revision,
                "timeout_ms": max(1, min(30_000, int((deadline - time.monotonic()) * 1000))),
            })
        response, _ = client.tool("saccade.web.observe", arguments)
        payloads.append(call_payload(response))
        if not response.get("error"):
            observation = views.apply(result_value(response))
            if observation.get("objects"):
                return observation
            after_revision = int(observation["revision"])
    raise RuntimeError("Saccade observation did not arrive")


def run_saccade(
    runtime: Path,
    runtime_dir: Path,
    iterations: int,
    tokens: Tokens,
) -> dict[str, Any]:
    environment = os.environ.copy()
    environment["SACCADE_RUNTIME_DIR"] = str(runtime_dir)
    client = Mcp([str(runtime), "mcp"], environment)
    views = AgentViews()
    runs = []
    try:
        client.initialize()
        tools = client.tools()
        for _ in range(iterations):
            started = time.perf_counter()
            payloads: list[Any] = []
            opened, open_ms = client.tool("saccade.tabs.open", {"url": URL, "active": True})
            payloads.append(call_payload(opened))
            observe_started = time.perf_counter()
            observation = saccade_observe(client, views, result_value(opened)["tab_id"], payloads)
            observe_ms = round((time.perf_counter() - observe_started) * 1000, 3)
            actions = []
            planned = []
            for role, name, operation, payload, option in (
                ("text_field", "Text input", "type", {"kind": "text", "text": TEXT_VALUE}, None),
                ("text_area", "Textarea", "type", {"kind": "text", "text": TEXTAREA_VALUE}, None),
                ("select", "Dropdown (select)", "select", {"kind": "none"}, "Two"),
                ("checkbox", "Default checkbox", "click", {"kind": "none"}, None),
                ("radio", "Default radio", "click", {"kind": "none"}, None),
            ):
                target = current_target(observation, role, name)
                action_payload = payload
                if option is not None:
                    choice = current_target(observation, "option", option)
                    action_payload = {"kind": "select", "option_object_id": choice["object_id"]}
                actions.append({
                    "action_token": target["action_token"],
                    "operation": operation,
                    "payload": action_payload,
                })
                planned.append(role)
            form_response, form_ms = client.tool(
                "saccade.web.form.fill",
                {
                    "browser_instance_id": observation["browser_instance_id"],
                    "tab_id": observation["tab_id"],
                    "document_id": observation["document_id"],
                    "basis_revision": observation["revision"],
                    "actions": actions,
                },
            )
            payloads.append(call_payload(form_response))
            form = result_value(form_response)
            if not form.get("all_verified") or form.get("completed") != 5:
                raise RuntimeError(f"Saccade form plan failed: {form}")
            observation = views.apply(form["view"])
            receipts = [
                {
                    "role": role,
                    "dispatch_status": step["dispatch_status"],
                    "postcondition": step["postcondition"],
                }
                for role, step in zip(planned, form["steps"], strict=True)
            ]

            submit = current_target(observation, "button", "Submit")
            submit_response, submit_ms = client.tool(
                "saccade.web.act",
                {
                    "browser_instance_id": observation["browser_instance_id"],
                    "tab_id": observation["tab_id"],
                    "document_id": observation["document_id"],
                    "basis_revision": observation["revision"],
                    "action_token": submit["action_token"],
                    "operation": "click",
                    "payload": {"kind": "none"},
                },
            )
            payloads.append(call_payload(submit_response))
            submit_receipt = result_value(submit_response)
            if submit_receipt.get("postcondition") != "verified":
                raise RuntimeError(f"Saccade submit failed: {submit_receipt}")
            views.apply(submit_receipt["view"])
            receipts.append({
                "role": "button",
                "dispatch_status": submit_receipt["dispatch_status"],
                "postcondition": submit_receipt["postcondition"],
            })
            serialized = compact(payloads)
            if TEXT_VALUE in serialized or TEXTAREA_VALUE in serialized:
                raise RuntimeError("Saccade leaked supplied editable content into tool results")
            runs.append(
                {
                    "passed": True,
                    "task_ms": round((time.perf_counter() - started) * 1000, 3),
                    "timing_ms": {
                        "tabs_open": open_ms,
                        "initial_observe": observe_ms,
                        "form_fill": form_ms,
                        "submit": submit_ms,
                    },
                    "model_facing_tokens": tokens.count(payloads),
                    "model_facing_token_breakdown": {
                        "tabs_open": tokens.count(payloads[0]),
                        "initial_view": tokens.count(payloads[1:-2]),
                        "form_fill": tokens.count(payloads[-2]),
                        "submit": tokens.count(payloads[-1]),
                    },
                    "model_facing_bytes": len(serialized.encode()),
                    "receipts": receipts,
                }
            )
        return {
            "tool_count": len(tools),
            "tool_schema_tokens": tokens.count(tools),
            "runs": runs,
            "summary": summarize(runs),
        }
    finally:
        client.close()


def run_playwright(command: list[str], iterations: int, tokens: Tokens) -> dict[str, Any]:
    client = Mcp(
        command
        + [
            "--headless",
            "--browser",
            "chrome",
            "--isolated",
            "--snapshot-mode",
            "none",
            "--output-mode",
            "stdout",
            "--image-responses",
            "omit",
        ],
        os.environ.copy(),
    )
    runs = []
    try:
        client.initialize()
        tools = client.tools()
        fields = [
            {"target": "#my-text-id", "name": "Text input", "type": "textbox", "value": TEXT_VALUE},
            {"target": "[name='my-textarea']", "name": "Textarea", "type": "textbox", "value": TEXTAREA_VALUE},
            {"target": "[name='my-select']", "name": "Dropdown (select)", "type": "combobox", "value": "Two"},
            {"target": "#my-check-2", "name": "Default checkbox", "type": "checkbox", "value": "true"},
            {"target": "#my-radio-2", "name": "Default radio", "type": "radio", "value": "true"},
        ]
        for _ in range(iterations):
            started = time.perf_counter()
            payloads = []
            for tool, arguments in (
                ("browser_navigate", {"url": URL}),
                ("browser_fill_form", {"fields": fields}),
                ("browser_click", {"target": "button[type='submit']", "element": "Submit"}),
                ("browser_evaluate", {"function": "() => document.body.innerText"}),
            ):
                response, _ = client.tool(tool, arguments)
                payloads.append(call_payload(response))
                if response.get("error"):
                    raise RuntimeError(str(response["error"]))
            if "received" not in result_text(response).casefold():
                raise RuntimeError("Playwright did not reach Selenium's submitted form result")
            runs.append(
                {
                    "passed": True,
                    "task_ms": round((time.perf_counter() - started) * 1000, 3),
                    "model_facing_tokens": tokens.count(payloads),
                    "model_facing_bytes": len(compact(payloads).encode()),
                    "tool_calls": 4,
                }
            )
        return {
            "tool_count": len(tools),
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
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument(
        "--playwright-command",
        nargs="+",
        default=["npx", "-y", "@playwright/mcp@0.0.78"],
    )
    args = parser.parse_args()
    tokens = Tokens()
    started = time.monotonic()
    try:
        saccade = run_saccade(
            args.runtime.resolve(), args.runtime_dir.resolve(), args.iterations, tokens
        )
        playwright = run_playwright(args.playwright_command, args.iterations, tokens)
        report = {
            "schema": "saccade-selenium-qa-parity/1",
            "verdict": "PASS"
            if saccade["summary"]["passed"] == args.iterations
            and playwright["summary"]["passed"] == args.iterations
            else "FAIL",
            "fixture": URL,
            "iterations": args.iterations,
            "scope": "Matched Selenium web-form completion; Saccade runs one five-control local form plan plus a separate verified submit, while Playwright receives fill_form and selector best case.",
            "saccade": saccade,
            "playwright": playwright,
            "comparison": {
                "task_time_ratio": round(
                    saccade["summary"]["median_task_ms"]
                    / playwright["summary"]["median_task_ms"],
                    3,
                ),
                "task_token_ratio": round(
                    saccade["summary"]["median_model_facing_tokens"]
                    / playwright["summary"]["median_model_facing_tokens"],
                    3,
                ),
            },
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:  # noqa: BLE001
        report = {
            "schema": "saccade-selenium-qa-parity/1",
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
