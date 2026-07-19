#!/usr/bin/env python3
"""Verify native resize invalidation, local semantic rebase, and receipts."""

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

from probe_ai038_conversational_dogfood import McpClient
from probe_cef_truth_reflex import EngineControl


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_MCP = ROOT / "target" / "debug" / "saccade-mcp"
DEFAULT_FIXTURE = ROOT / "test_pages" / "layout_epoch" / "index.html"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
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


def wait_for_grant(path: pathlib.Path, timeout: float) -> dict[str, Any]:
    def read() -> dict[str, Any] | None:
        try:
            value = json.loads(path.read_text())
            token = (value.get("control_capability") or {}).get("token")
            return value if token else None
        except (FileNotFoundError, json.JSONDecodeError):
            return None

    return wait_until(read, timeout, "waiting for CEF grant")


def resize_saccade(width: int, height: int) -> None:
    script = f'''
tell application "System Events"
  set targetProcess to first application process whose bundle identifier is "ai.saccade.browser"
  set frontmost of targetProcess to true
  set size of front window of targetProcess to {{{width}, {height}}}
end tell
'''
    result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"native resize failed: {result.stderr.strip()}")


def action_named(actions_result: dict[str, Any], label: str) -> dict[str, Any]:
    matches = [
        action
        for action in actions_result.get("actions", [])
        if action.get("label") == label
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected one {label!r} action, got {matches}")
    return matches[0]


def wait_for_layout(
    control: EngineControl, old_revision: int, timeout: float
) -> dict[str, Any]:
    def current() -> dict[str, Any] | None:
        status = control.call("shell_status")
        revision = int(status.get("page_revision", 0))
        return (
            status
            if revision > old_revision
            and status.get("revision_cause") == "layout"
            and status.get("collector_ready") is True
            else None
        )

    return wait_until(current, timeout, "waiting for refreshed layout epoch")


def main() -> int:
    args = parse_args()
    app = args.app.resolve()
    mcp_bin = args.mcp_bin.resolve()
    fixture = args.fixture.resolve()
    if not (app / "Contents" / "MacOS" / "Saccade").is_file():
        raise SystemExit("missing CEF app")
    if not mcp_bin.is_file() or not fixture.is_file():
        raise SystemExit("missing MCP binary or layout fixture")

    output = args.output_dir.resolve()
    shutil.rmtree(output, ignore_errors=True)
    output.mkdir(parents=True, mode=0o700)
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-layout-epoch-"))
    session = work / "session"
    profile = work / "profile"
    session.mkdir(mode=0o700)
    profile.mkdir(mode=0o700)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    pointer_path = work / "current-grant-path"
    log_path = output / "cef.log"
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    mcp: McpClient | None = None
    started = time.monotonic()
    report: dict[str, Any] = {
        "schema": "saccade-cef-layout-epoch-v1",
        "native_window_resize": True,
        "screenshots_used": False,
        "sensitive_values_logged": False,
    }
    try:
        env = os.environ.copy()
        env.update(
            {
                "SACCADE_ENGINE_SOCKET": str(socket_path),
                "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
                "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
                "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
                "SACCADE_ENGINE_INITIAL_TAB_GRANT": "1",
                "SACCADE_ENGINE_BROKER": "1",
                "SACCADE_PROFILE_MODE": "incognito",
                "SACCADE_PROFILE_NAME": "layout-epoch-gate",
            }
        )
        with log_path.open("wb") as log:
            process = subprocess.Popen(
                [
                    str(app / "Contents" / "MacOS" / "Saccade"),
                    f"--url={fixture.as_uri()}",
                    f"--user-data-dir={profile}",
                    "--incognito",
                    "--use-mock-keychain",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--window-position=80,80",
                    "--window-size=1200,820",
                ],
                cwd=ROOT,
                env=env,
                stdout=log,
                stderr=log,
            )
        grant = wait_for_grant(grant_path, args.timeout_sec)
        pointer_path.write_text(str(grant_path) + "\n")
        os.chmod(pointer_path, 0o600)
        control = EngineControl(socket_path, str(grant["control_capability"]["token"]))
        wait_until(
            lambda: control.call("shell_status").get("title") == "LAYOUT_READY",
            args.timeout_sec,
            "waiting for layout fixture",
        )
        mcp_env = os.environ.copy()
        mcp_env.update(
            {
                "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
                "SACCADE_APP_EXECUTABLE": str(app / "Contents" / "MacOS" / "Saccade"),
            }
        )
        mcp = McpClient(mcp_bin, mcp_env)
        mcp.request("initialize", {})
        attached = mcp.tool("saccade.tabs.grant_current", {})
        tab_id = int(attached["tab"]["tab_id"])

        wide_actions = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
        dom_action = action_named(wide_actions, "Responsive action")
        desktop_only = action_named(wide_actions, "Desktop only action")
        old_revision = int(wide_actions["page_revision"])
        old_epoch = int(wide_actions["layout_epoch"])
        resize_saccade(700, 820)
        resize_started = time.monotonic()
        narrow_status = wait_for_layout(control, old_revision, args.timeout_sec)
        invalidation_ms = (time.monotonic() - resize_started) * 1000

        dom_act_started = time.monotonic()
        dom_result = mcp.tool(
            "saccade.web.act",
            {
                "tab_id": tab_id,
                "action_id": dom_action["action_id"],
                "basis_page_revision": old_revision,
                "basis_layout_epoch": old_epoch,
            },
        )
        dom_rebase_ms = (time.monotonic() - dom_act_started) * 1000
        wait_until(
            lambda: control.call("shell_status").get("title") == "DOM_LAYOUT_PASS",
            5.0,
            "waiting for rebased DOM action",
        )
        stale_removed = False
        try:
            mcp.tool(
                "saccade.web.act",
                {
                    "tab_id": tab_id,
                    "action_id": desktop_only["action_id"],
                    "basis_page_revision": old_revision,
                    "basis_layout_epoch": old_epoch,
                },
            )
        except RuntimeError as exc:
            stale_removed = "stale layout removed" in str(exc)
        if not stale_removed:
            raise AssertionError("resize-hidden target was not rejected as stale")
        if control.call("shell_status").get("title") == "STALE_TARGET_WRONG_CLICK":
            raise AssertionError("resize-hidden target received native input")

        current_revision = int(narrow_status["page_revision"])
        resize_saccade(1200, 820)
        wait_for_layout(control, current_revision, args.timeout_sec)
        canvas_actions = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
        canvas_action = action_named(canvas_actions, "Canvas center target")
        canvas_revision = int(canvas_actions["page_revision"])
        canvas_epoch = int(canvas_actions["layout_epoch"])
        resize_saccade(700, 820)
        wait_for_layout(control, canvas_revision, args.timeout_sec)
        canvas_act_started = time.monotonic()
        canvas_result = mcp.tool(
            "saccade.web.act",
            {
                "tab_id": tab_id,
                "action_id": canvas_action["action_id"],
                "basis_page_revision": canvas_revision,
                "basis_layout_epoch": canvas_epoch,
            },
        )
        canvas_rebase_ms = (time.monotonic() - canvas_act_started) * 1000
        wait_until(
            lambda: control.call("shell_status").get("title") == "CANVAS_LAYOUT_PASS",
            5.0,
            "waiting for rebased Canvas action",
        )

        report.update(
            {
                "dom_layout_rebased": dom_result.get("layout_rebased") is True,
                "dom_receipt_verified": (dom_result.get("verification") or {}).get("verified") is True,
                "canvas_layout_rebased": canvas_result.get("layout_rebased") is True,
                "canvas_receipt_verified": (canvas_result.get("verification") or {}).get("verified") is True,
                "removed_target_rejected_before_input": stale_removed,
                "layout_invalidation_ms": round(invalidation_ms, 3),
                "dom_local_rebase_and_receipt_ms": round(dom_rebase_ms, 3),
                "canvas_local_rebase_and_receipt_ms": round(canvas_rebase_ms, 3),
                "page_revision_advanced": int(narrow_status["page_revision"]) > old_revision,
                "layout_epoch_advanced": int(narrow_status["layout_epoch"]) > old_epoch,
                "verdict": "PASS",
            }
        )
    except Exception as exc:
        report.update({"verdict": "FAIL", "error": str(exc)})
    finally:
        if mcp is not None:
            mcp.close()
        if control is not None:
            try:
                control.call("close")
            except Exception:
                pass
        if process is not None:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
        shutil.rmtree(work, ignore_errors=True)

    report["duration_sec"] = round(time.monotonic() - started, 3)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"CEF_LAYOUT_EPOCH verdict={report['verdict']} report={output / 'report.json'}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
