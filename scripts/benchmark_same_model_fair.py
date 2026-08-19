#!/usr/bin/env python3
"""Same-model fair comparison: one Claude CLI drives both browser routes.

Both lanes run through the same `claude -p` binary, model, prompt, URL, goal and
success condition. Only the connected browser MCP differs:

- saccade lane:    Saccade Truth for observation + saccade.act for execution
- playwright lane: the locked official @playwright/mcp release, nothing else

Every timestamp comes from this wrapper's monotonic clock, and token counts come
from the Claude stream-json `result` usage. Any lane missing a required field
makes the run INVALID; nothing is estimated or back-filled.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import os
import selectors
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PLAYWRIGHT_LOCK = ROOT / "benchmarks/playwright-mcp.lock.json"
LANES = ("saccade", "playwright")

# Observation tools, per lane. A call to one of these before the first execution
# call counts toward discovery payload.
#
# Names are matched against the bare tool name, i.e. the segment after the
# `mcp__<server>__` prefix that the stream-json trace actually carries. Matching
# the full prefixed name would let a server segment ("claude-in-chrome") collide
# with a tool keyword ("chrome") and misclassify every tool on that server.
#
# The saccade lane counts Claude-in-Chrome page reads as observation too: reading
# a page through the execution route instead of Truth is off-route, and charging
# those bytes to the lane keeps a detour from understating Saccade's discovery.
SACCADE_OBSERVE = ("truth_read", "read_page", "get_page_text", "find")
SACCADE_NAVIGATE = ("tabs_open", "tabs_list", "tabs_close", "system_capabilities",
                    "tabs_create_mcp", "tabs_context_mcp", "tabs_close_mcp", "navigate")
PLAYWRIGHT_OBSERVE = ("snapshot", "browser_snapshot", "browser_take_snapshot", "browser_find")
# Navigation confirmations carry no page facts on either lane, so neither may
# bank them as discovery payload.
PLAYWRIGHT_NAVIGATE = ("browser_navigate", "browser_navigate_back", "browser_tabs")
# Execution tools. The first of these ends the discovery phase.
SACCADE_EXECUTE = ("act", "computer", "form_input", "browser_batch")
PLAYWRIGHT_EXECUTE = ("click", "type", "select_option", "fill", "press", "browser_click",
                      "browser_type", "browser_select_option", "browser_press_key",
                      "browser_fill_form")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


API_FAILURE_MARKERS = (
    "api error: 529",
    "api error: 500",
    "api error: 503",
    "overloaded",
    "rate_limit",
    "rate limit reached",
)


def infrastructure_failure(final_text: str) -> str | None:
    """An API-side failure is missing evidence, never a lane result.

    A model that never got a reply did not lose a browsing comparison, and
    recording it as a lane failure would silently credit the other engine.
    """
    lowered = final_text.casefold()
    if "you've hit your limit" in lowered or "you have hit your limit" in lowered:
        return "account_usage_limit"
    for marker in API_FAILURE_MARKERS:
        if marker in lowered:
            return marker
    return None


def unfenced(text: str) -> str:
    """Strip a markdown code fence around the final JSON.

    The same model fences its reply on some runs and not others. Penalising the
    lane that happened to fence would measure formatting, not browsing.
    """
    stripped = text.strip()
    if not stripped.startswith("```"):
        return stripped
    body = stripped.split("\n", 1)[1] if "\n" in stripped else ""
    return body.rsplit("```", 1)[0].strip()


def final_json(text: str) -> dict[str, Any] | None:
    """Parse the requested final object without scoring prose as browsing failure.

    Claude occasionally precedes an otherwise valid fenced JSON object with one
    sentence. That is a formatting variation, not evidence about either browser
    lane. Whole-response JSON remains preferred. Complete fenced blocks and one
    complete trailing object after harmless prose are also accepted; browser
    success still comes only from tool output, never from this object.
    """
    candidates = [unfenced(text)]
    parts = text.split("```")
    for index in range(1, len(parts), 2):
        block = parts[index].strip()
        if block.casefold().startswith("json"):
            block = block[4:].lstrip()
        candidates.append(block)
    decoder = json.JSONDecoder()
    for offset, char in enumerate(text):
        if char != "{":
            continue
        try:
            candidate, end = decoder.raw_decode(text[offset:])
        except json.JSONDecodeError:
            continue
        if text[offset + end:].strip().strip("`").strip():
            continue
        candidates.append(json.dumps(candidate))
    for candidate_text in candidates:
        try:
            candidate = json.loads(candidate_text)
        except json.JSONDecodeError:
            continue
        if (isinstance(candidate, dict)
                and isinstance(candidate.get("completed"), bool)
                and isinstance(candidate.get("summary"), str)):
            return candidate
    return None


def compact(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)


def load_task(path: Path) -> dict[str, Any]:
    task = json.loads(path.read_text(encoding="utf-8"))
    if task.get("schema") != "saccade-agent-benchmark-task/1":
        raise ValueError("unsupported benchmark task schema")
    required = {"name", "url", "task", "success", "redact", "timeout_seconds"}
    missing = required - set(task)
    if missing:
        raise ValueError(f"benchmark task is missing {sorted(missing)}")
    if not str(task["url"]).startswith(("http://", "https://")):
        raise ValueError("benchmark URL must use HTTP or HTTPS")
    needles = task["success"].get("tool_output_contains")
    if not isinstance(needles, list) or not needles or not all(isinstance(x, str) and x for x in needles):
        raise ValueError("success.tool_output_contains must be a non-empty string list")
    return task


def load_playwright_lock(path: Path = PLAYWRIGHT_LOCK) -> dict[str, Any]:
    lock = json.loads(path.read_text(encoding="utf-8"))
    if lock.get("schema") != "saccade-playwright-mcp-lock/1":
        raise ValueError("unsupported Playwright MCP lock schema")
    if lock.get("package") != "@playwright/mcp" or not isinstance(lock.get("version"), str):
        raise ValueError("Playwright MCP lock must name an exact official package version")
    if not lock["version"] or any(c in lock["version"] for c in "@*^~<>= "):
        raise ValueError("Playwright MCP lock version must be exact")
    return lock


def prompt_for(task: dict[str, Any], lane: str) -> str:
    """Identical URL, goal and success condition; only the route sentence differs."""
    if lane == "saccade":
        route = (
            "Wayne authorizes exactly one browser route for this lane: the connected Saccade MCP. "
            "Open the URL with saccade.tabs.open. If the goal names one explicitly labeled target, make the first "
            "saccade.truth.read exactly query={text:<its label words>,roles:[<narrow role>],"
            "visible_only:false,frame_scope:root,min_objects:1,max_objects:8}. If it names a section/example "
            "rather than the control label, use those exact section/example words as text with the narrow role; semantic query "
            "includes bounded nearby-heading context for each control. For a multi-field "
            "form, extract one exact visible label or placeholder phrase per required control from the task, "
            "then query text_any:[<those phrases>] with roles:[text_field,text_area,search_field,"
            "content_editable,spin_button,select,option,checkbox,radio,switch,button], "
            "visible_only:false, frame_scope:root, min_objects:<number of required matching controls>, "
            "and max_objects=32. Do not include unrelated controls merely because they share a role. For radio "
            "and checkbox targets, use the actual desired choice label (for example Male), not only its group "
            "heading (for example Gender). For a select, include both the parent label and desired option in text_any; "
            "min_objects still counts required controls. If that option is returned without click while its parent exposes "
            "select, batch select directly with option_object_id; do not click the parent first. Plan from "
            "that working_set. The text query requires every whitespace-separated word across "
            "name/text/description; use section context plus label to disambiguate duplicates. "
            "After clicking a dynamic select or menu, query the exact named "
            "option with roles:[option] and visible_only:false; do not drain ambient geometry deltas. "
            "If a non-query read returns full, plan from it. If it returns catalog, treat it as the complete compact "
            "directory, choose only task-relevant stable object_ids, and request those details once "
            "with the exact document_id and revision as basis_revision. Never fetch every object's "
            "details, resync because a catalog was returned, or repeat the initial read. Perform "
            "every action with saccade.act, naming the object_id you read from Truth. When that "
            "working_set exposes multiple independent editable/select/checkable form controls, send "
            "those planned edits once in saccade.act actions; a type action uses value, never text. "
            "Never batch submit or navigation. all_verified=true is complete proof: use "
            "next_basis_revision for the next separate action and never read queued ambient churn afterward. "
            "Never pass a coordinate, never take a screenshot, and never read the page with any "
            "other tool. If saccade.act returns external_execution_required for a control, say so "
            "and stop rather than reaching for another route. Truth delivery is automatic: every "
            "later ordinary read is only the delta from the Agent cursor, and "
            "saccade.act may return that revision-bound delta as transition. Fold each delta or "
            "transition into the cached Truth; never reread a full page. Close the Agent-owned tab "
            "through Saccade when you finish."
        )
    else:
        route = (
            "Wayne explicitly authorizes the connected Playwright MCP as the only browser route for "
            "this comparison lane. Saccade is intentionally unavailable here."
        )
    needles = ", ".join(repr(x) for x in task["success"]["tool_output_contains"])
    return f"""You are one lane in a browser-agent benchmark. Start with no knowledge of the page. {route}

