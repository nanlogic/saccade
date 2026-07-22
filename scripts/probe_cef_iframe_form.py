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

NESTED_INNER_HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Third form layer</title></head>
<body><form><input id="destiny" placeholder="Destiny" data-sensitive="none"></form></body>
</html>
"""

NESTED_MIDDLE_HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Second form layer</title></head>
<body>
  <form><input id="current-crush" placeholder="Current Crush Name" data-sensitive="none"></form>
  <iframe title="Third form layer" src="{inner_url}"
          style="width:760px;height:180px;border:1px solid #888"></iframe>
</body></html>
"""

NESTED_OUTER_HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>First form layer</title></head>
<body>
  <form><input id="first-crush" placeholder="First Crush" data-sensitive="none"></form>
  <iframe title="Second form layer" src="{middle_url}"
          style="width:820px;height:360px;border:1px solid #888"></iframe>
</body></html>
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
    parser.add_argument("--mcp-bin", type=pathlib.Path)
    parser.add_argument("--public-url")
    parser.add_argument("--nested", action="store_true")
    parser.add_argument("--headed", action="store_true")
    parser.add_argument("--keep-open-sec", type=float, default=0.0)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    executable = args.exe.resolve()
    if not executable.is_file():
        raise SystemExit(f"missing Saccade executable: {executable}")
    args.output.parent.mkdir(parents=True, exist_ok=True)

    child_server: http.server.ThreadingHTTPServer | None = None
    parent_server: http.server.ThreadingHTTPServer | None = None
    fixture_servers: list[http.server.ThreadingHTTPServer] = []
    if args.public_url:
        parent_url = args.public_url
    elif args.nested:
        inner_server, _ = serve(NESTED_INNER_HTML.encode())
        fixture_servers.append(inner_server)
        inner_url = f"http://127.0.0.1:{inner_server.server_port}/inner.html"
        middle_server, _ = serve(NESTED_MIDDLE_HTML.format(inner_url=inner_url).encode())
        fixture_servers.append(middle_server)
        middle_url = f"http://127.0.0.1:{middle_server.server_port}/middle.html"
        child_server, _ = serve(
            NESTED_OUTER_HTML.format(middle_url=middle_url).encode()
        )
        fixture_servers.append(child_server)
        child_url = f"http://127.0.0.1:{child_server.server_port}/outer.html"
        parent_server, _ = serve(PARENT_HTML.format(child_url=child_url).encode())
        fixture_servers.append(parent_server)
        parent_url = f"http://127.0.0.1:{parent_server.server_port}/index.html"
    else:
        child_server, _ = serve(CHILD_HTML.encode())
        fixture_servers.append(child_server)
        child_url = f"http://127.0.0.1:{child_server.server_port}/child.html"
        parent_server, _ = serve(PARENT_HTML.format(child_url=child_url).encode())
        fixture_servers.append(parent_server)
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
        "--window-size=1100,800",
    ]
    if not args.headed:
        command.append("--initial-show-state=hidden")

    process: subprocess.Popen[bytes] | None = None
    mcp: Any | None = None
    report: dict[str, Any]
    exit_code = 0
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

        tab_id: int | None = None
        if args.mcp_bin:
            from probe_cef_mcp_form_plan import McpClient

            mcp_binary = args.mcp_bin.resolve()
            if not mcp_binary.is_file():
                raise AssertionError(f"missing MCP binary: {mcp_binary}")
            mcp = McpClient(mcp_binary)
            mcp.request("initialize", {})
            granted = mcp.tool(
                "saccade.tabs.grant_current",
                {
                    "grant_path": str(grant_path),
                    "reason": "cross-origin iframe native form receipt gate",
                    "policy": {"explicit_user_grant": True, "local_dev_only": True},
                },
            )
            if granted.get("same_webview_attached") is not True:
                raise AssertionError(f"MCP did not attach to Saccade: {granted}")
            tab_id = int(granted["tab"]["tab_id"])

        expected_ids = (
            set()
            if args.public_url or args.nested
            else {"id:project-name", "id:homepage"}
        )
        inventory_deadline = time.monotonic() + args.timeout_sec
        inventory: dict[str, Any] = {}
        field_ids: set[Any] = set()
        while time.monotonic() < inventory_deadline:
            try:
                inventory = (
                    mcp.tool(
                        "saccade.web.form_inventory", {"tab_id": tab_id, "mode": "full"}
                    )
                    if mcp
                    else control.call("form_inventory")
                )
            except RuntimeError as error:
                if args.public_url and (
                    "layout changed while action was pending" in str(error)
                    or "renderer collector is not ready" in str(error)
                    or "renderer form command timed out" in str(error)
                ):
                    time.sleep(0.25)
                    continue
                raise
            field_ids = {
                field.get("field_id") for field in inventory.get("fields", [])
            }
            if args.nested:
                nested_labels = {"First Crush", "Current Crush Name", "Destiny"}
                expected_ids = {
                    str(field["field_id"])
                    for field in inventory.get("fields", [])
                    if field.get("label") in nested_labels
                }
                if len(expected_ids) != len(nested_labels):
                    expected_ids = set()
            if args.public_url:
                public_candidates = [
                    field
                    for field in inventory.get("fields", [])
                    if field.get("eligible") is True
                    and field.get("sensitivity") == "none"
                    and field.get("value_state") == "empty"
                    and field.get("type")
                    in {"text", "email", "tel", "url", "search", "textarea"}
                ]
                target_labels = {"First Crush", "Current Crush Name", "Destiny"}
                labelled_candidates = [
                    field
                    for field in public_candidates
                    if field.get("label") in target_labels
                ]
                if len(labelled_candidates) == len(target_labels):
                    public_candidates = labelled_candidates
                expected_ids = {
                    str(field["field_id"]) for field in public_candidates[:3]
                }
            if expected_ids and expected_ids.issubset(field_ids):
                break
            time.sleep(0.1)
        if not expected_ids or not expected_ids.issubset(field_ids):
            raise AssertionError(f"embedded fields were not discovered: {inventory}")
        if inventory.get("embedded_frame") is not True:
            raise AssertionError(f"embedded frame was not selected: {inventory}")
        expected_frame_scope = (
            "composited"
            if int(inventory.get("form_frame_count", 0)) > 1
            else "embedded"
        )
        if inventory.get("frame_scope") != expected_frame_scope:
            raise AssertionError(f"unexpected frame scope: {inventory}")
        if int(inventory.get("frame_count_scanned", 0)) < 2:
            raise AssertionError(f"frame fan-out was not observed: {inventory}")
        if inventory.get("frame_selection_ambiguous") is not False:
            raise AssertionError(f"unified frame routing was marked ambiguous: {inventory}")
        if inventory.get("frame_aggregation_complete") is not True:
            raise AssertionError(f"frame aggregation was incomplete: {inventory}")
        if args.nested and int(inventory.get("form_frame_count", 0)) != 3:
            raise AssertionError(f"three visible form layers were not unified: {inventory}")

        revision = int(inventory["page_revision"])
        if args.public_url or args.nested:
            fields_by_id = {
                field["field_id"]: field for field in inventory.get("fields", [])
            }
            assignments = {
                field_id: (
                    "saccade-public-probe@example.invalid"
                    if fields_by_id[field_id].get("type") == "email"
                    else "https://example.invalid/saccade-public-probe"
                    if fields_by_id[field_id].get("type") == "url"
                    else "5550100"
                    if fields_by_id[field_id].get("type") == "tel"
                    else "Saccade nested iframe probe"
                )
                for field_id in expected_ids
            }
        else:
            assignments = {
                "id:project-name": "Saccade iframe dogfood",
                "id:homepage": "https://example.invalid/saccade",
            }
        inspection = (
            mcp.tool(
                "saccade.web.inspect_fields",
                {"tab_id": tab_id, "fields": sorted(expected_ids)},
            )
            if mcp
            else control.call(
                "inspect_fields",
                {
                    "basis_page_revision": revision,
                    "fields": sorted(expected_ids),
                },
            )
        )
        inspected = {item.get("field_id") for item in inspection.get("fields", [])}
        if inspected != expected_ids:
            raise AssertionError(f"unexpected embedded inspection: {inspection}")

        policy = {"block_sensitive": True, "preserve_existing": True, "no_submit": True}
        plan = (
            mcp.tool(
                "saccade.web.form_compile_plan",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "assignments": assignments,
                    "policy": policy,
                },
            )
            if mcp
            else control.call(
                "form_compile_plan",
                {"basis_page_revision": revision, "assignments": assignments},
            )
        )
        planned = {item.get("field_id") for item in plan.get("eligible", [])}
        if planned != expected_ids:
            raise AssertionError(f"unexpected embedded plan: {plan}")
        direct_type_blocked = False
        try:
            control.call(
                "type_field_text",
                {
                    "basis_page_revision": revision,
                    "field_id": sorted(expected_ids)[0],
                    "text": "plan bypass must not be typed",
                },
            )
        except RuntimeError as error:
            direct_type_blocked = "POLICY_BLOCKED" in str(error)
        if not direct_type_blocked:
            raise AssertionError("ordinary iframe field bypassed compile/execute policy")
        execution_params = {
            "basis_page_revision": revision,
            "expected_plan_id": plan["plan_id"],
            "assignments": assignments,
        }
        execution = (
            mcp.tool(
                "saccade.web.form_execute_plan",
                {"tab_id": tab_id, **execution_params, "policy": policy},
            )
            if mcp
            else control.call("form_execute_plan", execution_params)
        )
        if execution.get("receipt_verified") is not True:
            raise AssertionError(f"embedded execution receipt failed: {execution}")
        if mcp and execution.get("verification_complete") is not True:
            raise AssertionError(f"MCP rejected embedded native receipts: {execution}")
        receipts = execution.get("native_input_receipts", [])
        if (
            execution.get("same_webview_native_input") is not True
            or len(receipts) != len(expected_ids)
        ):
            raise AssertionError(f"embedded native receipts missing: {execution}")
        if not all(
            receipt.get("schema") == "saccade.native_input_receipt/1"
            and receipt.get("same_webview") is True
            and receipt.get("dispatch_acknowledged") is True
            and receipt.get("postcondition_verified") is True
            and receipt.get("value_logged") is False
            for receipt in receipts
        ):
            raise AssertionError(f"embedded native receipt invalid: {execution}")
        if {item.get("field_id") for item in execution.get("filled", [])} != expected_ids:
            raise AssertionError(f"embedded fields were not filled: {execution}")

        report = {
            "ok": True,
            "url": parent_url,
            "parent_origin": (
                f"127.0.0.1:{parent_server.server_port}" if parent_server else None
            ),
            "child_origin": (
                f"127.0.0.1:{child_server.server_port}" if child_server else None
            ),
            "frame_count_scanned": inventory["frame_count_scanned"],
            "form_frame_count": inventory["form_frame_count"],
            "frame_scope": inventory["frame_scope"],
            "frame_aggregation_complete": inventory["frame_aggregation_complete"],
            "frame_inventory": inventory.get("frame_inventory", []),
            "field_ids": sorted(field_ids),
            "inspected": sorted(inspected),
            "planned": sorted(planned),
            "filled": sorted(item["field_id"] for item in execution["filled"]),
            "receipt_verified": execution["receipt_verified"],
            "native_input_receipt_count": len(receipts),
            "same_webview_native_input": execution["same_webview_native_input"],
            "direct_type_bypass_blocked": direct_type_blocked,
            "route": "mcp" if mcp else "engine_control",
            "mcp_verification_complete": (
                execution.get("verification_complete") if mcp else None
            ),
            "submitted": False,
        }
        if args.keep_open_sec > 0:
            time.sleep(min(args.keep_open_sec, 120.0))
    except Exception as error:
        exit_code = 1
        report = {
            "ok": False,
            "url": parent_url,
            "route": "mcp" if mcp else "engine_control",
            "submitted": False,
            "error": str(error),
        }
    finally:
        if mcp is not None:
            mcp.close()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
        for server in reversed(fixture_servers):
            server.shutdown()
            server.server_close()

    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
