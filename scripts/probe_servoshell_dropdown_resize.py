#!/usr/bin/env python3
"""Probe ServoShell dropdown visibility after grow/shrink window resize."""

from __future__ import annotations

import argparse
import base64
import json
import struct
import subprocess
import sys
import time
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SERVOSHELL = Path(
    "/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell"
)
DEFAULT_FIXTURE = ROOT / "test_pages/dropdown_resize/index.html"


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
        except Exception as error:  # noqa: BLE001 - probe should report transport failures.
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


def png_size(png_bytes: bytes) -> dict[str, int]:
    if len(png_bytes) < 24 or not png_bytes.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("screenshot is not a PNG")
    width, height = struct.unpack(">II", png_bytes[16:24])
    return {"width": width, "height": height}


def execute(port: int, session_id: str, script: str):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/execute/sync",
        {"script": script, "args": []},
    )
    return body.get("value") if isinstance(body, dict) else body


def set_window_rect(port: int, session_id: str, width: int, height: int):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/window/rect",
        {"width": width, "height": height},
    )
    time.sleep(0.5)
    return body


def capture_phase(port: int, session_id: str, output_dir: Path, label: str):
    snapshot = execute(
        port,
        session_id,
        f"return window.__saccadeDropdownProbe.snapshot({json.dumps(label)});",
    )
    _, screenshot = webdriver_request(port, "GET", f"/session/{session_id}/screenshot")
    png_bytes = base64.b64decode(screenshot["value"])
    screenshot_path = output_dir / f"{label}.png"
    screenshot_path.write_bytes(png_bytes)
    screenshot_size = png_size(png_bytes)
    menu_rect = snapshot.get("menuRect") or {}
    screenshot_css_width = screenshot_size["width"] / max(
        1, snapshot.get("viewport", {}).get("devicePixelRatio", 1)
    )
    snapshot["screenshot"] = {
        "path": str(screenshot_path),
        "bytes": screenshot_path.stat().st_size,
        **screenshot_size,
        "css_width_estimate": screenshot_css_width,
    }
    snapshot["menuWithinScreenshotCssWidth"] = (
        float(menu_rect.get("right") or 0) <= screenshot_css_width + 1
    )
    snapshot["screenshotHorizontalOverflow"] = max(
        0.0,
        float(menu_rect.get("right") or 0) - screenshot_css_width,
    )
    return snapshot


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--url", default="file://" + str(DEFAULT_FIXTURE))
    parser.add_argument("--port", type=int, default=7092)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    parser.add_argument(
        "--sizes",
        default="900x700,1200x740,900x700",
        help="Comma-separated outer window sizes to probe.",
    )
    args = parser.parse_args()

    output_dir = args.output_dir or ROOT / "runs/servoshell_ui" / f"dropdown_resize_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)

    sizes = []
    for token in args.sizes.split(","):
        width, height = token.lower().split("x", 1)
        sizes.append((int(width), int(height)))

    cmd = [
        str(args.servoshell),
        f"--webdriver={args.port}",
        "--temporary-storage",
        args.url,
    ]

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
        "cmd": cmd,
        "output_dir": str(output_dir),
        "sizes": [{"width": width, "height": height} for width, height in sizes],
        "phases": [],
    }
    session_id = None
    ok = False
    try:
        status_code, status_body = wait_for_status(args.port, proc, args.timeout_sec)
        report["status"] = {"code": status_code, "body": status_body}
        _, body = webdriver_request(
            args.port,
            "POST",
            "/session",
            {"capabilities": {"alwaysMatch": {"browserName": "servo"}}},
        )
        session_id = value_session_id(body)
        report["session_id"] = session_id

        for index, (width, height) in enumerate(sizes):
            label = f"phase_{index}_{width}x{height}"
            rect_response = set_window_rect(args.port, session_id, width, height)
            phase = capture_phase(args.port, session_id, output_dir, label)
            phase["requestedOuterRect"] = {"width": width, "height": height}
            phase["webdriverRectResponse"] = rect_response
            report["phases"].append(phase)

        failures = []
        for phase in report["phases"]:
            if not phase.get("menuWithinViewport"):
                failures.append(
                    f"{phase['label']}: menu escaped JS viewport by "
                    f"{phase.get('horizontalOverflow')}x{phase.get('verticalOverflow')}"
                )
            if not phase.get("menuWithinScreenshotCssWidth"):
                failures.append(
                    f"{phase['label']}: menu escaped screenshot css width by "
                    f"{phase.get('screenshotHorizontalOverflow')}"
                )
            if not phase.get("logoutVisible"):
                failures.append(f"{phase['label']}: logout bottom not visible")

        report["failures"] = failures
        ok = not failures
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
        "SERVOSHELL_DROPDOWN_RESIZE "
        f"ok={str(ok).lower()} "
        f"report={output_dir / 'report.json'}"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