URL: {task['url']}
Task: {task['task']}
Success condition: browser tool output must contain {needles}.

Do not use shell commands, web search, source inspection, selectors, XPath, DOM queries, JavaScript evaluation, raw coordinates, screenshots, or remembered site structure. Discover the page through the connected tools' normal semantic observation, operate it, then confirm the success condition from tool output. Do not ask a human for help. Reply with only a JSON object {{"completed": bool, "summary": string}}; set completed true only when tool output proves the success condition."""


def mcp_config(lane: str, runtime: Path, runtime_dir: Path, playwright_package: str,
               browser: str = "chrome") -> str:
    if lane == "saccade":
        servers = {
            "saccade": {
                "command": str(runtime),
                "args": ["mcp"],
                "env": {
                    "SACCADE_RUNTIME_DIR": str(runtime_dir),
                    "SACCADE_BENCHMARK_FRESH_INPUT_POLICY": "1",
                },
            }
        }
    else:
        playwright_browser = "msedge" if browser == "edge" else "chrome"
        servers = {
            "playwright": {
                "command": "npx",
                "args": ["-y", playwright_package, "--headless", "--browser", playwright_browser,
                         "--isolated", "--image-responses", "omit"],
            }
        }
    return json.dumps({"mcpServers": servers}, separators=(",", ":"))


