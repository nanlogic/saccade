#!/usr/bin/env python3
"""Verify CEF profile persistence and visible-tab close recovery."""

from __future__ import annotations

import argparse
import functools
import http.server
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import threading
import time
from typing import Any

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_FIXTURE = ROOT / "test_pages" / "cef_day5_session"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
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
        time.sleep(0.03)
    raise TimeoutError(f"{detail}: {last}")


def field(fields: list[dict[str, Any]], field_id: str) -> dict[str, Any]:
    for item in fields:
        if item.get("field_id") == field_id:
            return item
    raise AssertionError(f"missing field {field_id}")


def launch(
    executable: pathlib.Path,
    profile: pathlib.Path,
    url: str,
    session: pathlib.Path,
    log_path: pathlib.Path,
) -> tuple[subprocess.Popen[bytes], pathlib.Path]:
    session.mkdir(mode=0o700, exist_ok=True)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(socket_path),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_PROFILE_MODE": "normal",
            "SACCADE_PROFILE_NAME": "day5-gate",
        }
    )
    command = [
        str(executable),
        f"--url={url}",
        f"--user-data-dir={profile}",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1100,760",
    ]
    log_file = log_path.open("wb")
    process = subprocess.Popen(
        command, cwd=ROOT, env=env, stdout=log_file, stderr=subprocess.STDOUT
    )
    process._saccade_log_file = log_file  # type: ignore[attr-defined]
    return process, grant_path


