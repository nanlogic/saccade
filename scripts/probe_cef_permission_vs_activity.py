#!/usr/bin/env python3
"""Verify Agent activity pause/reconnect does not change human On/Off permission."""

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
    DEFAULT_HUMAN_PAGE,
    McpClient,
    control_from_grant,
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
    parser.add_argument("--url", default=DEFAULT_HUMAN_PAGE.as_uri())
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def launch_human_enabled_broker(
    executable: pathlib.Path,
    profile: pathlib.Path,
    pointer: pathlib.Path,
    session: pathlib.Path,
    url: str,
    log_path: pathlib.Path,
) -> tuple[subprocess.Popen[bytes], pathlib.Path]:
    grant_path = session / "grant.json"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_CURRENT_POINTER": str(pointer),
            "SACCADE_ENGINE_BROKER": "1",
            # This is the deterministic stand-in for the human-owned switch
            # being On before the LLM attaches.
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_PROFILE_MODE": "normal",
            "SACCADE_PROFILE_NAME": "df-r07-permission-activity",
        }
    )
    command = [
        str(executable),
        f"--url={url}",
        f"--user-data-dir={profile}",
        "--use-native",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--window-size=1200,820",
    ]
    log_file = log_path.open("wb")
    process = subprocess.Popen(
        command, cwd=ROOT, env=env, stdout=log_file, stderr=subprocess.STDOUT
    )
    process._saccade_log_file = log_file  # type: ignore[attr-defined]
    return process, grant_path


def mcp_env(pointer: pathlib.Path, executable: pathlib.Path) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_CURRENT_AGENT_POINTER": str(pointer),
            "SACCADE_APP_EXECUTABLE": str(executable),
        }
    )
    return env