def lane_command(lane: str, task: dict[str, Any], model: str | None, runtime: Path,
                 runtime_dir: Path, playwright_package: str,
                 effort: str | None = None, browser: str = "chrome") -> list[str]:
    allowed_mcp = f"mcp__{lane}__*"
    command = [
        "claude", "-p", prompt_for(task, lane),
        "--output-format", "stream-json", "--verbose",
        "--no-session-persistence", "--strict-mcp-config",
        "--mcp-config", mcp_config(lane, runtime, runtime_dir, playwright_package, browser),
        "--permission-mode", "auto",
        "--allowedTools", allowed_mcp,
        "--disallowedTools", "Bash,WebFetch,WebSearch,Read,Write,Edit,Glob,Grep",
    ]
    # Neither lane uses a browser client: Saccade executes through saccade.act
    # and Playwright executes through its own tools. In particular, the Saccade
    # lane never enables Claude-in-Chrome, --chrome, or the tab-claim route.
    if model:
        command.extend(["--model", model])
    # Both lanes must run at an identical effort level or the comparison is unfair.
    if effort:
        command.extend(["--effort", effort])
    return command


def bare_tool_name(tool: str) -> str:
    """Drop the `mcp__<server>__` prefix and normalise separators.

    The trace records `mcp__saccade__saccade_truth_read`, not `truth.read`, and the
    server segment must not participate in keyword matching.
    """
    name = tool.rsplit("__", 1)[-1] if "__" in tool else tool
    return name.casefold().replace(".", "_").replace("-", "_")


def classify(tool: str, lane: str) -> str:
    name = bare_tool_name(tool)
    observe = SACCADE_OBSERVE if lane == "saccade" else PLAYWRIGHT_OBSERVE
    execute = SACCADE_EXECUTE if lane == "saccade" else PLAYWRIGHT_EXECUTE
    navigate = SACCADE_NAVIGATE if lane == "saccade" else PLAYWRIGHT_NAVIGATE
    # Navigation first: it carries no page facts and must never be read as either
    # observation payload or an action that closes the discovery window.
    if any(word in name for word in navigate):
        return "navigate"
    if any(word in name for word in observe):
        return "observe"
    if any(word in name for word in execute):
        return "execute"
    return "other"


