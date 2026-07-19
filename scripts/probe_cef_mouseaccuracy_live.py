#!/usr/bin/env python3
"""Verify the CEF truth-to-native-input loop on MouseAccuracy."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import statistics
import subprocess
import tempfile
import time
from typing import Any

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Operate MouseAccuracy through the CEF owner bridge."
    )
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--hits", type=int, default=12)
    parser.add_argument("--timeout-sec", type=float, default=30.0)
    parser.add_argument("--hidden", action="store_true")
    return parser.parse_args()


def percentile_95(values: list[float]) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    return round(ordered[max(0, int(len(ordered) * 0.95 + 0.999) - 1)], 3)


def wait_for_start_action(
    control: EngineControl, timeout: float
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        action_map = control.call("actions")
        for action in action_map.get("actions", []):
            if action.get("label", "").strip().upper() == "START":
                return action
        time.sleep(0.05)
    raise TimeoutError("START action did not appear before timeout")


def main() -> int:
    args = parse_args()
    if args.hits <= 0:
        raise SystemExit("--hits must be positive")
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-mouseaccuracy-"))
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
            "SACCADE_REFLEX_GATE": "1",
        }
    )
    command = [
        str(executable),
        "--url=https://mouseaccuracy.com/",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--saccade-reflex-gate",
        "--window-size=1440,1000",
    ]
    if args.hidden:
        command.extend(["--use-views", "--initial-show-state=hidden"])

    browser_log_path = args.output_dir / "browser.log"
    report_path = args.output_dir / "report.json"
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    started = time.monotonic()
    report: dict[str, Any]
    with browser_log_path.open("wb") as browser_log:
        try:
            process = subprocess.Popen(
                command,
                cwd=ROOT,
                env=env,
                stdout=browser_log,
                stderr=subprocess.STDOUT,
            )
            grant = wait_for_grant(grant_path, process, args.timeout_sec)
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            truth = wait_for_collector(control, args.timeout_sec)
            start_action = wait_for_start_action(control, args.timeout_sec)
            accepted = control.call(
                "act",
                {
                    "action_id": start_action["action_id"],
                    "basis_page_revision": start_action["basis_page_revision"],
                },
            )
            start_receipt = control.call("next_receipt", {"timeout_ms": 3000})
            if accepted.get("status") != "accepted" or not start_receipt.get("verified"):
                raise RuntimeError("START did not produce a verified renderer receipt")

            latencies: list[float] = []
            receipts: list[dict[str, Any]] = []
            deadline = time.monotonic() + args.timeout_sec
            while len(receipts) < args.hits and time.monotonic() < deadline:
                try:
                    fact = control.call("next_fact", {"timeout_ms": 1000})
                except RuntimeError as error:
                    if "TIMEOUT" in str(error):
                        continue
                    raise
                if fact.get("role") != "target":
                    continue
                control.call(
                    "act",
                    {
                        "action_id": fact["action_id"],
                        "basis_page_revision": fact["page_revision"],
                    },
                )
                receipt = control.call("next_receipt", {"timeout_ms": 3000})
                if not receipt.get("verified") or receipt.get("action_id") != fact.get(
                    "action_id"
                ):
                    raise RuntimeError("target did not produce a matching renderer receipt")
                latencies.append(
                    float(receipt["renderer_epoch_ms"])
                    - float(fact["renderer_epoch_ms"])
                )
                receipts.append(receipt)

            status = control.call("shell_status")
            passed = (
                len(receipts) == args.hits
                and status.get("collector_ready") is True
                and status.get("url") == "https://mouseaccuracy.com/game"
            )
            report = {
                "schema": "saccade-cef-mouseaccuracy-live-v1",
                "verdict": "PASS" if passed else "FAIL",
                "start_receipt_verified": bool(start_receipt.get("verified")),
                "targets_receipted": len(receipts),
                "requested_hits": args.hits,
                "latency_ms": {
                    "median": round(statistics.median(latencies), 3)
                    if latencies
                    else None,
                    "p95": percentile_95(latencies),
                    "max": round(max(latencies), 3) if latencies else None,
                },
                "final_status": status,
                "collector_error": truth.get("collector_error", ""),
                "duration_sec": round(time.monotonic() - started, 3),
                "route": {"cdp_used": False, "screenshot_used": False},
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-mouseaccuracy-live-v1",
                "verdict": "FAIL",
                "error": str(error),
                "duration_sec": round(time.monotonic() - started, 3),
                "route": {"cdp_used": False, "screenshot_used": False},
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
        "CEF_MOUSEACCURACY_LIVE "
        f"verdict={report['verdict']} hits={report.get('targets_receipted', 0)} "
        f"p95_ms={report.get('latency_ms', {}).get('p95')} report={report_path}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
