#!/usr/bin/env python3
"""Verify that MCP lists/attaches only Agent On tabs from a live Saccade broker."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
from typing import Any

from probe_cef_tab_defaults import (
    DEFAULT_AGENT_PAGE,
    DEFAULT_HUMAN_PAGE,
    McpClient,
    control_from_grant,
    launch_human_broker,
    stop_process,
    wait_for_json,
)


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_MCP = ROOT / "target" / "debug" / "saccade-mcp"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--human-url", default=DEFAULT_HUMAN_PAGE.as_uri())
    parser.add_argument("--agent-url-one", default=DEFAULT_AGENT_PAGE.as_uri())
    parser.add_argument("--agent-url-two", default=DEFAULT_HUMAN_PAGE.as_uri() + "#agent-two")
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def wait_until(predicate: Any, timeout: float, detail: str) -> Any:
    deadline = time.monotonic() + timeout
    last: Any = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(0.05)
    raise TimeoutError(f"{detail}: {last}")


def browser_tab_ids(tabs: list[dict[str, Any]]) -> list[str]:
    return [str(tab.get("browser_tab_id", "")) for tab in tabs]


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file() or not args.mcp_bin.is_file():
        raise SystemExit("missing built Saccade app or MCP binary")

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "report.json"
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-df-r06-tabs-"))
    os.chmod(work, 0o700)
    process: subprocess.Popen[bytes] | None = None
    mcp: McpClient | None = None
    stage = "launch_human_broker"
    started = time.monotonic()
    report: dict[str, Any]
    try:
        session = work / "session"
        profile = work / "profile"
        session.mkdir(mode=0o700)
        profile.mkdir(mode=0o700)
        pointer = work / "current-grant-path"
        process, grant_path = launch_human_broker(
            executable,
            profile,
            pointer,
            session,
            args.human_url,
            output_dir / "saccade.log",
        )

        stage = "human_off_broker"
        initial_grant = wait_for_json(grant_path, process, args.timeout_sec)
        control = control_from_grant(initial_grant)
        initial_status = control.call("shell_status")
        if initial_status.get("agent_enabled") is not False:
            raise AssertionError(f"human tab did not start Agent Off: {initial_status}")

        stage = "open_two_agent_tabs"
        mcp_env = os.environ.copy()
        mcp_env.update(
            {
                "SACCADE_CURRENT_AGENT_POINTER": str(pointer),
                "SACCADE_APP_EXECUTABLE": str(executable),
            }
        )
        mcp = McpClient(args.mcp_bin.resolve(), mcp_env)
        mcp.request("initialize", {})
        opened_one = mcp.tool("saccade.tabs.open_agent", {"url": args.agent_url_one})
        opened_two = mcp.tool("saccade.tabs.open_agent", {"url": args.agent_url_two})

        stage = "registry_lists_only_agent_on_tabs"

        last_listed: dict[str, Any] = {}

        def listed_two() -> dict[str, Any] | None:
            nonlocal last_listed
            listed = mcp.tool("saccade.tabs.list", {})
            last_listed = listed
            tabs = listed.get("tabs")
            if not isinstance(tabs, list):
                return None
            ids = browser_tab_ids(tabs)
            if len(tabs) == 2 and all(ids) and len(set(ids)) == 2:
                return listed
            return None

        listed = wait_until(
            listed_two,
            args.timeout_sec,
            "MCP list did not expose exactly two Agent On tabs",
        )
        tabs = listed["tabs"]
        if listed.get("browser_count") != 3:
            raise AssertionError(f"registry should know 3 browser tabs: {listed}")
        if listed.get("eligible_count") != 2:
            raise AssertionError(f"registry should expose exactly 2 eligible tabs: {listed}")
        if listed.get("agent_off_tabs_omitted") is not True:
            raise AssertionError(f"registry did not assert Off tabs were omitted: {listed}")
        if any(tab.get("agent_enabled") is not True for tab in tabs):
            raise AssertionError(f"registry exposed a non-Agent-On tab: {listed}")
        if any("url" in tab for tab in tabs):
            raise AssertionError(f"registry should not expose full URLs: {listed}")

        stage = "attach_by_opaque_browser_tab_id"
        attach_target = str(tabs[0]["browser_tab_id"])
        attached = mcp.tool(
            "saccade.tabs.grant_current",
            {"browser_tab_id": attach_target, "reason": "DF-R06 attach by opaque tab id"},
        )
        attached_browser_tab_id = (
            attached.get("same_webview_control", {})
            .get("tab_identity")
            or attached.get("tab", {}).get("browser_tab_id")
            or attached.get("tab", {}).get("tab_identity")
        )
        attached_tab = attached.get("tab") or {}
        if attached.get("source") != "current_agent_tab_registry":
            raise AssertionError(f"attach did not use tab registry source: {attached}")
        if str(attached_tab.get("owner", "")).lower() != "agent":
            raise AssertionError(f"attach target was not Agent-owned: {attached}")
        if attached.get("agent_input_grant") is not True:
            raise AssertionError(f"attach did not grant agent input: {attached}")

        status = control_from_grant(json.loads(grant_path.read_text())).call("shell_status")
        if status.get("tab_identity") != attach_target:
            raise AssertionError(f"browser selected {status}, expected {attach_target}")

        report = {
            "schema": "saccade-df-r06-multi-tab-registry-v1",
            "verdict": "PASS",
            "agent_on_tabs_listed": len(tabs),
            "browser_count": listed.get("browser_count"),
            "eligible_count": listed.get("eligible_count"),
            "agent_off_tab_omitted": True,
            "attached_browser_tab_id": attach_target,
            "opened_one": opened_one.get("tab"),
            "opened_two": opened_two.get("tab"),
            "listed_tabs": tabs,
            "attached_tab": attached_tab,
            "attached_browser_tab_id_from_result": attached_browser_tab_id,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-df-r06-multi-tab-registry-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "last_listed": locals().get("last_listed", {}),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if mcp is not None:
            mcp.close()
        stop_process(process)
        shutil.rmtree(work, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_MULTI_TAB_REGISTRY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