def view_mode_of(payload: Any) -> str | None:
    if isinstance(payload, dict):
        if isinstance(payload.get("query"), dict):
            return "working_set"
        for key in ("view_mode", "mode"):
            if isinstance(payload.get(key), str):
                return payload[key]
    return None



# The trace keeps no tool inputs or bodies, so the claim route would leave no
# same-tab proof. Retain only the few fields that prove it, and never the full
# single-use claim token.
CLAIM_PROOF_TOOLS = ("tabs_open", "tabs_list")


def claim_input_digest(payload: Any) -> dict[str, Any] | None:
    if not isinstance(payload, dict):
        return None
    digest: dict[str, Any] = {}
    if isinstance(payload.get("claim"), str):
        digest["claim"] = payload["claim"]
    for key in ("tab_id", "tabId"):
        if payload.get(key) is not None:
            digest["tab_id"] = str(payload[key])
    if isinstance(payload.get("claim_id"), str):
        digest["claim_id_prefix"] = payload["claim_id"][:16] + "\u2026"
    return digest or None


def claim_result_digest(tool: str, content: Any) -> dict[str, Any] | None:
    name = bare_tool_name(tool)
    if not any(word in name for word in CLAIM_PROOF_TOOLS):
        return None
    text = compact(content)
    keep = {}
    for field in ("claim", "provenance", "ownership", "observation_ready", "tab_id", "opened"):
        match = re.search(rf'"{field}":("[^"]*"|true|false)', text)
        if match:
            keep[field] = match.group(1).strip('"')
    return keep or None


def carries_truth_transition(tool: str, content: Any) -> bool:
    """Whether an action response delivered its revision-bound observation inline."""
    if bare_tool_name(tool) != "saccade_act":
        return False
    def readable(value: Any) -> str:
        if isinstance(value, str):
            return value
        if isinstance(value, list):
            return "\n".join(readable(item) for item in value)
        if isinstance(value, dict):
            text = value.get("text")
            if isinstance(text, str):
                return text
        return compact(value)

    body = readable(content)
    return (re.search(r'"transition"\s*:', body) is not None
            and re.search(r'"mode"\s*:\s*"(?:delta|full)"', body) is not None)


def route_proof(trace: list[dict[str, Any]]) -> dict[str, Any]:
    """Prove the Saccade lane observed with Truth and executed with saccade.act.

    The lane is only an engine comparison if nothing else touched the page, so
    any client browser tool, screenshot or coordinate makes it unusable rather
    than merely slower.
    """
    acts = [c for c in trace if bare_tool_name(c["tool"]) == "saccade_act"]
    truth_reads = [c for c in trace if "truth_read" in bare_tool_name(c["tool"])]
    foreign = sorted({c["tool"] for c in trace if "claude-in-chrome" in c["tool"]
                      or "playwright" in c["tool"]})
    return {
        "observed_with_truth": bool(truth_reads),
        "executed_with_saccade_act": bool(acts),
        "act_calls": len(acts),
        "foreign_browser_tools": foreign,
        "screenshot_used": any("screenshot" in bare_tool_name(c["tool"]) for c in trace),
        "pure_engine_route": bool(truth_reads) and bool(acts) and not foreign,
    }


def claim_proof(trace: list[dict[str, Any]]) -> dict[str, Any]:
    """Prove arm -> client_create -> navigate -> confirm on one tab, from the trace."""
    armed = confirmed = listed = None
    created_tab = executed_tabs = None
    executed = set()
    for call in trace:
        name = bare_tool_name(call["tool"])
        ev = call.get("claim_evidence") or {}
        res = call.get("claim_result") or {}
        if "tabs_open" in name and ev.get("claim") == "arm":
            armed = call["sequence"]
        if "tabs_open" in name and ev.get("claim") == "confirm" and res.get("claim") == "confirmed":
            confirmed = {"sequence": call["sequence"], "tab_id": res.get("tab_id"),
                         "provenance": res.get("provenance"), "opened": res.get("opened"),
                         "claim_id_prefix": ev.get("claim_id_prefix")}
        if "tabs_create_mcp" in name:
            created_tab = call["sequence"]
        if "tabs_list" in name and res.get("provenance") == "agent_client":
            listed = {"provenance": res.get("provenance"), "ownership": res.get("ownership"),
                      "observation_ready": res.get("observation_ready")}
        if call.get("role") == "execute" and ev.get("tab_id"):
            executed.add(ev["tab_id"])
        if "truth_read" in name and ev.get("tab_id"):
            executed_tabs = ev["tab_id"]
    claimless = any("tabs_open" in bare_tool_name(c["tool"])
                    and not (c.get("claim_evidence") or {}).get("claim") for c in trace)
    tab = (confirmed or {}).get("tab_id")
    return {"armed": armed is not None, "client_created_tab": created_tab is not None,
            "confirmed": confirmed, "tabs_list_proof": listed,
            "claimless_tabs_open_used": claimless,
            "observed_tab_id": executed_tabs, "executed_tab_ids": sorted(executed),
            "same_tab": bool(tab) and executed_tabs == tab and executed <= {tab}}


