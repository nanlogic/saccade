#!/usr/bin/env python3
"""Compare overlay hit-testing in Chrome and ServoShell.

This is an AI-027 reduction for GitHub-like dropdowns that are visually above
content but may still hit-test to page content underneath.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_DIR = ROOT / "scripts"
DEFAULT_FIXTURE = ROOT / "test_pages/overlay_hit_test/index.html"
DEFAULT_SERVOSHELL = Path(
    "/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell"
)

sys.path.insert(0, str(SCRIPT_DIR))
from chrome_reference_cdp import (  # noqa: E402
    capture_screenshot,
    find_chrome,
    free_port,
    launch_chrome,
    wait_for_cdp_client,
)


SNAPSHOT_JS = """
return window.__saccadeOverlayHitTest.snapshot(arguments[0], { click: true });
"""

CHROME_SNAPSHOT_EXPRESSION = """
JSON.stringify(window.__saccadeOverlayHitTest.snapshot(%s, { click: true }))
"""


def unix_ms() -> int:
    return int(time.time() * 1000)


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


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
        except Exception as error:  # noqa: BLE001
            last_error = repr(error)
            time.sleep(0.2)
    raise TimeoutError(f"WebDriver status was not ready; last_error={last_error}")


def value_session_id(response: dict) -> str:
    value = response.get("value") if isinstance(response, dict) else None
    if isinstance(value, dict) and isinstance(value.get("sessionId"), str):
        return value["sessionId"]
    if isinstance(response.get("sessionId"), str):
        return response["sessionId"]
    raise RuntimeError(f"new session response did not include a session id: {response}")


def execute(port: int, session_id: str, script: str, args=None):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/execute/sync",
        {"script": script, "args": args or []},
    )
    return body.get("value") if isinstance(body, dict) else body


def set_window_rect(port: int, session_id: str, width: int, height: int):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/window/rect",
        {"width": width, "height": height},
    )
    time.sleep(0.35)
    return body


def wait_for_fixture_ready(port: int, session_id: str, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last = None
    while time.monotonic() < deadline:
        last = execute(
            port,
            session_id,
            "return { readyState: document.readyState, hasProbe: typeof window.__saccadeOverlayHitTest === 'object', url: location.href };",
        )
        if last and last.get("readyState") in ("interactive", "complete") and last.get("hasProbe"):
            return last
        time.sleep(0.2)
    raise TimeoutError(f"fixture probe did not become ready: {last}")


def variant_url(base: Path, variant: str) -> str:
    raw = base.resolve().as_uri()
    return f"{raw}?variant={urllib.parse.quote(variant)}"


def parse_sizes(raw: str) -> list[tuple[int, int]]:
    sizes = []
    for token in raw.split(","):
        width, height = token.lower().split("x", 1)
        sizes.append((int(width), int(height)))
    return sizes


def parse_variants(raw: str) -> list[str]:
    return [item.strip() for item in raw.split(",") if item.strip()]


def classify_snapshot(engine: str, variant: str, width: int, height: int, snapshot: dict) -> dict:
    failures = []
    if not snapshot.get("menuWithinViewport"):
        failures.append(
            f"menu overflow {snapshot.get('horizontalOverflow')}x{snapshot.get('verticalOverflow')}"
        )
    if not snapshot.get("expectedHit"):
        hit = snapshot.get("hitBeforeClick") or {}
        failures.append(
            "center hit "
            f"{hit.get('clickablePath') or hit.get('path') or '<none>'} "
            f"instead of #{snapshot.get('expectedClickableId')}"
        )
    if not snapshot.get("clickReceiptOk"):
        failures.append(
            f"click receipt menu={snapshot.get('menuClicks')} underlay={snapshot.get('underlayClicks')}"
        )
    return {
        "engine": engine,
        "variant": variant,
        "requested_size": {"width": width, "height": height},
        "ok": not failures,
        "failures": failures,
        "snapshot": snapshot,
    }


def run_chrome(args: argparse.Namespace, variants: list[str], sizes: list[tuple[int, int]], output_dir: Path):
    chrome = find_chrome()
    rows = []
    for variant in variants:
        for width, height in sizes:
            user_data_dir = tempfile.mkdtemp(prefix="saccade-overlay-chrome-")
            stderr_log = output_dir / f"chrome_{variant}_{width}x{height}_stderr.log"
            port = free_port()
            proc = launch_chrome(chrome, port, user_data_dir, width, height, stderr_log)
            client = None
            try:
                _, client = wait_for_cdp_client(port, args.timeout_sec)
                client.call("Page.enable")
                client.call("Runtime.enable")
                client.call("Page.navigate", {"url": variant_url(args.fixture, variant)}, timeout=10)
                wait_for_chrome_probe(client, args.timeout_sec)
                expression = CHROME_SNAPSHOT_EXPRESSION % json.dumps(f"chrome_{variant}_{width}x{height}")
                result = client.call(
                    "Runtime.evaluate",
                    {"expression": expression, "returnByValue": True, "awaitPromise": True},
                    timeout=10,
                )
                snapshot = json.loads(result.get("result", {}).get("value", "{}"))
                screenshot_path = None
                if args.screenshots:
                    screenshot_path = output_dir / f"chrome_{variant}_{width}x{height}.png"
                    screenshot_path.write_bytes(
                        __import__("base64").b64decode(capture_screenshot(client))
                    )
                row = classify_snapshot("chrome", variant, width, height, snapshot)
                row["artifacts"] = {"stderr": str(stderr_log)}
                if screenshot_path:
                    row["artifacts"]["screenshot"] = str(screenshot_path)
                rows.append(row)
            except Exception as error:  # noqa: BLE001
                rows.append(
                    {
                        "engine": "chrome",
                        "variant": variant,
                        "requested_size": {"width": width, "height": height},
                        "ok": False,
                        "failures": [repr(error)],
                        "artifacts": {"stderr": str(stderr_log)},
                    }
                )
            finally:
                if client:
                    try:
                        client.close()
                    except Exception:
                        pass
                if proc.poll() is None:
                    proc.terminate()
                    try:
                        proc.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()
                shutil.rmtree(user_data_dir, ignore_errors=True)
    return rows


def wait_for_chrome_probe(client, timeout_sec: float) -> dict:
    deadline = time.monotonic() + timeout_sec
    last = None
    while time.monotonic() < deadline:
        result = client.call(
            "Runtime.evaluate",
            {
                "expression": "JSON.stringify({ readyState: document.readyState, hasProbe: typeof window.__saccadeOverlayHitTest === 'object', url: location.href })",
                "returnByValue": True,
            },
            timeout=3,
        )
        last = json.loads(result.get("result", {}).get("value", "{}"))
        if last.get("readyState") in ("interactive", "complete") and last.get("hasProbe"):
            return last
        time.sleep(0.2)
    raise TimeoutError(f"Chrome fixture probe did not become ready: {last}")


def run_servo(args: argparse.Namespace, variants: list[str], sizes: list[tuple[int, int]], output_dir: Path):
    rows = []
    port = args.port
    cmd = [
        str(args.servoshell),
        f"--webdriver={port}",
        "--temporary-storage",
        variant_url(args.fixture, variants[0]),
    ]
    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    session_id = None
    try:
        wait_for_status(port, proc, args.timeout_sec)
        _, body = webdriver_request(
            port,
            "POST",
            "/session",
            {"capabilities": {"alwaysMatch": {"browserName": "servo", "pageLoadStrategy": "none"}}},
        )
        session_id = value_session_id(body)

        for variant in variants:
            for width, height in sizes:
                set_window_rect(port, session_id, width, height)
                _, navigate_body = webdriver_request(
                    port,
                    "POST",
                    f"/session/{session_id}/url",
                    {"url": variant_url(args.fixture, variant)},
                    timeout=args.timeout_sec,
                )
                ready = wait_for_fixture_ready(port, session_id, args.timeout_sec)
                snapshot = execute(
                    port,
                    session_id,
                    SNAPSHOT_JS,
                    [f"servo_{variant}_{width}x{height}"],
                )
                row = classify_snapshot("servo", variant, width, height, snapshot)
                row["webdriver"] = {"navigate": navigate_body, "ready": ready}
                rows.append(row)
    except Exception as error:  # noqa: BLE001
        rows.append(
            {
                "engine": "servo",
                "variant": "*",
                "requested_size": None,
                "ok": False,
                "failures": [repr(error)],
            }
        )
    finally:
        process = finish_servoshell(port, session_id, proc)
        for row in rows:
            if row.get("engine") == "servo":
                row.setdefault("artifacts", {})["process"] = process
    return rows


def finish_servoshell(port: int, session_id: str | None, proc: subprocess.Popen) -> dict:
    report = {"attempted": False, "route": "webdriver_servo_shutdown"}
    if session_id:
        report["attempted"] = True
        try:
            status, body = webdriver_request(port, "DELETE", f"/session/{session_id}/servo/shutdown", timeout=3)
            report["ok"] = True
            report["status"] = status
            report["body"] = body
        except Exception as error:  # noqa: BLE001
            report["ok"] = False
            report["error"] = repr(error)
    else:
        report["skipped"] = "missing_session_id"

    try:
        stdout, stderr = proc.communicate(timeout=10)
        report["termination"] = "graceful_servo_shutdown"
    except subprocess.TimeoutExpired:
        if proc.poll() is None:
            proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=3)
            report["termination"] = "sigterm_after_shutdown_timeout"
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
            report["termination"] = "sigkill_after_shutdown_timeout"
    report["returncode"] = proc.returncode
    report["stdout_head"] = stdout.splitlines()[:40]
    report["stderr_head"] = stderr.splitlines()[:80]
    return report


def summarize(rows: list[dict]) -> dict:
    by_engine: dict[str, dict] = {}
    for row in rows:
        engine = row.get("engine", "unknown")
        current = by_engine.setdefault(engine, {"total": 0, "passed": 0, "failed": 0})
        current["total"] += 1
        if row.get("ok"):
            current["passed"] += 1
        else:
            current["failed"] += 1
    return {
        "total": len(rows),
        "passed": sum(1 for row in rows if row.get("ok")),
        "failed": sum(1 for row in rows if not row.get("ok")),
        "by_engine": by_engine,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--port", type=int, default=7101)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    parser.add_argument("--variants", default="absolute,fixed,static-child,transformed-underlay,primer-like")
    parser.add_argument("--sizes", default="1200x760,900x700,1200x760")
    parser.add_argument("--engines", default="chrome,servo")
    parser.add_argument("--screenshots", action="store_true")
    args = parser.parse_args()

    output_dir = args.output_dir or ROOT / "runs/ai027_github_ui_canary" / f"overlay_hit_test_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)
    variants = parse_variants(args.variants)
    sizes = parse_sizes(args.sizes)
    engines = {engine.strip() for engine in args.engines.split(",") if engine.strip()}

    rows = []
    if "chrome" in engines:
        rows.extend(run_chrome(args, variants, sizes, output_dir))
    if "servo" in engines:
        rows.extend(run_servo(args, variants, sizes, output_dir))

    report = {
        "engine": "saccade-overlay-hit-test-probe-v0",
        "created_at_unix_ms": unix_ms(),
        "fixture": str(args.fixture),
        "fixture_url": args.fixture.resolve().as_uri(),
        "variants": variants,
        "sizes": [{"width": width, "height": height} for width, height in sizes],
        "summary": summarize(rows),
        "rows": rows,
    }
    report["ok"] = report["summary"]["failed"] == 0
    report_path = output_dir / "report.json"
    write_json(report_path, report)
    print(
        "OVERLAY_HIT_TEST "
        f"ok={str(report['ok']).lower()} "
        f"passed={report['summary']['passed']} "
        f"failed={report['summary']['failed']} "
        f"report={report_path}"
    )
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