def stop(process: subprocess.Popen[bytes], control: EngineControl | None) -> None:
    if control is not None:
        try:
            control.call("close")
        except Exception:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=5)
    process._saccade_log_file.close()  # type: ignore[attr-defined]


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    if not executable.is_file() or not (fixture / "index.html").is_file():
        raise SystemExit("missing CEF app or Day 5 fixture")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    args.output_dir = args.output_dir.resolve()
    profile = args.output_dir / "profile"
    shutil.rmtree(profile, ignore_errors=True)
    profile.mkdir(mode=0o700)

    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=fixture)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base_url = f"http://127.0.0.1:{server.server_port}/"
    child_url = f"{base_url}child.html"
    persisted_field = "id:persisted-note"
    value = "ordinary profile state"
    started = time.monotonic()
    report: dict[str, Any]
    stage = "first_launch"
    first: subprocess.Popen[bytes] | None = None
    second: subprocess.Popen[bytes] | None = None
    first_control: EngineControl | None = None
    second_control: EngineControl | None = None
    try:
        first_session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-day5-a-"))
        first, first_grant_path = launch(
            executable, profile, base_url, first_session, args.output_dir / "first.log"
        )
        grant = wait_for_grant(first_grant_path, first, args.timeout_sec)
        first_control = EngineControl(
            pathlib.Path(grant["control_endpoint"]["path"]),
            grant["control_capability"]["token"],
        )
        truth = wait_for_collector(first_control, args.timeout_sec)
        first_tab = truth["tab_id"]
        initial_status = first_control.call("shell_status")
        if (
            initial_status.get("browser_count") != 1
            or initial_status.get("popup_count") != 0
            or initial_status.get("current_is_popup") is not False
        ):
            raise AssertionError(f"main browser role was wrong: {initial_status}")

        stage = "persist_ordinary_value"
        revision = int(truth["page_revision"])
        compiled = first_control.call(
            "form_compile_plan",
            {"basis_page_revision": revision, "assignments": {persisted_field: value}},
        )
        executed = first_control.call(
            "form_execute_plan",
            {
                "basis_page_revision": revision,
                "expected_plan_id": compiled["plan_id"],
                "assignments": {persisted_field: value},
            },
        )
        if executed.get("receipt_verified") is not True:
            raise AssertionError("ordinary profile write was not verified")

        stage = "open_child_tab"
        def recovery_action() -> tuple[dict[str, Any], dict[str, Any]] | None:
            current = first_control.call("actions")
            current_revision = int(current["page_revision"])
            link = next(
                (
                    item
                    for item in current["actions"]
                    if item.get("label") == "Open recovery tab"
                    and int(item.get("basis_page_revision", 0)) == current_revision
                ),
                None,
            )
            return (current, link) if link else None

        actions, link = wait_until(
            recovery_action, args.timeout_sec, "child-tab action was not discovered"
        )
        try:
            first_control.call(
                "act_drag",
                {
                    "action_id": link["action_id"],
                    "basis_page_revision": int(actions["page_revision"]),
                    "direction": "east",
                },
            )
        except RuntimeError as error:
            if "visible surface" not in str(error):
                raise
        else:
            raise AssertionError("ordinary link was accepted as a drag surface")
        first_control.call(
            "act",
            {
                "action_id": link["action_id"],
                "basis_page_revision": int(link["basis_page_revision"]),
            },
        )
        try:
            first_control.call("next_receipt", {"timeout_ms": 3000})
        except RuntimeError as error:
            if "TIMEOUT" not in str(error):
                raise

        def child_truth() -> dict[str, Any] | None:
            current = first_control.call("truth")
            return current if current.get("url") == child_url else None

        child = wait_until(child_truth, args.timeout_sec, "child tab did not become visible")
        child_tab = child["tab_id"]
        if child_tab == first_tab:
            raise AssertionError("new tab reused the original tab identity")
        child_status = first_control.call("shell_status")
        if (
            child_status.get("browser_count") != 2
            or child_status.get("popup_count") != 0
            or child_status.get("current_is_popup") is not False
        ):
            raise AssertionError(f"child browser role was wrong: {child_status}")

        stage = "close_child_recover_parent"
        first_control.call("close")

        def parent_truth() -> dict[str, Any] | None:
            current = first_control.call("truth")
            return current if current.get("url") == base_url else None

        recovered = wait_until(
            parent_truth, args.timeout_sec, "bridge did not recover the remaining tab"
        )
        if recovered["tab_id"] != first_tab:
            raise AssertionError("bridge recovered the wrong tab identity")

        def recovered_shell_status() -> dict[str, Any] | None:
            current = first_control.call("shell_status")
            if (
                current.get("browser_count") == 1
                and current.get("popup_count") == 0
                and current.get("current_is_popup") is False
            ):
                return current
            return None

        recovered_status = wait_until(
            recovered_shell_status,
            args.timeout_sec,
            "parent browser role did not recover",
        )
        if (
            recovered_status.get("browser_count") != 1
            or recovered_status.get("popup_count") != 0
            or recovered_status.get("current_is_popup") is not False
        ):
            raise AssertionError(
                f"parent browser role did not recover: {recovered_status}"
            )
        stop(first, first_control)
        first = None
        first_control = None

        stage = "profile_restart"
        second_session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-day5-b-"))
        second, second_grant_path = launch(
            executable, profile, base_url, second_session, args.output_dir / "second.log"
        )
        second_grant = wait_for_grant(second_grant_path, second, args.timeout_sec)
        second_control = EngineControl(
            pathlib.Path(second_grant["control_endpoint"]["path"]),
            second_grant["control_capability"]["token"],
        )
        second_truth = wait_for_collector(second_control, args.timeout_sec)
        inspected = second_control.call(
            "inspect_fields",
            {
                "basis_page_revision": int(second_truth["page_revision"]),
                "field_ids": [persisted_field],
            },
        )
        persisted = field(inspected["fields"], persisted_field).get("value") == value
        if not persisted:
            raise AssertionError("normal profile did not retain ordinary local state")
        report = {
            "schema": "saccade-cef-day5-session-v1",
            "verdict": "PASS",
            "engine": "cef",
            "profile": {"mode": "normal", "restart_persisted": True},
            "tabs": {
                "distinct_identity": True,
                "visible_child_followed": True,
                "close_recovered_parent": True,
                "main_role_verified": True,
                "ordinary_child_opened_as_tab": True,
                "non_surface_drag_rejected": True,
            },
            "values_logged": False,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-cef-day5-session-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if first is not None:
            stop(first, first_control)
        if second is not None:
            stop(second, second_control)
        server.shutdown()
        server.server_close()

    report_path = args.output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_DAY5_SESSION verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