def trace_events(stdout: str | list[tuple[float, str]], lane: str, started: float) -> dict[str, Any]:
    """Timestamp every tool request and return against the wrapper monotonic clock."""
    calls: dict[str, dict[str, Any]] = {}
    order: list[str] = []
    usage: dict[str, Any] = {}
    # Success must be proven by tool output. The model's own final JSON is never
    # evidence, so the needle search runs over tool_result bodies only.
    tool_output_chunks: list[str] = []
    final_text = ""
    result_subtype = None
    is_error = None
    lines = ([(round((time.monotonic() - started) * 1000, 3), line)
              for line in stdout.splitlines()] if isinstance(stdout, str) else stdout)
    for stamp, line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(event, dict):
            continue
        kind = event.get("type")
        message = event.get("message") or {}
        if kind == "assistant":
            for block in message.get("content") or []:
                if isinstance(block, dict) and block.get("type") == "tool_use":
                    identifier = str(block.get("id") or f"call-{len(order)}")
                    calls[identifier] = {
                        "sequence": len(order) + 1,
                        "tool": str(block.get("name") or "unknown_tool"),
                        "requested_ms": stamp,
                        "returned_ms": None,
                        "duration_ms": None,
                        "response_bytes": None,
                        "view_mode": view_mode_of(block.get("input")),
                        "claim_evidence": claim_input_digest(block.get("input")),
                    }
                    order.append(identifier)
        elif kind == "user":
            for block in message.get("content") or []:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    identifier = str(block.get("tool_use_id") or "")
                    call = calls.get(identifier)
                    if call is None:
                        continue
                    call["returned_ms"] = stamp
                    call["duration_ms"] = round(stamp - call["requested_ms"], 3)
                    call["response_bytes"] = len(compact(block.get("content")).encode())
                    call["claim_result"] = claim_result_digest(call["tool"], block.get("content"))
                    call["in_response_truth_transition"] = carries_truth_transition(
                        call["tool"], block.get("content")
                    )
                    tool_output_chunks.append(compact(block.get("content")))
        elif kind == "result":
            usage = event.get("usage") if isinstance(event.get("usage"), dict) else {}
            final_text = str(event.get("result") or "")
            result_subtype = event.get("subtype")
            is_error = event.get("is_error")

    trace = [calls[i] for i in order]
    for call in trace:
        call["role"] = classify(call["tool"], lane)
    return {"trace": trace, "usage": usage, "final_text": final_text,
            "tool_output": "\n".join(tool_output_chunks),
            "result_subtype": result_subtype, "result_is_error": is_error}


