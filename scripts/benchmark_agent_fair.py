#!/usr/bin/env python3
"""Run the same unknown-page task through Codex with Saccade and Playwright MCP."""

from __future__ import annotations

import argparse
import datetime as dt
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
PLAYWRIGHT_LOCK = ROOT / "benchmarks/playwright-mcp.lock.json"
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


def load_playwright_lock(path: Path = PLAYWRIGHT_LOCK) -> dict[str, Any]:
    lock = json.loads(path.read_text(encoding="utf-8"))
    if lock.get("schema") != "saccade-playwright-mcp-lock/1":
        raise ValueError("unsupported Playwright MCP lock schema")
    if lock.get("package") != "@playwright/mcp" or not isinstance(lock.get("version"), str):
        raise ValueError("Playwright MCP lock must name an exact official package version")
    if not lock["version"] or any(character in lock["version"] for character in "@*^~<>= "):
        raise ValueError("Playwright MCP lock version must be exact")
    return lock


def prompt_for(task: dict[str, Any], lane: str, operation_mode: str = "inferred") -> str:
    if operation_mode not in {"explicit", "inferred"}:
        raise ValueError("operation_mode must be explicit or inferred")
    authorization = (
        "Wayne authorizes Saccade as the only browser route for this lane."
        if lane == "saccade"
        else "Wayne explicitly authorizes Playwright as the only browser route for this comparison lane. Saccade is intentionally unavailable in this lane."
    )
    operation_contract = (
        "For this explicit-operation control lane, include operation in every single and batched "
        "saccade.act action, even when Truth exposes only one affordance. "
        if operation_mode == "explicit"
        else
        "For this inferred-operation lane, omit operation from every single and batched saccade.act "
        "action. Supply only object_id and any required value or option_object_id; Runtime must compile "
        "the current Truth affordance. "
    )
    route = (
        "Call saccade.system.capabilities once, then saccade.tabs.open. Make exactly one initial "
        "saccade.truth.read. For a read-only goal naming multiple distinct labels or fact phrases, use "
        "one query:{text_any:[...],roles:[\"heading\",\"paragraph\",\"list_item\",\"link\",\"button\",\"status\"],"
        "frame_scope:\"root\",min_objects:<number of distinct targets>,max_objects:32} containing every "
        "exact requested phrase; do not let one actionable target suppress the structural targets. "
        "Otherwise, when the goal names one actionable label, use exactly query:{text:\"LABEL\","
        "roles:[\"button\"],frame_scope:\"root\",min_objects:1,max_objects:12}; replace LABEL and "
        "the role but keep the plural roles array. The returned working set "
        "already includes bounded nearby decision text and sibling controls. For a form, use one "
        "query:{text_any:[...],roles:[...],frame_scope:\"root\",max_objects:32} containing the exact "
        "required labels and relevant control roles. "
        "Do not issue another initial query to collect adjacent labels or context. If the first read is "
        "a catalog, request details once for only task-relevant IDs. Execute only with saccade.act; "
        "batch independent form edits, never submit/navigation. A type action uses value. "
        + operation_contract +
        "Treat verified/all_verified receipts as proof and fold any receipt transition immediately. For "
        "an iterative queue, continue directly from act.transition when it contains the next record or "
        "completion proof. If any saccade.act text or structured transition contains the exact success "
        "condition string, the task is complete: return completed=true immediately and do not read again. "
        "Only when work remains and the receipt has no transition, make one plain "
        "truth.read with after_revision equal to the receipt revision; do not query again. For a "
        "non-iterative task, do not read again after verified completion. On stale, fold one exact-tab "
        "delta and resume; never resync or repeat a full read. Close the temporary Agent-owned tab."
        if lane == "saccade"
        else
        "Use the official Playwright MCP semantic snapshot and its object-addressed actions. Close "
        "the temporary Playwright page when finished."
    )
    return f"""You are one lane in a browser-agent benchmark. Start with no knowledge of the page. {authorization}

URL: {task['url']}
Task: {task['task']}

{route}

Use only the connected lane MCP. Do not use shell commands, web search, source inspection, selectors, XPath, DOM queries, JavaScript evaluation, coordinates, screenshots, another browser tool, or remembered site structure. Do not ask a human for help. Return completed=true only when browser tool output proves the requested task completed; otherwise return completed=false with a concise reason."""


