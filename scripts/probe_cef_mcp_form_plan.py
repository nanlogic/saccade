#!/usr/bin/env python3
"""Verify the public MCP form tools against the granted CEF tab."""

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

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_MCP = ROOT / "target" / "release" / "saccade-mcp"
DEFAULT_FIXTURE = ROOT / "test_pages" / "form_plan" / "index.html"
SENTINELS = (
    "MCP_CEF_TEAM_VALUE",
    "MCP_CEF_SSN_VALUE",
    "MCP_CEF_PASSWORD_VALUE",
    "MCP_CEF_OVERWRITE_VALUE",
)
TOOLS = (
    "saccade.web.form_inventory",
    "saccade.web.form_compile_plan",
    "saccade.web.form_execute_plan",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    parser.add_argument("--headed", action="store_true")
    return parser.parse_args()


class McpClient:
    def __init__(self, binary: pathlib.Path) -> None:
        self.process = subprocess.Popen(
            [str(binary), "serve-stdio"],
            cwd=ROOT,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.next_id = 1

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        assert self.process.stdin is not None and self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        self.process.stdin.write(
            json.dumps(
                {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
            )
            + "\n"
        )
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
            raise RuntimeError(f"MCP tool {name} returned no structured content")
        return content

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def assert_value_free(value: Any, location: str) -> None:
    encoded = json.dumps(value, sort_keys=True)
    leaked = [sentinel for sentinel in SENTINELS if sentinel in encoded]
    if leaked:
        raise AssertionError(f"assignment values leaked through {location}: {leaked}")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    mcp_binary = args.mcp_bin.resolve()
    if not executable.is_file() or not mcp_binary.is_file() or not fixture.is_file():
        raise SystemExit("missing CEF app, MCP binary, or form fixture")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    output_dir = args.output_dir.resolve()
    replay_path = output_dir / "replay.jsonl"
    report_path = output_dir / "report.json"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-mcp-form-"))
    os.chmod(session, 0o700)
    grant_path = session / "grant.json"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
        }
    )
    command = [
        str(executable),
        f"--url={fixture.as_uri()}",
        f"--user-data-dir={session / 'profile'}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    if not args.headed:
        command.extend(["--use-views", "--initial-show-state=hidden"])

    browser: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    mcp: McpClient | None = None
    started = time.monotonic()
    stage = "launch"
    report: dict[str, Any]
    with (output_dir / "browser.log").open("wb") as browser_log:
        try:
            browser = subprocess.Popen(
                command, cwd=ROOT, env=env, stdout=browser_log, stderr=subprocess.STDOUT
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, browser, args.timeout_sec)
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            truth = wait_for_collector(control, args.timeout_sec)
            initial_revision = int(truth["page_revision"])

            stage = "mcp_initialize"
            mcp = McpClient(mcp_binary)
            mcp.request("initialize", {})
            listed = mcp.request("tools/list", {}).get("tools", [])
            listed_names = {tool.get("name") for tool in listed}
            missing_tools = sorted(set(TOOLS) - listed_names)
            if missing_tools:
                raise AssertionError(f"public MCP tools missing: {missing_tools}")

            stage = "mcp_grant"
            granted = mcp.tool(
                "saccade.tabs.grant_current",
                {
                    "grant_path": str(grant_path),
                    "reason": "CEF MCP ordinary-field draft gate",
                    "policy": {"explicit_user_grant": True, "local_dev_only": True},
                },
            )
            if granted.get("same_webview_attached") is not True:
                raise AssertionError("MCP did not attach to the granted CEF tab")
            tab_id = int(granted["tab"]["tab_id"])
            revision = int(granted["tab"]["page_revision"])
            if revision < initial_revision:
                raise AssertionError(
                    f"MCP grant returned older CEF revision {revision} < {initial_revision}"
                )

            stage = "mcp_inventory"
            inventory = mcp.tool(
                "saccade.web.form_inventory", {"tab_id": tab_id, "mode": "full"}
            )
            if (
                int(inventory.get("field_count", 0)) < 15
                or inventory.get("eligible_count") != 6
                or inventory.get("sensitive_count") != 2
            ):
                raise AssertionError(
                    "unexpected CEF inventory counts: "
                    f"fields={inventory.get('field_count')} "
                    f"eligible={inventory.get('eligible_count')} "
                    f"sensitive={inventory.get('sensitive_count')}"
                )
            assert_value_free(inventory, "MCP inventory")

            assignments: dict[str, Any] = {
                "id:team": SENTINELS[0],
                "id:region": "west",
                "id:instances": 24,
                "id:launch-date": "2026-08-15",
                "id:include-staging": True,
                "id:summary": "Ordinary capacity draft.",
                "id:user-note": SENTINELS[3],
                "id:ssn": SENTINELS[1],
                "id:password": SENTINELS[2],
            }
            policy = {
                "block_sensitive": True,
                "preserve_existing": True,
                "no_submit": True,
            }
            stage = "mcp_compile"
            compiled = mcp.tool(
                "saccade.web.form_compile_plan",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "assignments": assignments,
                    "policy": policy,
                },
            )
            eligible = {item["field_id"] for item in compiled.get("eligible", [])}
            expected = {
                "id:team",
                "id:region",
                "id:instances",
                "id:launch-date",
                "id:include-staging",
                "id:summary",
            }
            if eligible != expected:
                raise AssertionError(f"unexpected MCP plan: {compiled}")
            rejected = {item["field_id"] for item in compiled.get("rejected", [])}
            if not {"id:user-note", "id:ssn", "id:password"}.issubset(rejected):
                raise AssertionError("MCP plan did not preserve human/sensitive fields")
            assert_value_free(compiled, "MCP plan")

            stage = "mcp_execute"
            executed = mcp.tool(
                "saccade.web.form_execute_plan",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "expected_plan_id": compiled["plan_id"],
                    "assignments": assignments,
                    "policy": policy,
                },
            )
            if executed.get("receipt_verified") is not True:
                raise AssertionError(f"MCP execution receipt failed: {executed}")
            if {item["field_id"] for item in executed.get("filled", [])} != expected:
                raise AssertionError(f"MCP did not fill all ordinary fields: {executed}")
            if executed.get("failed") or executed.get("repair"):
                raise AssertionError(f"MCP execution needs repair: {executed}")
            assert_value_free(executed, "MCP execution")

            replay_text = replay_path.read_text()
            if any(sentinel in replay_text for sentinel in SENTINELS):
                raise AssertionError("CEF replay contained a task or sensitive value")
            replay_events = [json.loads(line) for line in replay_text.splitlines() if line]
            if not replay_events or not all(
                event.get("values_logged") is False for event in replay_events
            ):
                raise AssertionError("CEF replay was not value-free")

            report = {
                "schema": "saccade-cef-mcp-form-plan-v1",
                "verdict": "PASS",
                "engine": "cef",
                "public_tools": list(TOOLS),
                "same_webview_attached": True,
                "field_count": inventory["field_count"],
                "ordinary_fields_filled": len(expected),
                "sensitive_fields_blocked": 2,
                "human_values_preserved": True,
                "receipt_verified": True,
                "submitted": False,
                "values_logged": False,
                "replay_events": len(replay_events),
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-mcp-form-plan-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
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
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_MCP_FORM verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