def run_streaming(command: list[str], timeout: int, environment: dict[str, str],
                  started: float) -> tuple[str, str, int, bool, list[tuple[float, str]]]:
    """Read JSONL as it arrives so monotonic stamps describe real event delivery."""
    chunks: list[str] = []
    stamped: list[tuple[float, str]] = []
    pending = b""
    timed_out = False

    def consume(data: bytes) -> None:
        nonlocal pending
        pending += data
        while b"\n" in pending:
            raw, pending = pending.split(b"\n", 1)
            line = raw.decode(errors="replace")
            chunks.append(line + "\n")
            stamped.append((round((time.monotonic() - started) * 1000, 3), line))

    with tempfile.TemporaryFile(mode="w+") as stderr_file:
        process = subprocess.Popen(
            command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
            stderr=stderr_file, env=environment,
        )
        assert process.stdout is not None
        os.set_blocking(process.stdout.fileno(), False)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        deadline = started + timeout
        try:
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    timed_out = True
                    process.kill()
                    break
                ready = selector.select(min(0.1, remaining))
                if ready:
                    try:
                        data = os.read(process.stdout.fileno(), 65536)
                    except BlockingIOError:
                        continue
                    if not data:
                        break
                    consume(data)
                elif process.poll() is not None:
                    while True:
                        try:
                            data = os.read(process.stdout.fileno(), 65536)
                        except BlockingIOError:
                            break
                        if not data:
                            break
                        consume(data)
                    break
            if pending:
                line = pending.decode(errors="replace")
                chunks.append(line)
                stamped.append((round((time.monotonic() - started) * 1000, 3), line))
            returncode = process.wait()
        finally:
            selector.close()
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdout.close()
        stderr_file.seek(0)
        stderr = stderr_file.read()
    if timed_out:
        stderr += f"\nlane timed out after {timeout} seconds\n"
        returncode = 124
    return "".join(chunks), stderr, returncode, timed_out, stamped


def discovery_bytes(trace: list[dict[str, Any]]) -> dict[str, Any]:
    """Every observation payload consumed before the first executable action.

    This deliberately accumulates an index read plus every region read, not just
    the smallest single response, so a cheap index cannot understate discovery.
    """
    total = 0
    reads = 0
    modes: list[str] = []
    for call in trace:
        if call["role"] == "execute":
            break
        if call["role"] == "observe":
            total += call["response_bytes"] or 0
            reads += 1
            if call["view_mode"]:
                modes.append(call["view_mode"])
    return {
        "initial_transfer_bytes": total or None,
        "discovery_observation_calls": reads,
        "discovery_view_modes": modes,
    }


def delta_latencies(trace: list[dict[str, Any]]) -> list[float]:
    """Execution tool return → the next observation call's return."""
    latencies = []
    for index, call in enumerate(trace):
        if call["role"] != "execute" or call["returned_ms"] is None:
            continue
        for follower in trace[index + 1:]:
            if follower["role"] == "observe":
                if follower["returned_ms"] is not None:
                    latencies.append(round(follower["returned_ms"] - call["returned_ms"], 3))
                break
    return latencies


def logical_input_tokens(usage: dict[str, Any]) -> int | None:
    """All input tokens the model consumed, including cache reads and writes."""
    fields = ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")
    values = [usage.get(field, 0) for field in fields]
    if not all(isinstance(value, int) and value >= 0 for value in values):
        return None
    total = sum(values)
    return total or None


