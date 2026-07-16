#!/usr/bin/env python3
"""Measure GitHub New Issue editors and account-menu actions in CEF."""

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
DEFAULT_PROFILE = (
    pathlib.Path.home()
    / "Library"
    / "Application Support"
    / "Saccade"
    / "CEF"
    / "Profiles"
    / "default"
)
DEFAULT_URL = "https://github.com/servo/servo/issues/new"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--profile", type=pathlib.Path, default=DEFAULT_PROFILE)
    parser.add_argument("--url", default=DEFAULT_URL)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=45.0)
    return parser.parse_args()


def wait_for_issue_inventory(
    control: EngineControl, timeout: float
) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, Any]]]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        try:
            last = control.call("form_inventory")
        except RuntimeError as error:
            transient = (
                "page changed while form command was pending",
                "renderer collector is not ready",
            )
            if not any(marker in str(error) for marker in transient):
                raise
            time.sleep(0.2)
            continue
        fields = last.get("fields", [])
        titles = [
            field
            for field in fields
            if field.get("visible") is True
            and field.get("type") in {"text", "textarea", "contenteditable"}
            and "title" in str(field.get("label") or "").lower()
        ]
        bodies = [
            field
            for field in fields
            if field.get("visible") is True
            and field.get("type") in {"textarea", "contenteditable"}
            and any(
                marker in str(field.get("label") or "").lower()
                for marker in ("markdown", "description", "comment", "body")
            )
        ]
        if titles and bodies:
            return last, titles, bodies
        time.sleep(0.2)
    raise TimeoutError(
        "GitHub authoring controls did not become visible: "
        f"fields={last.get('field_count')} eligible={last.get('eligible_count')}"
    )


def find_account_button(actions: list[dict[str, Any]]) -> dict[str, Any] | None:
    preferred = (
        "open user navigation menu",
        "view profile and more",
        "account menu",
        "user navigation",
    )
    for marker in preferred:
        for action in actions:
            if action.get("role") == "button" and marker in str(action.get("label") or "").lower():
                return action
    for action in actions:
        label = str(action.get("label") or "").strip()
        if action.get("role") == "button" and label.startswith("@"):
            return action
    return None


def wait_for_account_menu(
    control: EngineControl, timeout: float
) -> tuple[dict[str, Any], set[str]]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("actions")
        labels = {str(item.get("label") or "").strip().lower() for item in last.get("actions", [])}
        matched = {
            marker
            for marker in ("your profile", "settings", "sign out")
            if any(marker in label for label in labels)
        }
        if len(matched) >= 2:
            return last, matched
        time.sleep(0.1)
    raise TimeoutError(
        f"GitHub account menu actions did not appear; action_count={len(last.get('actions', []))}"
    )


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "cefsimple"
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    args.profile.mkdir(parents=True, exist_ok=True, mode=0o700)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = args.output_dir.resolve()
    replay_path = output / "replay.jsonl"
    report_path = output / "report.json"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-github-canary-"))
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
        f"--url={args.url}",
        f"--user-data-dir={args.profile.resolve()}",
        "--no-first-run",
        "--no-default-browser-check",
        "--window-size=1440,1000",
    ]
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    started = time.monotonic()
    stage = "launch"
    report: dict[str, Any]
    with (output / "browser.log").open("wb") as browser_log:
        try:
            process = subprocess.Popen(
                command, cwd=ROOT, env=env, stdout=browser_log, stderr=subprocess.STDOUT
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, process, args.timeout_sec)
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            stage = "page_ready"
            truth = wait_for_collector(control, args.timeout_sec)
            if "github.com" not in str(truth.get("url") or ""):
                raise AssertionError(f"GitHub navigation failed: {truth.get('url')}")
            if "sign in" in str(truth.get("title") or "").lower():
                raise RuntimeError("GitHub profile is logged out; user login required")

            stage = "issue_editors"
            inventory, title_fields, body_fields = wait_for_issue_inventory(
                control, args.timeout_sec
            )
            if inventory.get("sensitive_values_exposed") is not False:
                raise AssertionError("GitHub form inventory exposed protected values")

            stage = "account_button"
            action_map = control.call("actions")
            account = find_account_button(action_map.get("actions", []))
            if account is None:
                raise AssertionError("visible GitHub account-menu button was not discovered")
            action_revision = int(account["basis_page_revision"])
            control.call(
                "act",
                {
                    "action_id": account["action_id"],
                    "basis_page_revision": action_revision,
                },
            )
            receipt = control.call("next_receipt", {"timeout_ms": 3000})
            if receipt.get("verified") is not True:
                raise AssertionError(f"account-menu click receipt failed: {receipt}")

            stage = "dynamic_menu"
            menu_actions, matched_menu_items = wait_for_account_menu(
                control, args.timeout_sec
            )
            replay_text = replay_path.read_text()
            replay_events = [json.loads(line) for line in replay_text.splitlines() if line]
            if not replay_events or not all(
                event.get("values_logged") is False for event in replay_events
            ):
                raise AssertionError("GitHub canary replay was not value-free")

            report = {
                "schema": "saccade-cef-github-canary-v1",
                "verdict": "PASS",
                "engine": "cef",
                "url": args.url,
                "source_title_available": bool(truth.get("title")),
                "authenticated_profile_reused": True,
                "issue_surface": {
                    "field_count": inventory.get("field_count"),
                    "eligible_count": inventory.get("eligible_count"),
                    "visible_title_fields": len(title_fields),
                    "visible_body_fields": len(body_fields),
                    "sensitive_values_exposed": False,
                },
                "account_menu": {
                    "button_fact_bound": True,
                    "click_receipt_verified": True,
                    "dynamic_items_observed": sorted(matched_menu_items),
                    "post_open_action_count": len(menu_actions.get("actions", [])),
                    "sign_out_clicked": False,
                },
                "submitted": False,
                "fields_written": 0,
                "screenshot_used": False,
                "values_logged": False,
                "replay_events": len(replay_events),
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-github-canary-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
                "submitted": False,
                "fields_written": 0,
                "values_logged": False,
                "duration_sec": round(time.monotonic() - started, 3),
            }
        finally:
            if control is not None:
                try:
                    control.call("close")
                except Exception:
                    pass
            if process is not None:
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_GITHUB_CANARY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