def wait_until(predicate: Any, timeout: float, detail: str) -> Any:
    deadline = time.monotonic() + timeout
    last: Any = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(0.05)
    raise TimeoutError(f"{detail}: {last}")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file() or not args.mcp_bin.is_file():
        raise SystemExit("missing built Saccade app or MCP binary")

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "report.json"
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-df-r07-activity-"))
    os.chmod(work, 0o700)
    process: subprocess.Popen[bytes] | None = None
    mcp: McpClient | None = None
    stage = "launch_human_enabled_broker"
    started = time.monotonic()
    report: dict[str, Any]
    try:
        session = work / "session"
        profile = work / "profile"
        session.mkdir(mode=0o700)
        profile.mkdir(mode=0o700)
        pointer = work / "current-grant-path"
        process, grant_path = launch_human_enabled_broker(
            executable,
            profile,
            pointer,
            session,
            args.url,
            output_dir / "saccade.log",
        )

        stage = "initial_human_permission_on"
        initial_grant = wait_for_json(grant_path, process, args.timeout_sec)
        initial_grant = wait_until(
            lambda: json.loads(grant_path.read_text())
            if grant_path.exists()
            and json.loads(grant_path.read_text()).get("status") == "granted"
            and json.loads(grant_path.read_text()).get("url")
            else None,
            args.timeout_sec,
            "human-enabled grant did not publish a URL",
        )
        control = control_from_grant(initial_grant)
        initial_status = control.call("shell_status")
        if initial_grant.get("grant_type") != "current_tab_copilot":
            raise AssertionError(f"expected human current-tab grant: {initial_grant}")
        if initial_grant.get("owner") != "human":
            raise AssertionError(f"expected human owner: {initial_grant}")
        if initial_status.get("agent_enabled") is not True:
            raise AssertionError(f"human-enabled tab was not Agent On: {initial_status}")
        if initial_status.get("agent_activity") != "idle":
            raise AssertionError(f"initial activity should be idle: {initial_status}")

        stage = "attach_idle"
        mcp = McpClient(args.mcp_bin.resolve(), mcp_env(pointer, executable))
        mcp.request("initialize", {})
        attached = mcp.tool(
            "saccade.tabs.grant_current",
            {"reason": "DF-R07 attach human-enabled current tab"},
        )
        tab = attached.get("tab") or {}
        tab_id = int(tab["tab_id"])
        if str(tab.get("owner", "")).lower() != "human":
            raise AssertionError(f"attach should preserve human owner: {attached}")
        if attached.get("agent_input_grant") is not True:
            raise AssertionError(f"attach should have agent input grant: {attached}")

        stage = "pause_runtime_permission_stays_on"
        paused = mcp.tool("saccade.tabs.pause_agent", {"tab_id": tab_id})
        if paused.get("agent_permission_unchanged") is not True:
            raise AssertionError(f"pause did not preserve permission: {paused}")
        paused_grant = json.loads(grant_path.read_text())
        paused_status = control_from_grant(paused_grant).call("shell_status")
        if paused_grant.get("grant_type") != "current_tab_copilot":
            raise AssertionError(f"pause changed grant type: {paused_grant}")
        if paused_grant.get("agent_input_grant") is not True:
            raise AssertionError(f"pause turned Agent permission Off: {paused_grant}")
        if paused_grant.get("paused") is not True:
            raise AssertionError(f"grant did not record paused runtime: {paused_grant}")
        if paused_status.get("agent_enabled") is not True or paused_status.get("paused") is not True:
            raise AssertionError(f"browser status did not preserve On+paused: {paused_status}")
        listed = mcp.tool("saccade.tabs.list", {})
        listed_tabs = listed.get("tabs") or []
        if len(listed_tabs) != 1 or listed_tabs[0].get("agent_activity") != "paused":
            raise AssertionError(f"registry should list the On paused tab: {listed}")

        stage = "paused_runtime_blocks_truth"
        paused_truth_blocked = False
        try:
            mcp.tool(
                "saccade.web.truth",
                {"tab_id": tab_id, "basis_page_revision": int(tab.get("page_revision", 1))},
            )
        except RuntimeError as error:
            paused_truth_blocked = "AGENT_PAUSED" in str(error) or "paused" in str(error)
        if not paused_truth_blocked:
            raise AssertionError("paused runtime did not block truth")

        stage = "disconnect_reconnect_resumes_idle"
        mcp.close()
        mcp = McpClient(args.mcp_bin.resolve(), mcp_env(pointer, executable))
        mcp.request("initialize", {})
        reattached = mcp.tool(
            "saccade.tabs.grant_current",
            {"reason": "DF-R07 reconnect to human-enabled current tab"},
        )
        resumed_grant = json.loads(grant_path.read_text())
        resumed_status = control_from_grant(resumed_grant).call("shell_status")
        if resumed_status.get("agent_enabled") is not True:
            raise AssertionError(f"reconnect lost Agent On permission: {resumed_status}")
        if resumed_status.get("paused") is not False:
            raise AssertionError(f"reconnect did not resume idle activity: {resumed_status}")
        if resumed_status.get("agent_activity") != "idle":
            raise AssertionError(f"reconnect activity should be idle: {resumed_status}")

        report = {
            "schema": "saccade-df-r07-permission-vs-activity-v1",
            "verdict": "PASS",
            "human_permission_started_on": True,
            "attach_preserved_human_owner": True,
            "pause_preserved_agent_on_permission": True,
            "paused_truth_blocked": True,
            "disconnect_reconnect_resumed_idle": True,
            "initial_status": initial_status,
            "paused_status": paused_status,
            "resumed_status": resumed_status,
            "paused_registry": listed,
            "attached_tab": tab,
            "reattached_tab": reattached.get("tab"),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        last_grant: Any = None
        last_status: Any = None
        try:
            if "grant_path" in locals() and grant_path.exists():
                last_grant = json.loads(grant_path.read_text())
                last_status = control_from_grant(last_grant).call("shell_status")
        except Exception as debug_error:  # pragma: no cover - diagnostic only.
            last_status = f"debug failed: {debug_error}"
        report = {
            "schema": "saccade-df-r07-permission-vs-activity-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "last_grant": last_grant,
            "last_status": last_status,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if mcp is not None:
            mcp.close()
        stop_process(process)
        shutil.rmtree(work, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_PERMISSION_ACTIVITY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
