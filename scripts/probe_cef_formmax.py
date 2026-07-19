#!/usr/bin/env python3
"""Fill the 96-row, two-page FORMMAX fixture through the CEF bridge."""

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
FIXTURE = ROOT / "test_pages" / "formmax" / "index.html"
OWNERS = ["Ari", "Mina", "Ravi", "Sol", "Theo", "Uma"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    return parser.parse_args()


def assignment_for(field_id: str) -> Any:
    name = field_id.removeprefix("name:")
    row_text, field = name.split("_", 1)
    row = int(row_text.removeprefix("CAP-"))
    values: dict[str, Any] = {
        "site_name": f"Region {(row + 7) // 8} / Site {row:03d}",
        "rack_count": 8 + (row % 12),
        "power_mw": round(1.2 + (row % 9) * 0.35, 2),
        "cooling_tons": 40 + (row % 15) * 3,
        "owner": OWNERS[row % len(OWNERS)],
        "target_date": f"2026-{1 + (row % 12):02d}-{1 + (row % 25):02d}",
        "approved": row % 3 != 0,
    }
    return values[field]


def row_number(field_id: str) -> int | None:
    if not field_id.startswith("name:CAP-"):
        return None
    return int(field_id[9:12])


def wait_for_page_rows(
    control: EngineControl, first_row: int, timeout: float
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("form_inventory")
        rows = {
            row_number(field["field_id"])
            for field in last.get("fields", [])
            if row_number(field["field_id"]) is not None
        }
        if first_row in rows:
            return last
        time.sleep(0.02)
    raise TimeoutError(f"FORMMAX page starting at {first_row} did not appear: {last}")


def fill_page(
    control: EngineControl,
    inventory: dict[str, Any],
    last_row: int,
) -> tuple[int, int, dict[str, Any]]:
    filled_ids: set[str] = set()
    batches = 0
    while True:
        revision = int(inventory["page_revision"])
        assignments = {
            field["field_id"]: assignment_for(field["field_id"])
            for field in inventory["fields"]
            if field.get("eligible")
            and row_number(field["field_id"]) is not None
            and field["field_id"] not in filled_ids
        }
        if assignments:
            compiled = control.call(
                "form_compile_plan",
                {"basis_page_revision": revision, "assignments": assignments},
            )
            executed = control.call(
                "form_execute_plan",
                {
                    "basis_page_revision": revision,
                    "expected_plan_id": compiled["plan_id"],
                    "assignments": assignments,
                },
            )
            if executed.get("receipt_verified") is not True:
                raise AssertionError(f"FORMMAX batch was not verified: {executed}")
            if len(executed.get("filled", [])) != len(assignments):
                raise AssertionError("FORMMAX batch did not fill every eligible field")
            filled_ids.update(item["field_id"] for item in executed["filled"])
            batches += 1
            revision = int(executed["page_revision"])
            inventory = control.call("form_inventory")

        visible_rows = [
            row_number(field["field_id"])
            for field in inventory["fields"]
            if row_number(field["field_id"]) is not None
        ]
        if visible_rows and max(visible_rows) >= last_row:
            return len(filled_ids), batches, inventory
        revealed = control.call(
            "form_reveal_more", {"basis_page_revision": revision}
        )
        if revealed.get("changed_scrollers", 0) < 1:
            raise AssertionError(f"FORMMAX lazy table did not reveal more rows: {revealed}")
        inventory = control.call("form_inventory")


def click_submit(control: EngineControl, revision: int, expected_label: str) -> None:
    action = next(
        (
            candidate
            for candidate in control.call("actions").get("actions", [])
            if candidate.get("label") == expected_label
        ),
        None,
    )
    if not action:
        raise AssertionError(f"missing visible action {expected_label}")
    control.call(
        "act",
        {
            "action_id": action["action_id"],
            "basis_page_revision": revision,
        },
    )
    receipt = control.call("next_receipt", {"timeout_ms": 3000})
    if receipt.get("verified") is not True:
        raise AssertionError(f"unverified {expected_label} receipt: {receipt}")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report_path = args.output_dir / "report.json"
    replay_path = args.output_dir / "replay.jsonl"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-formmax-"))
    os.chmod(session, 0o700)
    profile = session / "profile"
    profile.mkdir(mode=0o700)
    grant_path = session / "grant.json"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_REFLEX_GATE": "1",
        }
    )
    command = [
        str(executable),
        f"--url={FIXTURE.resolve().as_uri()}",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--use-views",
        "--initial-show-state=hidden",
        "--window-size=1440,1000",
    ]
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    started = time.monotonic()
    report: dict[str, Any]
    with (args.output_dir / "browser.log").open("wb") as browser_log:
        try:
            process = subprocess.Popen(
                command, cwd=ROOT, env=env, stdout=browser_log, stderr=subprocess.STDOUT
            )
            grant = wait_for_grant(grant_path, process, args.timeout_sec)
            if "form_reveal_more" not in grant["engine_adapter"]["capabilities"]:
                raise AssertionError("CEF adapter did not advertise lazy-form reveal")
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            wait_for_collector(control, args.timeout_sec)
            page_one = wait_for_page_rows(control, 1, args.timeout_sec)
            page_one_filled, page_one_batches, page_one = fill_page(
                control, page_one, 48
            )
            click_submit(control, int(page_one["page_revision"]), "Submit page")
            page_two = wait_for_page_rows(control, 49, args.timeout_sec)
            page_two_filled, page_two_batches, page_two = fill_page(
                control, page_two, 96
            )
            if page_two.get("sensitive_count") != 3:
                raise AssertionError("FORMMAX sensitive final-page fields were not classified")
            click_submit(
                control, int(page_two["page_revision"]), "Submit final page"
            )
            replay_text = replay_path.read_text()
            if any(value in replay_text for value in ("Region 1", "2026-", "Tax ID")):
                raise AssertionError("FORMMAX replay contained field values or labels")
            events = [json.loads(line) for line in replay_text.splitlines() if line]
            if not events or not all(event.get("values_logged") is False for event in events):
                raise AssertionError("FORMMAX replay was not value-free")
            total_filled = page_one_filled + page_two_filled
            report = {
                "schema": "saccade-cef-formmax-v1",
                "verdict": "PASS" if total_filled == 96 * 7 else "FAIL",
                "rows": 96,
                "pages": 2,
                "fields_filled_verified": total_filled,
                "batches": page_one_batches + page_two_batches,
                "sensitive_fields_blocked": 3,
                "submit_receipts_verified": 2,
                "replay": {
                    "path": str(replay_path),
                    "event_count": len(events),
                    "values_logged": False,
                    "sentinel_scan": "PASS",
                },
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-formmax-v1",
                "verdict": "FAIL",
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
    print(f"CEF_FORMMAX verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
