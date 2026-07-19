#!/usr/bin/env python3
"""Verify Chrome-style CEF download handling and metadata-only MCP receipts."""

from __future__ import annotations

import argparse
import functools
import hashlib
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

from probe_ai038_conversational_dogfood import McpClient
from probe_cef_truth_reflex import EngineControl, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_MCP = ROOT / "target" / "release" / "saccade-mcp"
DEFAULT_FIXTURE = ROOT / "test_pages" / "downloads" / "index.html"
EXPECTED_FILE_NAMES = {"saccade-free-sound-license.txt", "free-sound-license.txt"}
PAYLOAD_SENTINEL = "Asset payload intentionally omitted."


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    return parser.parse_args()


def one_action(actions: list[dict[str, Any]], label: str) -> dict[str, Any]:
    matches = [action for action in actions if action.get("label") == label]
    if len(matches) != 1:
        raise AssertionError(f"expected one {label!r} action, got {matches}")
    return matches[0]


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    mcp_binary = args.mcp_bin.resolve()
    if not executable.is_file() or not fixture.is_file() or not mcp_binary.is_file():
        raise SystemExit("missing CEF app, download fixture, or MCP binary")

    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    download_dir = output / "downloads"
    download_dir.mkdir(exist_ok=True)
    report_path = output / "report.json"
    replay_path = output / "replay.jsonl"
    session = pathlib.Path(
        tempfile.mkdtemp(prefix="saccade-download-", dir="/private/tmp")
    )
    os.chmod(session, 0o700)
    grant_path = session / "grant.json"
    pointer_path = session / "current-grant-path"
    pointer_path.write_text(str(grant_path) + "\n", encoding="utf-8")
    os.chmod(pointer_path, 0o600)

    handler = functools.partial(QuietHandler, directory=str(fixture.parent))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    url = f"http://127.0.0.1:{server.server_port}/{fixture.name}"

    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
            "SACCADE_DOWNLOAD_DIR": str(download_dir),
        }
    )
    command = [
        str(executable),
        f"--url={url}",
        f"--user-data-dir={session / 'profile'}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    browser: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    mcp: McpClient | None = None
    stage = "launch"
    started = time.monotonic()
    report: dict[str, Any]
    with (output / "browser.log").open("wb") as browser_log:
        try:
            browser = subprocess.Popen(
                command,
                cwd=ROOT,
                env=env,
                stdout=browser_log,
                stderr=subprocess.STDOUT,
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, browser, args.timeout_sec)
            capabilities = set(grant["engine_adapter"]["capabilities"])
            if "downloads" not in capabilities:
                raise AssertionError("CEF adapter did not advertise downloads")
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                str(grant["control_capability"]["token"]),
            )

            stage = "mcp_attach"
            mcp = McpClient(mcp_binary, env)
            mcp.request("initialize", {})
            attached = mcp.tool("saccade.tabs.grant_current", {})
            tab_id = int(attached["tab"]["tab_id"])
            initial = mcp.tool("saccade.downloads.list", {"tab_id": tab_id})
            if initial.get("download_count") != 0:
                raise AssertionError(f"new tab exposed prior downloads: {initial}")

            stage = "download_action"
            time.sleep(0.15)
            action_map = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
            action = one_action(
                action_map.get("actions", []), "Download free sound license"
            )
            acted = mcp.tool(
                "saccade.web.act",
                {
                    "tab_id": tab_id,
                    "action_id": action["action_id"],
                    "basis_page_revision": int(action["basis_page_revision"]),
                },
            )
            if acted.get("status") != "ok":
                raise AssertionError(f"download action was not dispatched: {acted}")

            stage = "download_receipt"
            deadline = time.monotonic() + 12
            completed: dict[str, Any] | None = None
            receipts: dict[str, Any] = {}
            downloaded_file: pathlib.Path | None = None
            while time.monotonic() < deadline:
                receipts = mcp.tool("saccade.downloads.list", {"tab_id": tab_id})
                for candidate in receipts.get("downloads", []):
                    if candidate.get("status") == "complete":
                        completed = candidate
                        name = str(candidate.get("file_name") or "")
                        downloaded_file = download_dir / name
                        break
                if completed is not None and downloaded_file and downloaded_file.is_file():
                    break
                time.sleep(0.1)
            if completed is None or downloaded_file is None or not downloaded_file.is_file():
                raise AssertionError(
                    f"download did not complete with a file and receipt: {receipts}"
                )
            if completed.get("file_name") not in EXPECTED_FILE_NAMES:
                raise AssertionError(f"unsafe or unexpected file name: {completed}")
            if downloaded_file.read_text(encoding="utf-8").strip().splitlines()[-1] != PAYLOAD_SENTINEL:
                raise AssertionError("downloaded fixture payload did not match")
            if (
                receipts.get("full_paths_exposed") is not False
                or receipts.get("contents_exposed") is not False
                or receipts.get("auto_execute_allowed") is not False
                or completed.get("full_path_exposed") is not False
                or completed.get("contents_exposed") is not False
                or completed.get("auto_executed") is not False
            ):
                raise AssertionError(f"download receipt broadened file authority: {receipts}")
            public_blob = json.dumps(mcp.public_results, sort_keys=True)
            if str(download_dir) in public_blob or PAYLOAD_SENTINEL in public_blob:
                raise AssertionError("MCP exposed the full download path or file contents")

            report = {
                "schema": "saccade-cef-downloads-v1",
                "verdict": "PASS",
                "human_chrome_style_handler": True,
                "agent_download_action": True,
                "download_complete": True,
                "download_file_name": completed.get("file_name"),
                "download_mime_type": completed.get("mime_type"),
                "download_sha256": sha256(downloaded_file),
                "source_origin": completed.get("source_origin"),
                "full_path_exposed": False,
                "file_contents_exposed": False,
                "auto_execute_allowed": False,
                "values_logged": False,
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-downloads-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
                "values_logged": False,
                "duration_sec": round(time.monotonic() - started, 3),
            }
        finally:
            if mcp is not None:
                mcp.close()
            if control is not None:
                try:
                    control.call("close")
                except Exception:
                    pass
            if browser is not None:
                try:
                    browser.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    browser.terminate()
                    browser.wait(timeout=5)
            server.shutdown()
            server.server_close()
            server_thread.join(timeout=2)
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"CEF_DOWNLOADS verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
