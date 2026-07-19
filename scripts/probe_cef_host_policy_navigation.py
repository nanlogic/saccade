#!/usr/bin/env python3
"""Verify host-owned actions, canonical targets, navigation, and 20-run readiness."""

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
from probe_cef_truth_reflex import EngineControl, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_MCP = ROOT / "target" / "release" / "saccade-mcp"
DEFAULT_FIXTURE = ROOT / "test_pages" / "host_policy_navigation" / "index.html"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def one_action(actions: list[dict[str, Any]], label: str) -> dict[str, Any]:
    matches = [action for action in actions if action.get("label") == label]
    if len(matches) != 1:
        raise AssertionError(f"expected one canonical {label!r} action, got {matches}")
    return matches[0]


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    mcp_binary = args.mcp_bin.resolve()
    if not executable.is_file() or not fixture.is_file() or not mcp_binary.is_file():
        raise SystemExit("missing CEF app, host-policy fixture, or MCP binary")

    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    replay_path = output / "replay.jsonl"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-r12-r13-"))
    os.chmod(session, 0o700)
    grant_path = session / "grant.json"
    pointer_path = session / "current-grant-path"
    pointer_path.write_text(str(grant_path) + "\n", encoding="utf-8")
    os.chmod(pointer_path, 0o600)
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
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
    browser: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    mcp: McpClient | None = None
    stage = "launch"
    started = time.monotonic()
    report: dict[str, Any]
    with (output / "browser.log").open("wb") as browser_log:
        try:
            browser = subprocess.Popen(
                command, cwd=ROOT, env=env, stdout=browser_log, stderr=subprocess.STDOUT
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, browser, args.timeout_sec)
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                str(grant["control_capability"]["token"]),
            )
            required = {"back", "forward", "reload", "navigate", "act", "actions"}
            capabilities = set(grant["engine_adapter"]["capabilities"])
            if not required.issubset(capabilities):
                raise AssertionError(f"missing CEF capabilities: {sorted(required - capabilities)}")

            stage = "mcp_attach"
            mcp = McpClient(mcp_binary, env)
            mcp.request("initialize", {})
            attached = mcp.tool("saccade.tabs.grant_current", {})
            tab_id = int(attached["tab"]["tab_id"])
            if attached.get("collector_ready") is not True:
                raise AssertionError(f"collector was not ready at MCP attach: {attached}")

            stage = "canonical_actions"
            time.sleep(0.15)
            action_map = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
            actions = action_map.get("actions", [])
            details = one_action(actions, "Read details")
            purchase = one_action(actions, "Purchase item")
            submit = one_action(actions, "Submit order")
            for action in (details, purchase, submit):
                if (
                    action.get("authorization_source") != "llm_host_policy"
                    or action.get("requires_user_confirmation") is not False
                ):
                    raise AssertionError(f"Saccade retained a site-action approval gate: {action}")
            first_ids = {action["label"]: action["action_id"] for action in (details, purchase, submit)}
            time.sleep(0.15)
            rescanned = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
            second_ids = {
                label: one_action(rescanned.get("actions", []), label)["action_id"]
                for label in first_ids
            }
            if second_ids != first_ids:
                raise AssertionError(f"canonical action IDs changed after DOM replacement: {first_ids} -> {second_ids}")

            stage = "host_owned_action"
            acted = mcp.tool(
                "saccade.web.act",
                {
                    "tab_id": tab_id,
                    "action_id": purchase["action_id"],
                    "basis_page_revision": int(purchase["basis_page_revision"]),
                },
            )
            receipt = control.call("next_receipt", {"timeout_ms": 3000})
            if acted.get("status") != "ok" or receipt.get("verified") is not True:
                raise AssertionError(f"host-owned action was not verified: {acted} {receipt}")

            stage = "navigation"
            page2 = fixture.with_name("page2.html").as_uri()
            nav = mcp.tool(
                "saccade.browser.navigate",
                {"tab_id": tab_id, "action": "navigate", "url": page2},
            )
            if nav.get("url") != page2 or nav.get("changed") is not True:
                raise AssertionError(f"navigate did not settle in the same WebView: {nav}")
            back = mcp.tool("saccade.browser.navigate", {"tab_id": tab_id, "action": "back"})
            if pathlib.Path(str(back.get("url", "")).removeprefix("file://")).name != "index.html":
                raise AssertionError(f"Back did not return to the fixture: {back}")
            forward = mcp.tool("saccade.browser.navigate", {"tab_id": tab_id, "action": "forward"})
            if forward.get("url") != page2:
                raise AssertionError(f"Forward did not return to page two: {forward}")
            reload_result = mcp.tool(
                "saccade.browser.navigate", {"tab_id": tab_id, "action": "reload"}
            )
            if reload_result.get("changed") is not True or reload_result.get("url") != page2:
                raise AssertionError(f"Reload did not settle: {reload_result}")

            stage = "twenty_dynamic_runs"
            readiness: list[dict[str, Any]] = []
            for run in range(1, args.runs + 1):
                run_url = f"{fixture.as_uri()}?run={run}"
                result = mcp.tool(
                    "saccade.browser.navigate",
                    {"tab_id": tab_id, "action": "navigate", "url": run_url},
                )
                current_actions = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
                canonical = one_action(current_actions.get("actions", []), "Read details")
                readiness.append(
                    {
                        "run": run,
                        "url": result.get("url"),
                        "page_revision": result.get("page_revision"),
                        "collector_ready": result.get("shell", {}).get("collector_ready"),
                        "canonical_action_id": canonical.get("action_id"),
                    }
                )
            if any(
                item["url"] != f"{fixture.as_uri()}?run={item['run']}"
                or item["collector_ready"] is not True
                for item in readiness
            ):
                raise AssertionError(f"dynamic readiness failed: {readiness}")

            report = {
                "schema": "saccade-host-policy-navigation-readiness-v1",
                "verdict": "PASS",
                "site_action_policy_owner": "llm_host",
                "saccade_confirmation_required": False,
                "navigation": {"back": True, "forward": True, "reload": True},
                "canonical_action_ids": first_ids,
                "dynamic_runs": readiness,
                "dynamic_runs_passed": len(readiness),
                "values_logged": False,
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-host-policy-navigation-readiness-v1",
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
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"CEF_HOST_POLICY_NAVIGATION verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
