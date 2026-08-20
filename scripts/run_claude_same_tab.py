#!/usr/bin/env python3
"""Run one value-free Claude Chrome + Saccade same-tab fixture dogfood.

The harness opens the target tab through the ordinary Saccade MCP stdio protocol
*before* the `claude` subprocess starts, then names that exact `tab_id` in the
prompt. Claude in Chrome must adopt the tab Saccade already owns; it may not
navigate, and it may not open a second copy of the URL. Same-tab identity is a
hard pass condition, and the harness always closes the tab it opened.
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
    tab_id: str,
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

Saccade has ALREADY opened that URL and the tab is active. Its Saccade tab_id is exactly {tab_id}, and the same integer is the Chrome tab id. Do not call saccade.tabs.open, do not navigate, and do not open a second copy of this URL: a duplicate tab fails the run.

Do this in order:
1. Call your Claude in Chrome tabs_context_mcp first and report exactly what it returns.
2. Read one full Saccade Truth view of tab {tab_id} and note the Toggle signal button's pressed state.
3. Using your own Claude in Chrome tool with tabId {tab_id}, click the visible Toggle signal button in that exact tab.
4. Read one revision-bounded Saccade delta of tab {tab_id} and verify the button's pressed state changed.

Do not close, create, or reshuffle Chrome tab groups; the tab is already open and is the only tab you may act on.

Leave the tab open; the harness closes it. Do not use Bash, web search, screenshots, source inspection, DOM queries, JavaScript, selectors, coordinates, Playwright, or the Reference Actuator.

Return only JSON with completed, browser_instance_id, tab_id, execution_tab_id, initial_revision, final_revision, tabs_context, and summary. Set completed true only if your Chrome action actually executed on tab {tab_id} and Saccade observed the change."""
    result = [
        str(claude), "-p", prompt, "--output-format", "stream-json", "--verbose",
        "--no-session-persistence", "--chrome", "--strict-mcp-config",
        "--mcp-config", config, "--permission-mode", "auto",
        "--disallowedTools", "Bash,WebFetch,WebSearch",
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
        with SaccadeMcp(args.runtime, args.runtime_dir) as mcp:
            opened = mcp.tool("saccade.tabs.open", {"url": args.url, "active": True})
            opened_tab_id = str(opened.get("tab_id") or "")
            if not opened_tab_id:
                raise RuntimeError(f"Saccade did not return a tab_id: {opened}")
            before = mcp.tool("saccade.truth.read", {"tab_id": opened_tab_id})
            initial_revision = revision_of(before)
            initial_pressed = pressed_state(before)

        completed = subprocess.run(
            command(
                args.claude,
                args.runtime,
                args.runtime_dir,
                args.url,
                opened_tab_id,
                args.model,
                args.effort,
            ),
            capture_output=True, text=True, timeout=args.timeout, check=False,
        )
    except Exception as error:  # noqa: BLE001 - recorded as evidence, never swallowed
        setup_error = f"{type(error).__name__}: {error}"
    finally:
        if opened_tab_id:
            try:
                with SaccadeMcp(args.runtime, args.runtime_dir) as mcp:
                    duplicate_target_tabs = [t for t in tabs_with_url(mcp, args.url)
                                             if t != opened_tab_id]
                    try:
                        after = mcp.tool("saccade.truth.read", {"tab_id": opened_tab_id})
                        final_revision = revision_of(after)
                        final_pressed = pressed_state(after)
                    except RuntimeError:
                        final_revision = None
                    mcp.tool("saccade.tabs.close", {"tab_id": opened_tab_id})
            except Exception as error:  # noqa: BLE001
                setup_error = setup_error or f"cleanup failed: {type(error).__name__}: {error}"

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
    answer: dict[str, Any] = {}
    try:
        parsed = json.loads(str(result.get("result") or ""))
        if isinstance(parsed, dict):
            answer = parsed
    except json.JSONDecodeError:
        pass

    names = tool_names(events)
    used_saccade = any("saccade" in name.casefold() for name in names)
    used_chrome = any("chrome" in name.casefold() for name in names)
    execution_tab_ids = chrome_execution_tab_ids(events)
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
        "tab_preopened_before_claude_started": bool(opened_tab_id),
        "saccade_tab_id": opened_tab_id,
        "claude_execution_tab_ids": execution_tab_ids,
        "same_tab": same_tab,
        "duplicate_target_tabs": duplicate_target_tabs,
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
        "chrome_tool_failures": chrome_tool_failures(events),
        "saccade_observation_used": used_saccade,
        "claude_chrome_execution_used": used_chrome,
        "setup_error": setup_error,
        "answer": answer,
        "stderr_tail": (completed.stderr[-1000:] if completed else ""),
    }
    evidence["passed"] = bool(
        setup_error is None
        and completed is not None and completed.returncode == 0
        and answer.get("completed") is True
        and used_saccade and used_chrome
        and same_tab
        and not duplicate_target_tabs
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
