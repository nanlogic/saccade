#!/usr/bin/env python3
"""Probe official ServoShell's WebDriver control surface."""

from __future__ import annotations

import argparse
import base64
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SERVOSHELL = Path("/Applications/Servo.app/Contents/MacOS/servoshell")
DEFAULT_FIXTURE = ROOT / "test_pages/browser_session/index.html"


def unix_ms() -> int:
    return int(time.time() * 1000)


def webdriver_request(port: int, method: str, path: str, payload=None, timeout: float = 10.0):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"} if payload is not None else {}
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        text = response.read().decode("utf-8", "replace")
        return response.status, json.loads(text) if text else None


def wait_for_status(port: int, proc: subprocess.Popen, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last_error = None
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"servoshell exited before WebDriver was ready: {proc.returncode}")
        try:
            return webdriver_request(port, "GET", "/status", timeout=0.5)
        except Exception as error:  # noqa: BLE001 - probe should report any transport failure.
            last_error = repr(error)
            time.sleep(0.25)
    raise TimeoutError(f"WebDriver status was not ready; last_error={last_error}")


def value_session_id(response) -> str:
    value = response.get("value") if isinstance(response, dict) else None
    if isinstance(value, dict) and isinstance(value.get("sessionId"), str):
        return value["sessionId"]
    if isinstance(response.get("sessionId"), str):
        return response["sessionId"]
    raise RuntimeError(f"new session response did not include a session id: {response}")


def element_id(response) -> str | None:
    value = response.get("value") if isinstance(response, dict) else None
    if not isinstance(value, dict):
        return None
    return value.get("element-6066-11e4-a52e-4f735466cecf") or value.get("ELEMENT")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--url", default="file://" + str(DEFAULT_FIXTURE))
    parser.add_argument("--port", type=int, default=7081)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--no-headless", action="store_true")
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    args = parser.parse_args()

    output_dir = args.output_dir or ROOT / "runs/servoshell_webdriver" / f"probe_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        str(args.servoshell),
        f"--webdriver={args.port}",
        "--temporary-storage",
        args.url,
    ]
    if not args.no_headless:
        cmd.insert(1, "-z")

    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    report = {
        "servoshell": str(args.servoshell),
        "url": args.url,
        "port": args.port,
        "headless": not args.no_headless,
        "cmd": cmd,
        "output_dir": str(output_dir),
    }
    session_id = None
    ok = False
    try:
        status_code, status_body = wait_for_status(args.port, proc, args.timeout_sec)
        report["status"] = {"code": status_code, "body": status_body}

        new_session = {
            "capabilities": {
                "alwaysMatch": {
                    "browserName": "servo",
                }
            }
        }
        code, body = webdriver_request(args.port, "POST", "/session", new_session)
        report["new_session"] = {"code": code, "body": body}
        session_id = value_session_id(body)
        report["session_id"] = session_id

        code, truth = webdriver_request(
            args.port,
            "POST",
            f"/session/{session_id}/execute/sync",
            {
                "script": (
                    "return {"
                    "title: document.title,"
                    "url: location.href,"
                    "bodyTextLength: document.body ? document.body.innerText.length : 0,"
                    "revision: document.body && document.body.dataset.sessionRevision,"
                    "viewport: {width: innerWidth, height: innerHeight, dpr: devicePixelRatio}"
                    "};"
                ),
                "args": [],
            },
        )
        report["execute_truth"] = {"code": code, "body": truth}

        try:
            code, element = webdriver_request(
                args.port,
                "POST",
                f"/session/{session_id}/element",
                {"using": "css selector", "value": "#verify-action"},
            )
            report["find_verify_action"] = {"code": code, "body": element}
            target_id = element_id(element)
            if target_id:
                code, click = webdriver_request(
                    args.port,
                    "POST",
                    f"/session/{session_id}/element/{target_id}/click",
                    {},
                )
                report["click_verify_action"] = {"code": code, "body": click}
                _, after = webdriver_request(
                    args.port,
                    "POST",
                    f"/session/{session_id}/execute/sync",
                    {
                        "script": "return document.body && document.body.dataset.sessionRevision;",
                        "args": [],
                    },
                )
                report["post_click_revision"] = after
        except urllib.error.HTTPError as error:
            report["click_probe_http_error"] = {
                "code": error.code,
                "body": error.read().decode("utf-8", "replace"),
            }

        code, screenshot = webdriver_request(args.port, "GET", f"/session/{session_id}/screenshot")
        screenshot_path = output_dir / "screenshot.png"
        screenshot_path.write_bytes(base64.b64decode(screenshot["value"]))
        report["screenshot"] = {
            "code": code,
            "path": str(screenshot_path),
            "bytes": screenshot_path.stat().st_size,
        }
        ok = True
    except Exception as error:  # noqa: BLE001 - report all probe failures.
        report["error"] = repr(error)
    finally:
        if session_id:
            try:
                webdriver_request(args.port, "DELETE", f"/session/{session_id}", timeout=3)
            except Exception:
                pass
        proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
        report["returncode"] = proc.returncode
        report["stdout_head"] = stdout.splitlines()[:80]
        report["stderr_head"] = stderr.splitlines()[:120]
        report["ok"] = ok
        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    print(
        "SERVOSHELL_WEBDRIVER_PROBE "
        f"ok={str(ok).lower()} "
        f"report={output_dir / 'report.json'} "
        f"screenshot={output_dir / 'screenshot.png'}"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