def run_lane(lane: str, task: dict[str, Any], model: str | None, runtime: Path,
             runtime_dir: Path, playwright_package: str, output_dir: Path,
             effort: str | None = None, browser: str = "chrome") -> dict[str, Any]:
    command = lane_command(
        lane, task, model, runtime, runtime_dir, playwright_package, effort, browser
    )
    started_at = utc_now()
    started = time.monotonic()
    stdout, stderr, returncode, timed_out, stamped = run_streaming(
        command, int(task["timeout_seconds"]), os.environ.copy(), started,
    )
    elapsed_ms = round((time.monotonic() - started) * 1000, 3)
    completed_at = utc_now()

    parsed = trace_events(stamped, lane, started)
    trace = parsed["trace"]
    redactions = task["redact"]
    (output_dir / f"{lane}.jsonl").write_text(redact_text(stdout, redactions), encoding="utf-8")
    (output_dir / f"{lane}.stderr.log").write_text(redact_text(stderr, redactions), encoding="utf-8")

    final = final_json(parsed["final_text"])
    if final is None:
        final = {"completed": False, "summary": parsed["final_text"][:500]}

    # Previously this searched compact(trace), which carries only per-call metadata
    # and never tool bodies, so no lane could ever prove its success condition.
    tool_blob = parsed["tool_output"].casefold()
    evidence = {n: n.casefold() in tool_blob for n in task["success"]["tool_output_contains"]}
    observe_calls = sum(1 for c in trace if c["role"] == "observe")
    latencies = delta_latencies(trace)
    inline_transition_latencies = [
        call["duration_ms"] for call in trace
        if call.get("in_response_truth_transition")
        and isinstance(call.get("duration_ms"), (int, float))
    ]
    usage = parsed["usage"] or {}
    failure_reason = final.get("failure_reason")
    if not trace and not timed_out:
        diagnostic = f"{parsed['final_text']}\n{stderr}".casefold()
        if "not logged in" in diagnostic or "please run /login" in diagnostic:
            failure_reason = failure_reason or "claude_cli_not_authenticated"
        elif "you've hit your limit" in diagnostic or "you have hit your limit" in diagnostic:
            failure_reason = failure_reason or "claude_account_usage_limit"
        else:
            failure_reason = failure_reason or "browser_mcp_unavailable_no_tool_calls"

    metrics = {
        **discovery_bytes(trace),
        "observe_or_snapshot_calls": observe_calls,
        "post_initial_reobservation_calls": max(0, observe_calls - 1),
        # An inline act transition is already present when the action returns,
        # so return-to-observation latency is exactly zero and no read follows.
        "action_return_to_delta_read_ms": (
            0.0 if inline_transition_latencies else min(latencies) if latencies else None
        ),
        "delta_latency_samples_ms": latencies,
        "in_response_transition_latency_ms": inline_transition_latencies,
        "in_response_transition_count": len(inline_transition_latencies),
        "latency_measurement_status": "wrapper_monotonic_single_process",
        "dynamic_replacement_recoveries": tool_blob.count("stale_before_dispatch")
                                          + tool_blob.count("replacement_recovered"),
        "stale_events": tool_blob.count("stale"),
        "transcript_bytes": sum(c["response_bytes"] or 0 for c in trace),
        "model_logical_input_tokens": logical_input_tokens(usage),
        "trace": trace,
    }
    return {
        "lane": lane,
        "passed": (not timed_out and returncode == 0
                   and final.get("completed") is True and all(evidence.values())),
        "elapsed_ms": elapsed_ms,
        "returncode": returncode,
        "timed_out": timed_out,
        "usage": usage,
        "infrastructure_failure": infrastructure_failure(parsed["final_text"]),
        "route_proof": route_proof(trace) if lane == "saccade" else None,
        "tool_calls": len(trace),
        "browser_metrics": metrics,
        "success_evidence": evidence,
        "final": final,
        "failure_reason": failure_reason,
        "timing": {"started_at": started_at, "completed_at": completed_at,
                   "clock_source": "wrapper_monotonic", "elapsed_ms": elapsed_ms},
        "stderr_tail": redact_text(stderr, redactions)[-2000:],
    }


def redact_text(text: str, values: list[str]) -> str:
    import urllib.parse
    for value in sorted(values, key=len, reverse=True):
        variants = {value, value.replace("\n", " "), value.replace("\n", "\\n"),
                    value.replace("\n", "\r\n"),
                    urllib.parse.quote(value, safe=""), urllib.parse.quote_plus(value, safe=""),
                    urllib.parse.quote(value.replace("\n", "\r\n"), safe="")}
        for variant in sorted(variants, key=len, reverse=True):
            text = text.replace(variant, "[REDACTED_EDITABLE]")
    return text


def lane_evidence_errors(lane: dict[str, Any]) -> list[str]:
    errors = []
    if lane.get("infrastructure_failure"):
        # Do not also report every downstream missing field: the run produced no
        # evidence because the API failed, and that is the whole finding.
        return [f"infrastructure_failure:{lane['infrastructure_failure']}"]
    timing = lane.get("timing") or {}
    if timing.get("clock_source") != "wrapper_monotonic":
        errors.append("wrapper_monotonic_clock_missing")
    if not isinstance(timing.get("elapsed_ms"), (int, float)) or timing.get("elapsed_ms", 0) <= 0:
        errors.append("end_to_end_elapsed_ms_missing")
    usage = lane.get("usage") or {}
    metrics = lane.get("browser_metrics") or {}
    logical_tokens = metrics.get("model_logical_input_tokens") or logical_input_tokens(usage)
    if not isinstance(logical_tokens, int) or logical_tokens <= 0:
        errors.append("model_input_tokens_missing")
    if not isinstance(usage.get("output_tokens"), int) or usage.get("output_tokens", 0) <= 0:
        errors.append("model_output_tokens_missing")
    if not isinstance(metrics.get("initial_transfer_bytes"), int) or metrics["initial_transfer_bytes"] <= 0:
        errors.append("discovery_transfer_bytes_missing")
    if not isinstance(lane.get("tool_calls"), int) or lane.get("tool_calls", 0) <= 0:
        errors.append("tool_call_count_missing")
    if not isinstance(metrics.get("action_return_to_delta_read_ms"), (int, float)):
        errors.append("delta_latency_missing")
    if not isinstance(metrics.get("dynamic_replacement_recoveries"), int):
        errors.append("dynamic_replacement_recovery_count_missing")
    proof = lane.get("route_proof")
    if proof is not None:
        # Saccade lane only. Without Truth observation and saccade.act execution
        # this is not the engine comparison it claims to be.
        if not proof.get("observed_with_truth"):
            errors.append("truth_observation_missing")
        if not proof.get("executed_with_saccade_act"):
            errors.append("saccade_act_execution_missing")
        if proof.get("foreign_browser_tools"):
            errors.append("foreign_browser_tool_used")
        if proof.get("screenshot_used"):
            errors.append("screenshot_used")
    return errors


