#!/usr/bin/env python3
"""Verify physical macOS mouse/focus/typing reaches a headed CEF page."""

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
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_FIXTURE = ROOT / "test_pages" / "cef_human_input" / "index.html"


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
        time.sleep(0.05)
    raise TimeoutError(f"{detail}: {last}")


def wait_for_grant(path: pathlib.Path, timeout: float) -> dict[str, Any]:
    def read() -> dict[str, Any] | None:
        try:
            value = json.loads(path.read_text())
            capability = value.get("control_capability") or {}
            return value if capability.get("token") else None
        except (FileNotFoundError, json.JSONDecodeError):
            return None

    return wait_until(read, timeout, "waiting for LaunchServices grant")


def title(control: EngineControl) -> str:
    return str(control.call("shell_status").get("title") or "")


def focus_saccade() -> None:
    script = (
        'tell application "System Events"\n'
        '  set targetProcess to first application process whose bundle identifier is "ai.saccade.browser"\n'
        '  set frontmost of targetProcess to true\n'
        "end tell"
    )
    result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"macOS focus failed: {result.stderr.strip()}")


def system_click(x: int, y: int) -> None:
    focus_saccade()
    source = r"""
import CoreGraphics
import Foundation

let x = Double(CommandLine.arguments[1])!
let y = Double(CommandLine.arguments[2])!
let point = CGPoint(x: x, y: y)
let source = CGEventSource(stateID: .hidSystemState)

CGEvent(mouseEventSource: source, mouseType: .mouseMoved,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(50_000)
CGEvent(mouseEventSource: source, mouseType: .leftMouseDown,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(50_000)
CGEvent(mouseEventSource: source, mouseType: .leftMouseUp,
        mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
"""
    with tempfile.NamedTemporaryFile("w", suffix=".swift", delete=False) as handle:
        handle.write(source)
        helper = pathlib.Path(handle.name)
    try:
        result = subprocess.run(
            ["swift", str(helper), str(x), str(y)], capture_output=True, text=True
        )
    finally:
        helper.unlink(missing_ok=True)
    if result.returncode != 0:
        raise RuntimeError(f"macOS click failed: {result.stderr.strip()}")


def system_type(text: str) -> None:
    focus_saccade()
    script = (
        'tell application "System Events"\n'
        f"  keystroke {json.dumps(text)}\n"
        "end tell"
    )
    result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"macOS typing failed: {result.stderr.strip()}")


def main() -> int:
    args = parse_args()
    app = args.app.resolve()
    fixture = args.fixture.resolve()
    if not (app / "Contents" / "MacOS" / "cefsimple").is_file() or not fixture.is_file():
        raise SystemExit("missing signed CEF app or human-input fixture")

    output = args.output_dir.resolve()
    shutil.rmtree(output, ignore_errors=True)
    output.mkdir(parents=True, mode=0o700)
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-input-session-"))
    profile = pathlib.Path(tempfile.mkdtemp(prefix="saccade-input-profile-"))
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    log_path = output / "cef.log"
    url = fixture.as_uri()

    subprocess.run(
        ["osascript", "-e", 'tell application id "ai.saccade.browser" to quit'],
        check=False,
        capture_output=True,
    )
    wait_until(
        lambda: subprocess.run(
            ["pgrep", "-f", "Saccade.app/Contents/MacOS/cefsimple"],
            capture_output=True,
        ).returncode != 0,
        8.0,
        "waiting for prior Saccade instance to quit",
    )

    command = [
        "open", "-n",
        "--env", f"SACCADE_ENGINE_SOCKET={socket_path}",
        "--env", f"SACCADE_ENGINE_GRANT_PATH={grant_path}",
        "--env", "SACCADE_ENGINE_GRANT_CURRENT_TAB=1",
        "--env", "SACCADE_PROFILE_MODE=incognito",
        "--env", "SACCADE_PROFILE_NAME=human-input-gate",
        "--stdout", str(log_path), "--stderr", str(log_path),
        str(app), "--args", f"--url={url}", f"--user-data-dir={profile}",
        "--incognito", "--use-mock-keychain", "--no-first-run",
        "--no-default-browser-check", "--window-position=80,80",
        "--window-size=1000,700",
    ]
    subprocess.run(command, check=True)

    control: EngineControl | None = None
    report: dict[str, Any] = {
        "schema": "saccade-cef-human-input-v1",
        "physical_mouse_route": "macos_coregraphics_hid_event",
        "browser_input_api_used": False,
        "typed_value_logged": False,
    }
    try:
        grant = wait_for_grant(grant_path, args.timeout_sec)
        control = EngineControl(
            socket_path, str(grant["control_capability"]["token"])
        )
        ready = wait_until(
            lambda: (current := title(control)).startswith("HUMAN_INPUT_READY:") and current,
            args.timeout_sec,
            "waiting for geometry title",
        )
        _, screen_x, screen_y, outer_h, inner_h = ready.split(":")
        content_top = int(screen_y) + int(outer_h) - int(inner_h)
        click_x = int(screen_x) + 240 + 220
        system_click(click_x, content_top + 180 + 70)
        wait_until(lambda: title(control) == "HUMAN_CLICK_PASS", 5.0, "physical click")

        system_click(click_x, content_top + 380 + 32)
        system_type("abcdef")
        wait_until(lambda: title(control) == "HUMAN_TYPE_PASS", 5.0, "physical typing")
        report.update({"click": "PASS", "focus_and_type": "PASS", "verdict": "PASS"})
    except Exception as exc:
        report.update({"verdict": "FAIL", "error": str(exc)})
    finally:
        if control is not None:
            try:
                control.call("close")
            except Exception:
                pass
        time.sleep(1)
        if subprocess.run(
            ["pgrep", "-f", "Saccade.app/Contents/MacOS/cefsimple"],
            capture_output=True,
        ).returncode == 0:
            subprocess.run(
                ["osascript", "-e", 'tell application id "ai.saccade.browser" to quit'],
                check=False,
                capture_output=True,
            )
        shutil.rmtree(session, ignore_errors=True)
        shutil.rmtree(profile, ignore_errors=True)

    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"CEF_HUMAN_INPUT verdict={report['verdict']} report={output / 'report.json'}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