def toml_string(value: str) -> str:
    return json.dumps(value)


def common_codex_command(model: str | None, effort: str | None, workdir: Path) -> list[str]:
    command = [
        "codex", "exec", "--json", "--ephemeral", "--ignore-user-config", "--ignore-rules",
        "--skip-git-repo-check", "--sandbox", "read-only", "-C", str(workdir),
        "--output-schema", str(RESULT_SCHEMA), "--disable", "shell_tool",
        "-c", 'web_search="disabled"', "-c", "features.apps=false",
        "-c", "features.multi_agent=false", "-c", "agents.enabled=false",
    ]
    if model:
        command.extend(["--model", model])
    if effort:
        command.extend(["-c", f"model_reasoning_effort={toml_string(effort)}"])
    return command


def lane_command(
    lane: str,
    model: str | None,
    effort: str | None,
    workdir: Path,
    runtime: Path,
    runtime_dir: Path,
    playwright_package: str,
) -> list[str]:
    command = common_codex_command(model, effort, workdir)
    if lane == "saccade":
        command.extend([
            "-c", f"mcp_servers.saccade.command={toml_string(str(runtime))}",
            "-c", 'mcp_servers.saccade.args=["mcp"]',
            "-c", (
                "mcp_servers.saccade.env={"
                f"SACCADE_RUNTIME_DIR={toml_string(str(runtime_dir))},"
                'SACCADE_BENCHMARK_FRESH_INPUT_POLICY="1"}'
            ),
            "-c", 'mcp_servers.saccade.default_tools_approval_mode="approve"',
        ])
    elif lane == "playwright":
        # @playwright/mcp 0.0.79 removed --output-mode. Its 0.0.78 value here was
        # "stdout", which was already that version's default, so dropping the flag
        # preserves the lane's behavior instead of changing it.
        playwright_args = [
            "-y", playwright_package, "--headless", "--browser", "chrome",
            "--isolated", "--image-responses", "omit",
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


def tool_name(item: dict[str, Any]) -> str:
    for key in ("tool", "name", "server_tool_name"):
        if item.get(key):
            return str(item[key])
    return str(item.get("type") or "unknown_tool")


def browser_metrics(tool_items: list[dict[str, Any]]) -> dict[str, Any]:
    trace = []
    serialized_items = []
    initial_transfer_bytes = 0
    steady_state_bytes = 0
    action_receipt_bytes = 0
    action_seen = False
    view_modes: list[str] = []
    transition_views = 0
    local_wait_values: list[int] = []
    for index, item in enumerate(tool_items, start=1):
        serialized = compact(item)
        serialized_items.append(serialized)
        name = tool_name(item).casefold()
        is_action = any(word in name for word in ("saccade.act", "browser_click", "browser_type", "browser_fill", "browser_select"))
        result_bytes = len(compact(item.get("result")).encode()) if item.get("result") is not None else 0
        if not action_seen and not is_action and any(word in name for word in ("truth.read", "navigate", "snapshot", "find")):
            initial_transfer_bytes += result_bytes
        elif action_seen or is_action:
            steady_state_bytes += result_bytes
        if is_action:
            action_receipt_bytes += result_bytes
        action_seen = action_seen or is_action
        result = item.get("result")
        projected = result
        if isinstance(result, dict) and isinstance(result.get("structured_content"), dict):
            projected = result["structured_content"]
        if isinstance(projected, dict):
            local_wait = projected.get("local_wait_ms")
            if isinstance(local_wait, (int, float)) and local_wait > 0:
                local_wait_values.append(round(local_wait))
            if isinstance(projected.get("mode"), str):
                view_modes.append(projected["mode"])
            transition = projected.get("transition")
            if isinstance(transition, dict):
                transition_views += 1
                if isinstance(transition.get("mode"), str):
                    view_modes.append(transition["mode"])
        trace.append({
            "sequence": index,
            "tool": tool_name(item),
            "transcript_bytes": len(serialized.encode()),
        })
    combined = "\n".join(serialized_items).casefold()
    observation_calls = sum(
        any(word in row["tool"].casefold() for word in ("truth.read", "observe", "snapshot"))
        for row in trace
    )
    return {
        "trace": trace,
        "transcript_bytes": sum(row["transcript_bytes"] for row in trace),
        "initial_transfer_bytes": initial_transfer_bytes or None,
        "discovery": {"transfer_bytes": initial_transfer_bytes or None},
        "steady_state": {
            "transfer_bytes": steady_state_bytes,
            "action_receipt_bytes": action_receipt_bytes,
            "delta_views": view_modes.count("delta"),
            "transition_views": transition_views,
        },
        "full_views": view_modes.count("full"),
        "working_set_views": view_modes.count("working_set"),
        "catalog_views": view_modes.count("catalog"),
        "detail_views": view_modes.count("details"),
        "delta_views": view_modes.count("delta"),
        "stale_events": combined.count("stale_before_dispatch") + combined.count("stale action basis"),
        "stability": {
            "local_waits": len(local_wait_values),
            "local_wait_ms_total": sum(local_wait_values),
            "local_wait_ms_max": max(local_wait_values, default=0),
            "stale": combined.count("stale_before_dispatch") + combined.count("stale action basis"),
            "retries": combined.count('"retry_safe":true'),
            "replacements": combined.count("replacement"),
            "failure_prepare": combined.count('"failure_stage":"prepare"'),
            "failure_dispatch": combined.count('"failure_stage":"dispatch"'),
            "failure_verify": combined.count('"failure_stage":"verify"'),
        },
        "observe_or_snapshot_calls": observation_calls,
        "post_initial_reobservation_calls": max(0, observation_calls - 1),
        "action_return_to_delta_read_ms": None,
        "latency_measurement_status": "not_separately_available_in_codex_jsonl; included_in_end_to_end",
    }


def normalized_model_usage(usage: dict[str, Any]) -> dict[str, int]:
    input_tokens = int(usage.get("input_tokens") or 0)
    details = usage.get("input_tokens_details") or {}
    cached_input = int(
        usage.get("cached_input_tokens")
        or usage.get("cache_read_input_tokens")
        or details.get("cached_tokens", 0)
        or 0
    )
    return {
        "input_tokens": input_tokens,
        "cached_input_tokens": cached_input,
        "non_cached_input_tokens": max(0, input_tokens - cached_input),
        "output_tokens": int(usage.get("output_tokens") or 0),
    }


def without_echoed_query(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: without_echoed_query(item)
            for key, item in value.items()
            if key != "query"
        }
    if isinstance(value, list):
        return [without_echoed_query(item) for item in value]
    return value


def positive_tool_evidence(tool_items: list[dict[str, Any]], needle: str) -> bool:
    folded_needle = needle.casefold()
    for item in tool_items:
        result_text = compact(without_echoed_query(item.get("result"))).casefold()
        if folded_needle not in result_text:
            continue
        if "no matches found for" in result_text:
            continue
        return True
    return False


def infrastructure_failure(returncode: int, timed_out: bool, tool_items: list[dict[str, Any]], text: str) -> str | None:
    folded = text.casefold()
    if timed_out:
        return "timeout"
    if "529" in folded and "overload" in folded:
        return "api_529_overloaded"
    if not tool_items and returncode != 0:
        if "not logged in" in folded or "please run /login" in folded:
            return "agent_authentication"
        return "zero_tool_calls"
    return None


def lane_summary(lane: str, elapsed_ms: float, returncode: int, events: list[dict[str, Any]], stderr: str, task: dict[str, Any], timed_out: bool = False, expected_contract_hash: str | None = None) -> dict[str, Any]:
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
    if not final_value and not agent_messages:
        final_value = {"completed": False}
    tool_text = compact(tool_items)
    required_evidence = task["success"]["tool_output_contains"]
    evidence = {needle: positive_tool_evidence(tool_items, needle) for needle in required_evidence}
    usage_events = [event.get("usage") for event in events if event.get("type") == "turn.completed"]
    usage = usage_events[-1] if usage_events and isinstance(usage_events[-1], dict) else {}
    contract_hash_valid = lane != "saccade" or expected_contract_hash is None or expected_contract_hash in tool_text
    infrastructure = infrastructure_failure(returncode, timed_out, tool_items, f"{stderr}\n{compact(final_value)}")
    oracle_complete = all(evidence.values())
    model_completed = final_value.get("completed") is True
    passed = (
        infrastructure is None
        and returncode == 0
        and contract_hash_valid
        and oracle_complete
        and model_completed
    )
    model_report_consistent = model_completed or not oracle_complete
    # A lane that never reached its browser MCP is broken harness plumbing, not a
    # lost comparison. Name it explicitly so the other lane is never credited.
    failure_reason = final_value.get("failure_reason")
    if not tool_items and not timed_out:
        failure_reason = failure_reason or "browser_mcp_unavailable_no_tool_calls"
    if not contract_hash_valid:
        failure_reason = "stale_mcp_contract_or_registry"
    return {
        "lane": lane,
        "passed": passed,
        "elapsed_ms": round(elapsed_ms, 3),
        "returncode": returncode,
        "timed_out": timed_out,
        "usage": usage,
        "model_usage": normalized_model_usage(usage),
        "tool_calls": len(tool_items),
        "browser_metrics": browser_metrics(tool_items),
        "model_messages": len(agent_messages),
        "success_evidence": evidence,
        "final": final_value,
        "model_report_consistent": model_report_consistent,
        "failure_reason": failure_reason,
        "contract_hash_expected": expected_contract_hash if lane == "saccade" else None,
        "contract_hash_valid": contract_hash_valid,
        "infrastructure": {"failure": infrastructure},
        "stderr_tail": stderr[-2000:],
    }


def redact_text(text: str, values: list[str]) -> str:
    for value in sorted(values, key=len, reverse=True):
        variants = {
            value,
            value.replace("\n", " "),
            value.replace("\n", "\\n"),
            value.replace("\n", "\\\\n"),
            value.replace("\n", "\r\n"),
            value.replace("\n", "\\r\\n"),
            value.replace("\n", "\\\\r\\\\n"),
            urllib.parse.quote(value, safe=""),
            urllib.parse.quote_plus(value, safe=""),
            urllib.parse.quote(value.replace("\n", "\r\n"), safe=""),
            urllib.parse.quote_plus(value.replace("\n", "\r\n"), safe=""),
        }
        for variant in sorted(variants, key=len, reverse=True):
            text = text.replace(variant, "[REDACTED_EDITABLE]")
    return text


def load_client_native_evidence(path: Path, task: dict[str, Any], order: str) -> dict[str, Any]:
    evidence = json.loads(path.read_text(encoding="utf-8"))
    if evidence.get("schema") != "saccade-client-native-lane/1":
        raise ValueError("unsupported client-native evidence schema")
    if evidence.get("task") != {"name": task["name"], "url": task["url"]}:
        raise ValueError("client-native evidence belongs to a different task")
    if evidence.get("order") != order:
        raise ValueError("client-native evidence belongs to a different lane order")
    browser = evidence.get("browser") or {}
    if (browser.get("family") != "chrome"
            or browser.get("same_saccade_instance") is not True
            or browser.get("same_tab") is not True
            or not browser.get("browser_instance_id")
            or not browser.get("tab_id")):
        raise ValueError("client-native evidence does not prove the Saccade Chrome tab boundary")
    truth = evidence.get("truth") or {}
    if (truth.get("browser_instance_id") != browser.get("browser_instance_id")
            or truth.get("tab_id") != browser.get("tab_id")):
        raise ValueError("client-native evidence does not bind Truth to the acted browser tab")
    summary = evidence.get("summary")
    if not isinstance(summary, dict) or summary.get("lane") != "saccade":
        raise ValueError("client-native evidence has no Saccade lane summary")
    timing = evidence.get("timing")
    if (not isinstance(timing, dict)
            or not timing.get("started_at")
            or not timing.get("completed_at")
            or timing.get("clock_source") != "client_monotonic"
            or not isinstance(timing.get("elapsed_ms"), (int, float))
            or timing["elapsed_ms"] <= 0):
        raise ValueError("client-native evidence has no trusted end-to-end monotonic timing")
    result = dict(summary)
    result["timing"] = timing
    result["same_tab_proof"] = {
        "browser_instance_id": browser["browser_instance_id"],
        "tab_id": browser["tab_id"],
    }
    return result


def parse_timestamp(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("lane timestamps must include a timezone")
    return parsed


def validate_lane_order(saccade: dict[str, Any], playwright: dict[str, Any], order: str) -> None:
    first, second = (saccade, playwright) if order == "saccade-first" else (playwright, saccade)
    first_end = parse_timestamp(str(first["timing"]["completed_at"]))
    second_start = parse_timestamp(str(second["timing"]["started_at"]))
    if first_end > second_start:
        raise ValueError(f"lane timestamps do not prove {order}")


def wait_for_evidence(path: Path, timeout_seconds: int) -> None:
    deadline = time.monotonic() + timeout_seconds
    while not path.exists():
        if time.monotonic() >= deadline:
            raise TimeoutError("client_native_same_tab_evidence_timeout")
        time.sleep(0.25)


def blocked_lane(lane: str, reason: str) -> dict[str, Any]:
    return {
        "lane": lane,
        "passed": False,
        "elapsed_ms": 0.0,
        "returncode": 0,
        "timed_out": False,
        "usage": {},
        "tool_calls": 0,
        "browser_metrics": browser_metrics([]),
        "model_messages": 0,
        "success_evidence": {},
        "final": {"completed": False, "failure_reason": reason},
        "failure_reason": reason,
        "stderr_tail": "",
    }


def run_lane(
    lane: str,
    task: dict[str, Any],
    model: str | None,
    effort: str | None,
    runtime: Path,
    runtime_dir: Path,
    playwright_package: str,
    output_dir: Path,
    operation_mode: str = "inferred",
    expected_contract_hash: str | None = None,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix=f"saccade-fair-{lane}-") as temporary:
        workdir = Path(temporary)
        command = lane_command(
            lane,
            model,
            effort,
            workdir,
            runtime,
            runtime_dir,
            playwright_package,
        )
        started_at = dt.datetime.now(dt.timezone.utc)
        started = time.perf_counter()
        try:
            lane_env = os.environ.copy()
            if lane == "saccade":
                lane_env["SACCADE_BENCHMARK_FRESH_INPUT_POLICY"] = "1"
            completed = subprocess.run(
                [*command, prompt_for(task, lane, operation_mode)],
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=int(task["timeout_seconds"]),
                check=False,
                env=lane_env,
            )
            stdout = completed.stdout
            stderr = completed.stderr
            returncode = completed.returncode
            timed_out = False
        except subprocess.TimeoutExpired as error:
            def decoded(value: str | bytes | None) -> str:
                if isinstance(value, bytes):
                    return value.decode(errors="replace")
                return value or ""
            stdout = decoded(error.stdout)
            stderr = decoded(error.stderr) + f"\nbenchmark lane timed out after {task['timeout_seconds']} seconds\n"
            returncode = 124
            timed_out = True
        elapsed_ms = (time.perf_counter() - started) * 1000
        completed_at = dt.datetime.now(dt.timezone.utc)
    events = parse_events(stdout)
    redactions = task["redact"]
    (output_dir / f"{lane}.jsonl").write_text(
        redact_text(stdout, redactions), encoding="utf-8"
    )
    (output_dir / f"{lane}.stderr.log").write_text(
        redact_text(stderr, redactions), encoding="utf-8"
    )
    summary = lane_summary(
        lane, elapsed_ms, returncode, events, stderr, task, timed_out,
        expected_contract_hash,
    )
    summary["timing"] = {
        "started_at": started_at.isoformat().replace("+00:00", "Z"),
        "completed_at": completed_at.isoformat().replace("+00:00", "Z"),
        "clock_source": "python.perf_counter",
        "elapsed_ms": round(elapsed_ms, 3),
    }
    return summary


def lane_evidence_errors(lane: dict[str, Any]) -> list[str]:
    errors = []
    timing = lane.get("timing") or {}
    if timing.get("clock_source") not in {"client_monotonic", "python.perf_counter"}:
        errors.append("trusted_monotonic_clock_missing")
    if not isinstance(timing.get("elapsed_ms"), (int, float)) or timing.get("elapsed_ms", 0) <= 0:
        errors.append("end_to_end_elapsed_ms_missing")
    usage = lane.get("usage") or {}
    if not isinstance(usage.get("input_tokens"), int) or usage.get("input_tokens", 0) <= 0:
        errors.append("model_input_tokens_missing")
    metrics = lane.get("browser_metrics") or {}
    if not isinstance(metrics.get("initial_transfer_bytes"), int) or metrics.get("initial_transfer_bytes", 0) <= 0:
        errors.append("initial_transfer_bytes_missing")
    if not isinstance(lane.get("tool_calls"), int) or lane.get("tool_calls", 0) <= 0:
        errors.append("tool_call_count_missing")
    if lane.get("lane") == "saccade" and lane.get("contract_hash_expected") and lane.get("contract_hash_valid") is not True:
        errors.append("mcp_contract_hash_mismatch")
    if (lane.get("infrastructure") or {}).get("failure"):
        errors.append(f"infrastructure:{lane['infrastructure']['failure']}")
    return errors


def runtime_identity(runtime: Path, runtime_dir: Path) -> dict[str, str]:
    completed = subprocess.run(
        [str(runtime), "doctor"], capture_output=True, text=True, check=False,
        env={**os.environ, "SACCADE_RUNTIME_DIR": str(runtime_dir)}, timeout=15,
    )
    try:
        value = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ValueError("Runtime doctor did not return its MCP contract identity") from error
    contract_hash = value.get("mcp_contract_hash")
    runtime_version = value.get("runtime_version")
    if not isinstance(contract_hash, str) or len(contract_hash) != 64:
        raise ValueError("Runtime doctor returned an invalid MCP contract hash")
    if not isinstance(runtime_version, str) or not runtime_version:
        raise ValueError("Runtime doctor returned no Runtime version")
    return {"runtime_version": runtime_version, "mcp_contract_hash": contract_hash}


def measure_control_plane(command: list[str], environment: dict[str, str], profile_path: Path | None = None) -> dict[str, Any]:
    requests = "\n".join([
        compact({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"saccade-benchmark-meter","version":"1"}}}),
        compact({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
        compact({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    ]) + "\n"
    try:
        completed = subprocess.run(
            command, input=requests, capture_output=True, text=True, check=False,
            env=environment, timeout=45,
        )
    except subprocess.TimeoutExpired:
        return {"valid": False, "error": "control_plane_timeout"}
    responses = {}
    for line in completed.stdout.splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if value.get("id") in (1, 2):
            responses[value["id"]] = (line, value)
    if 1 not in responses or 2 not in responses:
        return {"valid": False, "error": "control_plane_responses_missing", "stderr_tail": completed.stderr[-500:]}
    initialize_line, initialize = responses[1]
    tools_line, listed = responses[2]
    instructions = initialize.get("result", {}).get("instructions") or ""
    profile_bytes = 0
    if profile_path and profile_path.exists():
        try:
            profile = json.loads(profile_path.read_text(encoding="utf-8"))
            profile_bytes = len(str(profile.get("behavior") or "").encode())
        except (OSError, json.JSONDecodeError):
            pass
    return {
        "valid": completed.returncode == 0,
        "initialize_bytes": len(initialize_line.encode()),
        "instructions_bytes": len(instructions.encode()),
        "tools_list_bytes": len(tools_line.encode()),
        "profile_behavior_bytes": profile_bytes,
        "task_prompt_bytes": None,
        "combined_mcp_bytes": len(initialize_line.encode()) + len(tools_line.encode()),
        "tool_count": len(listed.get("result", {}).get("tools", [])),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model")
    parser.add_argument("--effort", choices=("low", "medium", "high", "xhigh"), default="low")
    parser.add_argument("--playwright-package")
    parser.add_argument("--order", choices=("saccade-first", "playwright-first"), default="saccade-first")
    parser.add_argument("--operation-mode", choices=("inferred", "explicit"), default="inferred")
    args = parser.parse_args()
    playwright_lock = load_playwright_lock()
    locked_playwright_package = f"{playwright_lock['package']}@{playwright_lock['version']}"
    if args.playwright_package and args.playwright_package != locked_playwright_package:
        parser.error(f"--playwright-package must match frozen {locked_playwright_package}")
    playwright_package = locked_playwright_package
    task = load_task(args.task.resolve())
    runtime = args.runtime.resolve()
    runtime_dir = args.runtime_dir.resolve()
    identity = runtime_identity(runtime, runtime_dir)
    saccade_environment = {
        **os.environ,
        "SACCADE_RUNTIME_DIR": str(runtime_dir),
        "SACCADE_BENCHMARK_FRESH_INPUT_POLICY": "1",
    }
    control_plane = {
        "saccade": measure_control_plane(
            [str(runtime), "mcp"], saccade_environment, runtime_dir / "profile.json",
        ),
        "playwright": measure_control_plane(
            ["npx", "-y", playwright_package, "--headless", "--browser", "chrome", "--isolated", "--image-responses", "omit"],
            os.environ.copy(),
        ),
    }
    for lane in ("saccade", "playwright"):
        control_plane[lane]["task_prompt_bytes"] = len(
            prompt_for(task, lane, args.operation_mode).encode()
        )
    output_dir = args.output.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    order = ["saccade", "playwright"] if args.order == "saccade-first" else ["playwright", "saccade"]
    lanes: dict[str, Any] = {}
    started = time.monotonic()
    for lane in order:
        lanes[lane] = run_lane(
            lane, task, args.model, args.effort, runtime, runtime_dir,
            playwright_package, output_dir,
            expected_contract_hash=identity["mcp_contract_hash"] if lane == "saccade" else None,
            operation_mode=args.operation_mode,
        )
    validate_lane_order(lanes["saccade"], lanes["playwright"], args.order)
    evidence_errors = {name: lane_evidence_errors(lane) for name, lane in lanes.items()}
    control_plane_errors = [
        f"{lane}:{value.get('error', 'invalid')}"
        for lane, value in control_plane.items() if not value.get("valid")
    ]
    invalid = any(evidence_errors.values()) or bool(control_plane_errors)
    report = {
        "schema": "saccade-agent-benchmark/1",
        "task": {key: task[key] for key in ("name", "url", "success")},
        "agent": {
            "driver": "codex exec",
            "model": args.model or "codex-default-recommended",
            "effort": args.effort,
            "operation_mode": args.operation_mode,
        },
        "order": order,
        "playwright_mcp": playwright_lock,
        "saccade_contract": identity,
        "control_plane": control_plane,
        "control_plane_errors": control_plane_errors,
        "selector_or_site_execution_logic": False,
        "timing_boundary": "initial URL through browser-proven completion",
        "forbidden_routes": ["source inspection", "selector", "XPath", "DOM query", "JavaScript evaluation", "coordinate", "screenshot", "human help"],
        "lanes": lanes,
        "evidence_errors": evidence_errors,
        "verdict": "INVALID" if invalid else ("PASS" if all(lane["passed"] for lane in lanes.values()) else "FAIL"),
        "duration_seconds": round(time.monotonic() - started, 3),
    }
    report_text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    (output_dir / "report.json").write_text(
        redact_text(report_text, task["redact"]), encoding="utf-8"
    )
    print(json.dumps({"verdict": report["verdict"], "output": str(output_dir / "report.json")}))
    return 0 if report["verdict"] == "PASS" else (3 if report["verdict"] == "INVALID" else 1)


if __name__ == "__main__":
    raise SystemExit(main())
