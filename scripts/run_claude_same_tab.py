#!/usr/bin/env python3
"""Run one value-free Claude Chrome + Saccade same-tab fixture dogfood.

Claude uses Saccade's provisioned claim so its Chrome tool can create the tab
it acts on while the same MCP session observes and verifies the transition.
"""

from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


def utc_now() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


class SaccadeMcp:
    """Minimal stdio client for the runtime's normal MCP protocol.

    Nothing here bypasses the protocol: no internal HTTP, no database, no
    private surface. It speaks exactly the published Saccade tools.
    """

    def __init__(self, runtime: Path, runtime_dir: Path, timeout: float = 35.0) -> None:
        environment = os.environ.copy()
        environment["SACCADE_RUNTIME_DIR"] = str(runtime_dir)
        self.timeout = timeout
        self.next_id = 1
        self.process = subprocess.Popen(
            [str(runtime), "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, bufsize=1, env=environment,
        )
        try:
            self.rpc("initialize", {})
        except Exception:
            self.close()
            raise

    def rpc(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        assert self.process.stdin is not None and self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        self.process.stdin.write(
            json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method,
                        "params": params}) + "\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + self.timeout
        while True:
            ready, _, _ = select.select(
                [self.process.stdout], [], [], max(0.0, deadline - time.monotonic()))
            if not ready:
                raise RuntimeError(f"Saccade MCP timed out during {method}")
            line = self.process.stdout.readline()
            if not line:
                stderr = self.process.stderr.read() if self.process.stderr else ""
                raise RuntimeError(f"Saccade MCP exited during {method}: {stderr.strip()}")
            response = json.loads(line)
            if response.get("id") == request_id:
                break
        if "error" in response:
            raise RuntimeError(str(response["error"].get("message", response["error"])))
        return response.get("result") or {}

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.rpc("tools/call", {"name": name, "arguments": arguments})
        # The runtime carries the payload in structuredContent; the text block is
        # only the `saccade.result` marker.
        if isinstance(result.get("structuredContent"), dict):
            return result["structuredContent"]
        for block in result.get("content") or []:
            if isinstance(block, dict) and block.get("type") == "text":
                try:
                    parsed = json.loads(block["text"])
                except json.JSONDecodeError:
                    continue
                if isinstance(parsed, dict):
                    return parsed
        return result

    def close(self) -> None:
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            try:
                if stream is not None:
                    stream.close()
            except OSError:
                pass
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait()

    def __enter__(self) -> SaccadeMcp:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


TOGGLE_NAME = "Toggle signal"


def revision_of(view: dict[str, Any]) -> int | None:
    for key in ("revision", "basis_revision"):
        value = view.get(key)
        if isinstance(value, int):
            return value
    observation = view.get("observation")
    if isinstance(observation, dict) and isinstance(observation.get("revision"), int):
        return observation["revision"]
    return None


def pressed_state(view: dict[str, Any], name: str = TOGGLE_NAME) -> str | None:
    """The toggle button's observed `pressed` state, or None if not observed.

    Revision alone cannot prove the click landed: this fixture pushes its own
    `Browser cycle` status updates, so revision advances with no action at all.
    Only a `pressed` transition is evidence that Claude's click executed.
    """
    for obj in view.get("objects") or []:
        if not isinstance(obj, dict) or obj.get("role") != "button":
            continue
        if obj.get("name") != name:
            continue
        state = obj.get("state")
        if isinstance(state, dict) and state.get("pressed") is not None:
            return str(state["pressed"])
    return None


def tabs_with_url(mcp: SaccadeMcp, url: str) -> list[str]:
    listing = mcp.tool("saccade.tabs.list", {})
    return [str(tab.get("tab_id")) for tab in listing.get("tabs") or []
            if isinstance(tab, dict) and tab.get("url") == url]


def command(
    claude: Path,
    runtime: Path,
    runtime_dir: Path,
    url: str,
    model: str | None = None,
    effort: str | None = None,
) -> list[str]:
    config = json.dumps({
        "mcpServers": {
            "saccade": {
                "command": str(runtime),
                "args": ["mcp"],
                "env": {"SACCADE_RUNTIME_DIR": str(runtime_dir)},
            }
        }
    }, separators=(",", ":"))
    prompt = f"""Run one Saccade same-browser, same-tab closed loop on this harmless local fixture: {url}

Use only the Saccade MCP and your Claude in Chrome tools. Keep the entire loop in this one session. Use Saccade's provisioned claim exactly as described below; do not call the normal Saccade-created-tab form of tabs.open and do not open a second copy of the URL.

Do this in order:
1. Call saccade.system.capabilities.
2. Arm a claim by calling saccade.tabs.open with url={url} and claim="arm". Record the returned claim_id. This must not create a tab.
3. Call your Claude in Chrome tabs_context_mcp with createIfEmpty=true, then use Claude in Chrome to create exactly one new tab at {url}. Record the numeric Chrome tab id it returns.
4. Confirm that exact tab by calling saccade.tabs.open with url={url}, claim="confirm", the claim_id, and tab_id set to the Chrome tab id. Do not continue unless it returns claim="confirmed" and the same tab_id.
5. Read one full Saccade Truth view of that tab and record the revision and Toggle signal button's pressed state.
6. Using your own Claude in Chrome tool, find and click the visible Toggle signal button in that exact tab.
7. Read a revision-bounded Saccade delta from the initial revision and verify the button's pressed state changed.
8. Call saccade.tabs.list and verify exactly one tab has the target URL, then close the claimed tab with saccade.tabs.close.

Do not use Bash, web search, screenshots, source inspection, DOM queries, JavaScript, selectors, coordinates, Playwright, or the Reference Actuator.

Return only JSON with completed, browser_instance_id, tab_id, execution_tab_id, claim_armed, claim_confirmed, initial_revision, final_revision, initial_pressed, final_pressed, target_url_tab_count, tab_closed, tabs_context, and summary. Set completed true only if the claim was armed and confirmed, your Chrome action actually executed on that same claimed tab, Saccade observed the pressed-state change, there was exactly one target URL tab, and you closed it."""
    result = [
        str(claude), "-p", prompt, "--output-format", "stream-json", "--verbose",
        "--no-session-persistence", "--chrome", "--strict-mcp-config",
        "--mcp-config", config, "--permission-mode", "auto",
        "--disallowedTools", "Bash,WebFetch,WebSearch,mcp__saccade__saccade_act",
    ]
    if model:
        result.extend(["--model", model])
    if effort:
        result.extend(["--effort", effort])
    return result


def tool_names(events: list[dict[str, Any]]) -> list[str]:
    names = []
    for event in events:
        message = event.get("message") or {}
        for block in message.get("content") or []:
            if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name"):
                names.append(str(block["name"]))
    return names


def parse_result_answer(value: object) -> dict[str, Any]:
    """Parse Claude's JSON answer, including a short preface and JSON fence."""
    result_text = str(value or "").strip()
    fence = result_text.find("```json")
    if fence >= 0:
        fence_end = result_text.find("```", fence + 7)
        if fence_end >= 0:
            result_text = result_text[fence + 7:fence_end].strip()
    try:
        parsed = json.loads(result_text)
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


# Tools that act on or read a page. Tab-management calls carry a tabId too, but
# closing a scratch tab is not execution and must not count toward same-tab proof.
CHROME_PAGE_TOOLS = ("computer", "find", "read_page", "get_page_text",
                     "form_input", "browser_batch", "javascript_tool")


def chrome_execution_tab_ids(events: list[dict[str, Any]]) -> list[str]:
    """Every tabId Claude in Chrome was actually asked to act on or read."""
    seen = []
    for event in events:
        message = event.get("message") or {}
        for block in message.get("content") or []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = str(block.get("name") or "").casefold()
            if "chrome" not in name:
                continue
            if not any(tool in name for tool in CHROME_PAGE_TOOLS):
                continue
            arguments = block.get("input")
            if isinstance(arguments, dict) and arguments.get("tabId") is not None:
                seen.append(str(arguments["tabId"]))
    return seen


def chrome_action_tab_ids(events: list[dict[str, Any]]) -> list[str]:
    """Tab ids for Chrome calls capable of changing the page."""
    action_tools = ("computer", "form_input", "browser_batch")
    seen = []
    for event in events:
        message = event.get("message") or {}
        for block in message.get("content") or []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = str(block.get("name") or "").casefold()
            if "chrome" not in name or not any(tool in name for tool in action_tools):
                continue
            arguments = block.get("input")
            if isinstance(arguments, dict) and arguments.get("tabId") is not None:
                seen.append(str(arguments["tabId"]))
    return seen


def saccade_claim_modes(events: list[dict[str, Any]]) -> list[str]:
    """Claim modes Claude actually sent through the public tabs.open tool."""
    modes = []
    for event in events:
        message = event.get("message") or {}
        for block in message.get("content") or []:
            if not isinstance(block, dict) or block.get("type") != "tool_use":
                continue
            name = str(block.get("name") or "").casefold()
            arguments = block.get("input")
            if "saccade" not in name or "tabs_open" not in name or not isinstance(arguments, dict):
                continue
            if arguments.get("claim") in {"arm", "confirm"}:
                modes.append(str(arguments["claim"]))
    return modes


def chrome_tool_failures(events: list[dict[str, Any]]) -> list[str]:
    """Error text from Claude in Chrome results, kept verbatim for diagnosis."""
    chrome_calls = set()
    failures = []
    for event in events:
        message = event.get("message") or {}
        for block in message.get("content") or []:
            if not isinstance(block, dict):
                continue
            if block.get("type") == "tool_use" and "chrome" in str(block.get("name") or "").casefold():
                chrome_calls.add(str(block.get("id")))
            if block.get("type") == "tool_result" and block.get("is_error"):
                if str(block.get("tool_use_id")) in chrome_calls:
                    failures.append(json.dumps(block.get("content"))[:400])
    return failures


def run(args: argparse.Namespace) -> dict[str, Any]:
    opened_tab_id: str | None = None
    initial_revision: int | None = None
    final_revision: int | None = None
    initial_pressed: str | None = None
    final_pressed: str | None = None
    duplicate_target_tabs: list[str] = []
    setup_error: str | None = None
    completed: subprocess.CompletedProcess[str] | None = None
    started_at = utc_now()
    started = time.perf_counter()

    try:
        completed = subprocess.run(
            command(
                args.claude,
                args.runtime,
                args.runtime_dir,
                args.url,
                args.model,
                args.effort,
            ),
            capture_output=True, text=True, timeout=args.timeout, check=False,
        )
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or ""
        stderr = error.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", errors="replace")
        completed = subprocess.CompletedProcess(error.cmd, 124, stdout, stderr)
        setup_error = f"TimeoutExpired: Claude exceeded {args.timeout} seconds"
    except Exception as error:  # noqa: BLE001 - recorded as evidence, never swallowed
        setup_error = f"{type(error).__name__}: {error}"
    finally:
        pass

    elapsed_ms = round((time.perf_counter() - started) * 1000, 3)
    events = []
    for line in (completed.stdout if completed else "").splitlines():
        try:
            value = json.loads(line)
            if isinstance(value, dict):
                events.append(value)
        except json.JSONDecodeError:
            continue
    result_events = [event for event in events if event.get("type") == "result"]
    result = result_events[-1] if result_events else {}
    answer = parse_result_answer(result.get("result"))

    opened_tab_id = str(answer.get("tab_id") or "") or None
    initial_revision = answer.get("initial_revision") if isinstance(answer.get("initial_revision"), int) else None
    final_revision = answer.get("final_revision") if isinstance(answer.get("final_revision"), int) else None
    initial_pressed = str(answer["initial_pressed"]) if answer.get("initial_pressed") is not None else None
    final_pressed = str(answer["final_pressed"]) if answer.get("final_pressed") is not None else None
    target_url_tab_count = answer.get("target_url_tab_count")

    names = tool_names(events)
    used_saccade = any("saccade" in name.casefold() for name in names)
    used_chrome = any("chrome" in name.casefold() for name in names)
    used_saccade_act = any("saccade_act" in name.casefold() for name in names)
    execution_tab_ids = chrome_execution_tab_ids(events)
    action_tab_ids = chrome_action_tab_ids(events)
    claim_modes = saccade_claim_modes(events)
    chrome_failures = chrome_tool_failures(events)
    same_tab = bool(opened_tab_id) and bool(execution_tab_ids) and all(
        tab == opened_tab_id for tab in execution_tab_ids)
    # A revision bump is not evidence — the fixture pushes its own status updates.
    # Only a `pressed` transition proves Claude's click reached this tab.
    observed_change = (initial_pressed is not None and final_pressed is not None
                       and initial_pressed != final_pressed)

    evidence = {
        "schema": "saccade.claude-same-tab-dogfood/2",
        "client": "claude-code",
        "client_version": subprocess.run(
            [str(args.claude), "--version"], capture_output=True, text=True, check=False
        ).stdout.strip(),
        "timing": {
            "started_at": started_at,
            "completed_at": utc_now(),
            "clock_source": "python.perf_counter",
            "elapsed_ms": elapsed_ms,
        },
        "tab_preopened_before_claude_started": False,
        "saccade_tab_id": opened_tab_id,
        "claude_execution_tab_ids": execution_tab_ids,
        "claude_action_tab_ids": action_tab_ids,
        "same_tab": same_tab,
        "duplicate_target_tabs": duplicate_target_tabs,
        "target_url_tab_count": target_url_tab_count,
        "tab_closed": answer.get("tab_closed") is True,
        "claim_modes": claim_modes,
        "claim_armed": answer.get("claim_armed") is True,
        "claim_confirmed": answer.get("claim_confirmed") is True,
        "initial_revision": initial_revision,
        "final_revision": final_revision,
        "initial_pressed": initial_pressed,
        "final_pressed": final_pressed,
        "saccade_observed_change": observed_change,
        "change_evidence": "toggle_button_pressed_transition",
        "returncode": completed.returncode if completed else None,
        "usage": result.get("usage") or {},
        "result_subtype": result.get("subtype"),
        "result_is_error": result.get("is_error"),
        "result_text": str(result.get("result") or "")[:2000],
        "tool_names": names,
        "chrome_tool_failures": chrome_failures,
        "saccade_observation_used": used_saccade,
        "saccade_act_used": used_saccade_act,
        "claude_chrome_execution_used": used_chrome,
        "setup_error": setup_error,
        "answer": answer,
        "stderr_tail": (completed.stderr[-1000:] if completed else ""),
    }
    evidence["passed"] = bool(
        setup_error is None
        and completed is not None and completed.returncode == 0
        and answer.get("completed") is True
        and claim_modes == ["arm", "confirm"]
        and answer.get("claim_armed") is True
        and answer.get("claim_confirmed") is True
        and used_saccade and used_chrome
        and not used_saccade_act
        and same_tab
        and bool(action_tab_ids)
        and all(tab == opened_tab_id for tab in action_tab_ids)
        and not chrome_failures
        and target_url_tab_count == 1
        and answer.get("tab_closed") is True
        and observed_change
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    return evidence


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", required=True, type=Path)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", default=180, type=int)
    parser.add_argument("--model")
    parser.add_argument("--effort", choices=["low", "medium", "high"])
    args = parser.parse_args()
    evidence = run(args)
    print(json.dumps({"passed": evidence["passed"], "same_tab": evidence["same_tab"],
                      "saccade_tab_id": evidence["saccade_tab_id"],
                      "claude_execution_tab_ids": evidence["claude_execution_tab_ids"],
                      "output": str(args.output)}))
    return 0 if evidence["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
