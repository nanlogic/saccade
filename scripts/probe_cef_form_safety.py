#!/usr/bin/env python3
"""Verify CEF form, collaboration, screenshot-policy, and replay boundaries."""

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
DEFAULT_FIXTURE = ROOT / "test_pages" / "form_plan" / "index.html"
SCREENSHOT_FIXTURE = ROOT / "test_pages" / "visual_parity" / "dashboard" / "index.html"
SENSITIVE_SENTINELS = (
    "fixture-secret",
    "123-45-6789",
    "correct-horse-battery",
    "4111111111111111",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def field_by_id(fields: list[dict[str, Any]], field_id: str) -> dict[str, Any]:
    for field in fields:
        if field.get("field_id") == field_id:
            return field
    raise AssertionError(f"missing field {field_id}")


def assert_no_sensitive_values(value: Any, location: str) -> None:
    encoded = json.dumps(value, sort_keys=True)
    leaked = [sentinel for sentinel in SENSITIVE_SENTINELS if sentinel in encoded]
    if leaked:
        raise AssertionError(f"sensitive value leaked through {location}: {leaked}")


def wait_for_url(
    control: EngineControl, expected_url: str, timeout: float
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("truth")
        if last.get("url") == expected_url and last.get("collector_ready") is True:
            return last
        time.sleep(0.02)
    raise TimeoutError(f"CEF did not settle on {expected_url}: {last}")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    if not fixture.is_file():
        raise SystemExit(f"missing form fixture: {fixture}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    browser_log_path = args.output_dir / "browser.log"
    replay_path = args.output_dir / "replay.jsonl"
    report_path = args.output_dir / "report.json"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-day4-"))
    os.chmod(session, 0o700)
    profile = session / "profile"
    profile.mkdir(mode=0o700)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(socket_path),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_REFLEX_GATE": "1",
        }
    )
    command = [
        str(executable),
        f"--url={fixture.as_uri()}",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    started = time.monotonic()
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    report: dict[str, Any]
    stage = "launch"
    with browser_log_path.open("wb") as browser_log:
        try:
            process = subprocess.Popen(
                command,
                cwd=ROOT,
                env=env,
                stdout=browser_log,
                stderr=subprocess.STDOUT,
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, process, args.timeout_sec)
            advertised = set(grant["engine_adapter"]["capabilities"])
            required = {
                "form_inventory",
                "inspect_fields",
                "form_compile_plan",
                "form_execute_plan",
                "type_field_text",
                "screenshot_policy",
                "screenshot_audit",
                "article_text",
            }
            if not required.issubset(advertised):
                raise AssertionError(f"missing CEF Day 4 capabilities: {advertised}")
            if grant.get("artifacts", {}).get("values_logged") is not False:
                raise AssertionError("grant did not declare value-free replay")
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            stage = "collector"
            truth = wait_for_collector(control, args.timeout_sec)
            initial_revision = int(truth["page_revision"])

            stage = "form_inventory"
            inventory = control.call("form_inventory")
            fields = inventory["fields"]
            if inventory["sensitive_count"] != 2:
                raise AssertionError(f"unexpected sensitive inventory: {inventory}")
            password = field_by_id(fields, "id:password")
            ssn = field_by_id(fields, "id:ssn")
            if password.get("value_state") != "completed_without_value":
                raise AssertionError(f"password completion was not redacted: {password}")
            if ssn.get("eligible") is not False:
                raise AssertionError(f"SSN was eligible for agent fill: {ssn}")
            assert_no_sensitive_values(inventory, "inventory")

            stage = "inspect_fields"
            inspected = control.call(
                "inspect_fields", {"basis_page_revision": initial_revision}
            )
            inspected_fields = inspected["fields"]
            user_note = field_by_id(inspected_fields, "id:user-note")
            inspected_password = field_by_id(inspected_fields, "id:password")
            if any(field.get("field_id") == "id:hidden-token" for field in fields):
                raise AssertionError("hidden control was exposed in form inventory")
            if user_note.get("value") != "Keep this exact note.":
                raise AssertionError("non-sensitive human value was not inspectable")
            if inspected_password.get("value_redacted") is not True or "value" in inspected_password:
                raise AssertionError("sensitive password crossed the collaboration boundary")
            if any(field.get("field_id") == "id:hidden-token" for field in inspected_fields):
                raise AssertionError("hidden control was exposed by default inspection")
            assert_no_sensitive_values(inspected, "inspect_fields")

            stage = "sensitive_article_text"
            article = control.call(
                "article_text", {"basis_page_revision": initial_revision}
            )
            if article.get("sensitive_values_exposed") is not False:
                raise AssertionError("article reader did not declare redaction")
            if article.get("article_text_length", 0) <= 0:
                raise AssertionError("article reader returned no visible text")
            assert_no_sensitive_values(article, "article_text")

            assignments: dict[str, Any] = {
                "id:team": "Platform Reliability",
                "id:region": "west",
                "id:instances": 24,
                "id:launch-date": "2026-07-20",
                "id:user-note": "OVERWRITE-USER-NOTE",
                "id:project-code": "OVERWRITE-EXISTING",
                "id:ssn": "123-45-6789",
                "id:password": "correct-horse-battery",
                "id:include-staging": True,
                "id:summary": "Capacity for a blue-green launch rehearsal.",
                "id:cm-editor": "Structured editor draft.",
            }
            stage = "compile_plan"
            compiled = control.call(
                "form_compile_plan",
                {
                    "basis_page_revision": initial_revision,
                    "assignments": assignments,
                },
            )
            eligible_ids = {item["field_id"] for item in compiled["eligible"]}
            rejected_ids = {item["field_id"] for item in compiled["rejected"]}
            expected_eligible = {
                "id:team",
                "id:region",
                "id:instances",
                "id:launch-date",
                "id:include-staging",
                "id:summary",
            }
            if eligible_ids != expected_eligible:
                raise AssertionError(f"unexpected eligible plan: {compiled}")
            if not {
                "id:user-note",
                "id:project-code",
                "id:ssn",
                "id:password",
                "id:cm-editor",
            }.issubset(rejected_ids):
                raise AssertionError(f"unsafe fields were not rejected: {compiled}")
            assert_no_sensitive_values(compiled, "compiled plan")

            stage = "execute_plan"
            executed = control.call(
                "form_execute_plan",
                {
                    "basis_page_revision": initial_revision,
                    "expected_plan_id": compiled["plan_id"],
                    "assignments": assignments,
                },
            )
            if executed.get("receipt_verified") is not True:
                raise AssertionError(f"form receipt was not verified: {executed}")
            if len(executed.get("filled", [])) != len(expected_eligible):
                raise AssertionError(f"ordinary fields were not fully filled: {executed}")
            if executed.get("page_revision", initial_revision) <= initial_revision:
                raise AssertionError("form execution did not advance page revision")
            assert_no_sensitive_values(executed, "execution receipt")

            final_revision = int(executed["page_revision"])
            refreshed_truth = wait_for_collector(control, args.timeout_sec)
            if int(refreshed_truth["page_revision"]) != final_revision:
                raise AssertionError("post-fill collector advanced unexpectedly")
            stage = "native_rich_editor_type"
            native_typed = control.call(
                "type_field_text",
                {
                    "basis_page_revision": final_revision,
                    "field_id": "id:cm-editor",
                    "text": "Structured editor draft.",
                },
            )
            if native_typed.get("receipt_verified") is not True:
                raise AssertionError(f"native rich-editor typing failed: {native_typed}")
            if native_typed.get("method") != "cef_devtools_input_insert_text":
                raise AssertionError(f"unexpected rich-editor motor route: {native_typed}")
            stage = "post_fill_inspection"
            final_inspection = control.call(
                "inspect_fields",
                {
                    "basis_page_revision": final_revision,
                    "field_ids": [
                        "id:team",
                        "id:region",
                        "id:user-note",
                        "id:project-code",
                        "id:ssn",
                        "id:password",
                        "id:cm-editor",
                    ],
                },
            )
            if field_by_id(final_inspection["fields"], "id:team").get("value") != "Platform Reliability":
                raise AssertionError("agent-filled ordinary value was not visible in same tab")
            if field_by_id(final_inspection["fields"], "id:user-note").get("value") != "Keep this exact note.":
                raise AssertionError("human non-sensitive value was overwritten")
            if field_by_id(final_inspection["fields"], "id:project-code").get("value") != "USER-42":
                raise AssertionError("existing user value was overwritten")
            if field_by_id(final_inspection["fields"], "id:cm-editor").get("value") != "Structured editor draft.":
                raise AssertionError("native rich-editor text was not visible")
            for field_id in ("id:ssn", "id:password"):
                field = field_by_id(final_inspection["fields"], field_id)
                if field.get("value_redacted") is not True or "value" in field:
                    raise AssertionError(f"sensitive value escaped after fill: {field}")
            assert_no_sensitive_values(final_inspection, "post-fill inspection")

            stage = "sensitive_screenshot_policy"
            screenshot_policy = control.call(
                "screenshot_policy",
                {
                    "basis_page_revision": final_revision,
                    "audit_requested": True,
                },
            )
            if screenshot_policy.get("capture_allowed") is not False or screenshot_policy.get("reason") != "sensitive_fields_present":
                raise AssertionError(f"sensitive screenshot was not blocked: {screenshot_policy}")

            screenshot_url = SCREENSHOT_FIXTURE.resolve().as_uri()
            stage = "screenshot_fixture_navigation"
            control.call("navigate", {"url": screenshot_url})
            screenshot_truth = wait_for_url(
                control, screenshot_url, args.timeout_sec
            )
            stage = "non_sensitive_screenshot_audit"
            screenshot_result = control.call(
                "screenshot_audit",
                {
                    "basis_page_revision": int(screenshot_truth["page_revision"]),
                    "audit_requested": True,
                },
            )
            screenshot_path = pathlib.Path(screenshot_result["screenshot_path"])
            if screenshot_result.get("capture_allowed") is not True:
                raise AssertionError(f"non-sensitive screenshot was blocked: {screenshot_result}")
            if screenshot_result.get("truth_route_used") is not False:
                raise AssertionError("optional screenshot was used as the truth route")
            if not screenshot_path.is_file() or screenshot_path.read_bytes()[:8] != b"\x89PNG\r\n\x1a\n":
                raise AssertionError("CEF screenshot artifact was not a valid PNG")

            transcript = control.public_transcript
            assert_no_sensitive_values(transcript, "public control transcript")
            time.sleep(0.1)
            replay_text = replay_path.read_text()
            for sentinel in SENSITIVE_SENTINELS:
                if sentinel in replay_text:
                    raise AssertionError(f"sensitive sentinel leaked to replay: {sentinel}")
            replay_events = [json.loads(line) for line in replay_text.splitlines() if line]
            if not replay_events or not all(
                event.get("values_logged") is False for event in replay_events
            ):
                raise AssertionError("CEF replay did not remain value-free")
            if not any(event.get("event") == "form_screenshot_policy" for event in replay_events):
                raise AssertionError("screenshot skip was not recorded in replay")

            report = {
                "schema": "saccade-cef-day4-form-safety-v1",
                "verdict": "PASS",
                "engine": "cef",
                "capabilities": sorted(required),
                "form": {
                    "field_count": inventory["field_count"],
                    "sensitive_count": inventory["sensitive_count"],
                    "filled_verified": len(executed["filled"]) + 1,
                    "native_rich_editor_verified": True,
                    "unsafe_rejected": len(compiled["rejected"]),
                    "existing_values_preserved": len(executed["preserved"]),
                },
                "collaboration": {
                    "human_non_sensitive_visible": True,
                    "human_values_preserved": True,
                    "sensitive_values_redacted": True,
                },
                "screenshot": {
                    "sensitive_page": screenshot_policy,
                    "non_sensitive_page": screenshot_result,
                },
                "replay": {
                    "path": str(replay_path),
                    "event_count": len(replay_events),
                    "values_logged": False,
                    "sentinel_scan": "PASS",
                },
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-day4-form-safety-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
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
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        "CEF_DAY4_FORM_SAFETY "
        f"verdict={report['verdict']} report={report_path}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
