#!/usr/bin/env python3
"""Run the deterministic CEF structural human/agent agreement gate."""

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
DEFAULT_FIXTURE = ROOT / "test_pages" / "human_agent_agreement" / "index.html"
SENSITIVE_SENTINEL = "agreement-fixture-secret"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def wait_for_url(
    control: EngineControl, expected_url: str, timeout: float
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        try:
            last = control.call("truth")
        except RuntimeError as error:
            if "renderer collector is not ready" not in str(error):
                raise
            time.sleep(0.05)
            continue
        if last.get("url") == expected_url and last.get("collector_ready") is True:
            return last
        time.sleep(0.05)
    raise TimeoutError(f"CEF did not settle on {expected_url}: {last}")


def assert_common(result: dict[str, Any]) -> None:
    agreement = result.get("agreement", {})
    observations = result.get("observations", {})
    if result.get("engine") != "saccade-cef-render-preflight-v1":
        raise AssertionError(f"unexpected preflight engine: {result}")
    if agreement.get("scope") != "structural_preflight":
        raise AssertionError(f"unexpected agreement scope: {agreement}")
    if agreement.get("full_agreement_measured") is not False:
        raise AssertionError("structural preflight overclaimed full agreement")
    if observations.get("observation_base_consistent") is not True:
        raise AssertionError(f"preflight mixed page revisions: {observations}")
    if agreement.get("visual_evidence", {}).get("status") != "not_captured":
        raise AssertionError("preflight captured visual evidence by default")
    if result.get("sensitive_values_exposed") is not False:
        raise AssertionError("preflight did not declare value redaction")
    if SENSITIVE_SENTINEL in json.dumps(result, sort_keys=True):
        raise AssertionError("protected fixture value crossed the CEF boundary")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    if not fixture.is_file():
        raise SystemExit(f"missing agreement fixture: {fixture}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    output = args.output_dir.resolve()
    report_path = output / "report.json"
    replay_path = output / "replay.jsonl"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-ai034-"))
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
        }
    )
    green_url = fixture.as_uri() + "?mode=green"
    occluded_url = fixture.as_uri() + "?mode=occluded"
    command = [
        str(executable),
        f"--url={green_url}",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    started = time.monotonic()
    stage = "launch"
    report: dict[str, Any]
    with (output / "browser.log").open("wb") as browser_log:
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
            capabilities = set(grant["engine_adapter"]["capabilities"])
            if "render_preflight" not in capabilities:
                raise AssertionError("CEF grant did not advertise render_preflight")
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]),
                grant["control_capability"]["token"],
            )
            stage = "green_ready"
            wait_for_collector(control, args.timeout_sec)
            wait_for_url(control, green_url, args.timeout_sec)

            stage = "green_preflight"
            green = control.call("render_preflight", {"expected_surface": "page"})
            assert_common(green)
            green_hit = green.get("observations", {}).get("renderer_hit_test", {})
            if green.get("verdict") != "green" or green.get("agent_input_allowed") is not True:
                raise AssertionError(f"actionable fixture was not green: {green}")
            if green_hit.get("tested", 0) < 2 or green_hit.get("failed") != 0:
                raise AssertionError(f"green fixture hit agreement failed: {green_hit}")

            stage = "task_surface_mismatch"
            mismatch = control.call(
                "render_preflight", {"expected_surface": "github_issue"}
            )
            assert_common(mismatch)
            if (
                mismatch.get("verdict") != "red"
                or mismatch.get("recommended_route") != "navigate_task_surface"
                or mismatch.get("task_surface_match") is not False
            ):
                raise AssertionError(f"task-surface mismatch was not routed: {mismatch}")

            stage = "occluded_navigation"
            control.call("navigate", {"url": occluded_url})
            wait_for_url(control, occluded_url, args.timeout_sec)

            stage = "occluded_preflight"
            occluded = control.call(
                "render_preflight", {"expected_surface": "page"}
            )
            assert_common(occluded)
            occluded_hit = occluded.get("observations", {}).get(
                "renderer_hit_test", {}
            )
            if (
                occluded.get("verdict") != "red"
                or occluded.get("recommended_route") != "block"
                or occluded_hit.get("failed", 0) < 1
            ):
                raise AssertionError(f"occluded fixture was not blocked: {occluded}")

            replay_events = [
                json.loads(line)
                for line in replay_path.read_text(encoding="utf-8").splitlines()
                if line
            ]
            if len(replay_events) < 3 or not all(
                event.get("values_logged") is False for event in replay_events
            ):
                raise AssertionError("agreement replay was missing or not value-free")
            if SENSITIVE_SENTINEL in replay_path.read_text(encoding="utf-8"):
                raise AssertionError("protected fixture value leaked into replay")

            report = {
                "schema": "saccade-cef-human-agent-agreement-v1",
                "verdict": "PASS",
                "engine": "cef",
                "green": green,
                "task_surface_mismatch": mismatch,
                "occluded": occluded,
                "replay_events": len(replay_events),
                "screenshots_captured": False,
                "values_logged": False,
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-cef-human-agent-agreement-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
                "screenshots_captured": False,
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
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
            shutil.rmtree(session, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        f"CEF_HUMAN_AGENT_AGREEMENT verdict={report['verdict']} "
        f"report={report_path}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
