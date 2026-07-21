#!/usr/bin/env python3
"""Exercise fixed form inventory and execution inside a cross-origin iframe."""

from __future__ import annotations

import argparse
import http.server
import json
import os
import pathlib
import secrets
import socket
import subprocess
import tempfile
import threading
import time
from typing import Any


PARENT_HTML = """<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Saccade iframe form host</title></head>
<body>
  <h1>Visible embedded application form</h1>
  <iframe title="Application form" src="{child_url}"
          style="width:900px;height:500px;border:1px solid #888"></iframe>
</body>
</html>
"""

CHILD_HTML = """<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Embedded application form</title></head>
<body>
  <form>
    <label for="project-name">Project Name</label>
    <input id="project-name" name="project_name" data-sensitive="none">
    <label for="homepage">Homepage URL</label>
    <input id="homepage" name="homepage" type="url" data-sensitive="none">
  </form>
</body>
</html>
"""


class StaticHandler(http.server.BaseHTTPRequestHandler):
    body = b""

    def do_GET(self) -> None:  # noqa: N802
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(self.body)))
        self.end_headers()
        self.wfile.write(self.body)

    def log_message(self, _format: str, *_args: object) -> None:
        pass


def serve(body: bytes) -> tuple[http.server.ThreadingHTTPServer, threading.Thread]:
    handler = type("FixtureHandler", (StaticHandler,), {"body": body})
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, thread


class EngineControl:
    def __init__(self, endpoint: str, capability: str) -> None:
        self.endpoint = endpoint
        self.capability = capability
        self.request_id = 0

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self.request_id += 1
        request = {
            "id": self.request_id,
            "method": method,
            "params": params or {},
            "capability": self.capability,
        }
        encoded = json.dumps(request, separators=(",", ":")).encode() + b"\n"
        if os.name == "nt":
            with open(self.endpoint, "r+b", buffering=0) as stream:
                stream.write(encoded)
                response_bytes = stream.read(1024 * 1024)
        else:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
                stream.settimeout(12.0)
                stream.connect(self.endpoint)
                stream.sendall(encoded)
                response_bytes = b""
                while b"\n" not in response_bytes:
                    chunk = stream.recv(65536)
                    if not chunk:
                        break
                    response_bytes += chunk
        if not response_bytes:
            raise RuntimeError(f"control connection closed during {method}")
        response = json.loads(response_bytes.split(b"\n", 1)[0])
        if not response.get("ok"):
            raise RuntimeError(f"{method} failed: {response.get('error')}")
        return response.get("result") or {}


def wait_json(path: pathlib.Path, process: subprocess.Popen[bytes], timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Saccade exited before grant, status={process.returncode}")
        try:
            if path.stat().st_size:
                return json.loads(path.read_text(encoding="utf-8-sig"))
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.05)
    raise TimeoutError("timed out waiting for Saccade control grant")


def wait_ready(control: EngineControl, timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("truth")
        if last.get("collector_ready"):
            return last
        time.sleep(0.05)
    raise TimeoutError(f"renderer collector did not become ready: {last}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    executable = args.exe.resolve()
    if not executable.is_file():
        raise SystemExit(f"missing Saccade executable: {executable}")
    args.output.parent.mkdir(parents=True, exist_ok=True)

    child_server, _ = serve(CHILD_HTML.encode())
    child_url = f"http://127.0.0.1:{child_server.server_port}/child.html"
    parent_server, _ = serve(PARENT_HTML.format(child_url=child_url).encode())
    parent_url = f"http://127.0.0.1:{parent_server.server_port}/index.html"

    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-iframe-form-"))
    grant_path = session / "grant.json"
    replay_path = session / "replay.jsonl"
    profile_path = session / "profile"
    profile_path.mkdir()
    endpoint = (
        rf"\\.\pipe\Saccade-{secrets.token_hex(16)}"
        if os.name == "nt"
        else str(session / "control.sock")
    )
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": endpoint,
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_REFLEX_GATE": "1",
        }
    )
    command = [
        str(executable),
        f"--url={parent_url}",
        f"--user-data-dir={profile_path}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--initial-show-state=hidden",
        "--window-size=1100,800",
    ]

    process: subprocess.Popen[bytes] | None = None
    report: dict[str, Any]
    try:
        process = subprocess.Popen(
            command,
            cwd=executable.parent,
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        grant = wait_json(grant_path, process, args.timeout_sec)
        control = EngineControl(
            str(grant["control_endpoint"]["path"]),
            str(grant["control_capability"]["token"]),
        )
        wait_ready(control, args.timeout_sec)

        expected_ids = {"id:project-name", "id:homepage"}
        inventory_deadline = time.monotonic() + args.timeout_sec
        inventory: dict[str, Any] = {}
        field_ids: set[Any] = set()
        while time.monotonic() < inventory_deadline:
            inventory = control.call("form_inventory")
            field_ids = {
                field.get("field_id") for field in inventory.get("fields", [])
            }
            if expected_ids.issubset(field_ids):
                break
            time.sleep(0.1)
        if not expected_ids.issubset(field_ids):
            raise AssertionError(f"embedded fields were not discovered: {inventory}")
        if inventory.get("embedded_frame") is not True:
            raise AssertionError(f"embedded frame was not selected: {inventory}")
        if inventory.get("frame_scope") != "embedded":
            raise AssertionError(f"unexpected frame scope: {inventory}")
        if int(inventory.get("frame_count_scanned", 0)) < 2:
            raise AssertionError(f"frame fan-out was not observed: {inventory}")
        if inventory.get("frame_selection_ambiguous") is not False:
            raise AssertionError(f"single form frame was marked ambiguous: {inventory}")

        revision = int(inventory["page_revision"])
        assignments = {
            "id:project-name": "Saccade iframe dogfood",
            "id:homepage": "https://example.invalid/saccade",
        }
        inspection = control.call(
            "inspect_fields",
            {
                "basis_page_revision": revision,
                "fields": sorted(expected_ids),
            },
        )
        inspected = {item.get("field_id") for item in inspection.get("fields", [])}
        if inspected != expected_ids:
            raise AssertionError(f"unexpected embedded inspection: {inspection}")

        plan = control.call(
            "form_compile_plan",
            {"basis_page_revision": revision, "assignments": assignments},
        )
        planned = {item.get("field_id") for item in plan.get("eligible", [])}
        if planned != expected_ids:
            raise AssertionError(f"unexpected embedded plan: {plan}")
        execution = control.call(
            "form_execute_plan",
            {
                "basis_page_revision": revision,
                "expected_plan_id": plan["plan_id"],
                "assignments": assignments,
            },
        )
        if execution.get("receipt_verified") is not True:
            raise AssertionError(f"embedded execution receipt failed: {execution}")
        if {item.get("field_id") for item in execution.get("filled", [])} != expected_ids:
            raise AssertionError(f"embedded fields were not filled: {execution}")

        report = {
            "ok": True,
            "parent_origin": f"127.0.0.1:{parent_server.server_port}",
            "child_origin": f"127.0.0.1:{child_server.server_port}",
            "frame_count_scanned": inventory["frame_count_scanned"],
            "form_frame_count": inventory["form_frame_count"],
            "frame_scope": inventory["frame_scope"],
            "field_ids": sorted(field_ids),
            "inspected": sorted(inspected),
            "planned": sorted(planned),
            "filled": sorted(item["field_id"] for item in execution["filled"]),
            "receipt_verified": execution["receipt_verified"],
            "submitted": False,
        }
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
        parent_server.shutdown()
        parent_server.server_close()
        child_server.shutdown()
        child_server.server_close()

    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
