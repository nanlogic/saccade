#!/usr/bin/env python3
"""Verify human tabs default Off and MCP open_agent reuses Saccade."""

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

from probe_cef_truth_reflex import EngineControl


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "dist" / "saccade-cef-dogfood-current" / "Saccade.app"
DEFAULT_MCP = ROOT / "dist" / "saccade-cef-dogfood-current" / "bin" / "saccade-mcp"
DEFAULT_HUMAN_PAGE = ROOT / "test_pages" / "native_basics" / "page-one.html"
DEFAULT_AGENT_PAGE = ROOT / "test_pages" / "native_basics" / "page-two.html"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--human-url", default=DEFAULT_HUMAN_PAGE.as_uri())
    parser.add_argument("--agent-url", default=DEFAULT_AGENT_PAGE.as_uri())
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


class McpClient:
    def __init__(self, binary: pathlib.Path, env: dict[str, str]) -> None:
        self.process = subprocess.Popen(
            [str(binary), "serve-stdio"],
            cwd=ROOT,
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        assert self.process.stdin is not None
        assert self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        request = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr[-1000:]}")
        response = json.loads(line)
        if response.get("error"):
            raise RuntimeError(f"MCP {method} failed: {response['error']}")
        return response.get("result", {})

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent")
        if not isinstance(content, dict):
            raise RuntimeError(f"{name} returned no structured content")
        return content

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def wait_for_json(path: pathlib.Path, process: subprocess.Popen[bytes], timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: Any = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Saccade exited early with status {process.returncode}")
        try:
            if path.stat().st_size > 0:
                last = json.loads(path.read_text())
                return last
        except (FileNotFoundError, json.JSONDecodeError) as error:
            last = str(error)
        time.sleep(0.05)
    raise TimeoutError(f"timed out waiting for {path}: {last}")


def wait_until(predicate: Any, timeout: float, detail: str) -> Any:
    deadline = time.monotonic() + timeout
    last: Any = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(0.05)
    raise TimeoutError(f"{detail}: {last}")


def control_from_grant(grant: dict[str, Any]) -> EngineControl:
    endpoint = grant["control_endpoint"]
    capability = grant["control_capability"]["token"]
    return EngineControl(pathlib.Path(endpoint["path"]), capability)


def launch_human_broker(
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
            "SACCADE_PROFILE_MODE": "normal",
            "SACCADE_PROFILE_NAME": "df-r02-r03",
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


def stop_process(process: subprocess.Popen[bytes] | None) -> None:
    if process is None:
        return
    try:
        process.terminate()
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    finally:
        log_file = getattr(process, "_saccade_log_file", None)
        if log_file:
            log_file.close()


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file() or not args.mcp_bin.is_file():
        raise SystemExit("missing packaged Saccade app or MCP binary")

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "report.json"
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-df-r02-r03-"))
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
            executable, profile, pointer, session, args.human_url, output_dir / "saccade.log"
        )

        stage = "human_tab_default_off"
        initial_grant = wait_for_json(grant_path, process, args.timeout_sec)
        if initial_grant.get("status") != "available":
            raise AssertionError(f"human launch should publish available broker: {initial_grant}")
        if initial_grant.get("grant_type") != "tab_broker":
            raise AssertionError(f"human launch should not grant current tab: {initial_grant}")
        if initial_grant.get("agent_input_grant") is not False:
            raise AssertionError(f"human launch granted agent input: {initial_grant}")
        control = control_from_grant(initial_grant)
        initial_status = control.call("shell_status")
        if initial_status.get("agent_enabled") is not False:
            raise AssertionError(f"human tab did not start Agent Off: {initial_status}")
        if initial_status.get("browser_count") != 1:
            raise AssertionError(f"expected one human browser: {initial_status}")

        stage = "mcp_open_agent"
        mcp_env = os.environ.copy()
        mcp_env.update(
            {
                "SACCADE_CURRENT_AGENT_POINTER": str(pointer),
                "SACCADE_APP_EXECUTABLE": str(executable),
            }
        )
        mcp = McpClient(args.mcp_bin.resolve(), mcp_env)
        mcp.request("initialize", {})
        opened = mcp.tool("saccade.tabs.open_agent", {"url": args.agent_url})
        if opened.get("browser_was_running") is not True:
            raise AssertionError(f"open_agent did not reuse running Saccade: {opened}")
        tab = opened.get("tab") or {}
        if str(tab.get("owner", "")).lower() != "agent":
            raise AssertionError(f"open_agent did not return an Agent-owned tab: {opened}")
        if opened.get("agent_input_grant") is not True:
            raise AssertionError(f"Agent-created tab did not allow agent input: {opened}")

        stage = "agent_tab_on"
        agent_grant = wait_until(
            lambda: json.loads(grant_path.read_text())
            if grant_path.exists()
            and json.loads(grant_path.read_text()).get("grant_type") == "agent_created_tab"
            else None,
            args.timeout_sec,
            "agent-created tab grant not published",
        )
        agent_status = control_from_grant(agent_grant).call("shell_status")
        if agent_status.get("agent_enabled") is not True:
            raise AssertionError(f"agent tab was not Agent On: {agent_status}")
        if agent_status.get("browser_count") != 2:
            raise AssertionError(f"open_agent did not create a second tab: {agent_status}")

        stage = "close_agent_recovers_human_off"
        close_result = mcp.tool("saccade.tabs.close", {"tab_id": int(tab["tab_id"])})
        if close_result.get("status") not in {"closed", "ok"}:
            raise AssertionError(f"unexpected close result: {close_result}")

        def human_off_again() -> dict[str, Any] | None:
            current = json.loads(grant_path.read_text())
            status = control_from_grant(current).call("shell_status")
            if (
                current.get("grant_type") == "tab_broker"
                and status.get("agent_enabled") is False
                and status.get("browser_count") == 1
            ):
                return {"grant": current, "status": status}
            return None

        recovered = wait_until(
            human_off_again,
            args.timeout_sec,
            "human tab did not recover as Agent Off after closing Agent tab",
        )

        report = {
            "schema": "saccade-df-r02-r03-tab-defaults-v1",
            "verdict": "PASS",
            "human_tab_default_off": True,
            "open_agent_reused_running_process": True,
            "agent_tab_started_on": True,
            "human_tab_not_taken_over": True,
            "opened_tab": tab,
            "initial_status": initial_status,
            "agent_status": agent_status,
            "recovered_status": recovered["status"],
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-df-r02-r03-tab-defaults-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if mcp is not None:
            mcp.close()
        stop_process(process)
        shutil.rmtree(work, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_TAB_DEFAULTS verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