def validate_order(lanes: dict[str, Any], order: str) -> list[str]:
    first, second = (("saccade", "playwright") if order == "saccade-first"
                     else ("playwright", "saccade"))
    end = dt.datetime.fromisoformat(lanes[first]["timing"]["completed_at"].replace("Z", "+00:00"))
    start = dt.datetime.fromisoformat(lanes[second]["timing"]["started_at"].replace("Z", "+00:00"))
    return [] if end <= start else [f"lane timestamps do not prove {order}"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", help="one model id used identically by both lanes")
    parser.add_argument("--effort", help="one effort level used identically by both lanes")
    parser.add_argument("--browser", choices=("chrome", "edge"), default="chrome",
                        help="browser family used identically by both lanes")
    parser.add_argument("--order", choices=("saccade-first", "playwright-first"),
                        default="saccade-first")
    parser.add_argument("--only-lane", choices=LANES,
                        help="diagnostic: run a single lane. No cross-lane comparison is "
                             "produced and order validation is skipped.")
    args = parser.parse_args()

    lock = load_playwright_lock()
    playwright_package = f"{lock['package']}@{lock['version']}"
    task = load_task(args.task.resolve())
    output_dir = args.output.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    sequence = (["saccade", "playwright"] if args.order == "saccade-first"
                else ["playwright", "saccade"])
    if args.only_lane:
        sequence = [args.only_lane]
    started = time.monotonic()
    lanes = {
        lane: run_lane(lane, task, args.model, args.runtime.resolve(),
                       args.runtime_dir.resolve(), playwright_package, output_dir,
                       args.effort, args.browser)
        for lane in sequence
    }

    evidence_errors = {name: lane_evidence_errors(lane) for name, lane in lanes.items()}
    order_errors = [] if args.only_lane else validate_order(lanes, args.order)
    invalid = any(evidence_errors.values()) or bool(order_errors)
    report = {
        "schema": "saccade-same-model-benchmark/1",
        "task": {key: task[key] for key in ("name", "url", "success")},
        "agent": {"driver": "claude -p", "model": args.model or "claude-cli-default",
                  "same_model_both_lanes": True},
        "order": sequence,
        "playwright_mcp": lock,
        "browser": args.browser,
        "routes": {"saccade": "saccade MCP truth + saccade.act execution",
                   "playwright": f"{playwright_package} semantic tools only"},
        "single_lane_diagnostic": args.only_lane,
        "comparison_authorized": args.only_lane is None,
        "selector_or_site_execution_logic": False,
        "timing_boundary": "initial URL through browser-proven success condition",
        "lanes": lanes,
        "evidence_errors": evidence_errors,
        "order_errors": order_errors,
        "verdict": "INVALID" if invalid else ("PASS" if all(l["passed"] for l in lanes.values()) else "FAIL"),
        "duration_seconds": round(time.monotonic() - started, 3),
    }
    text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    (output_dir / "report.json").write_text(redact_text(text, task["redact"]), encoding="utf-8")
    print(json.dumps({"verdict": report["verdict"], "output": str(output_dir / "report.json")}))
    return {"PASS": 0, "FAIL": 1, "INVALID": 3}[report["verdict"]]


if __name__ == "__main__":
    raise SystemExit(main())
