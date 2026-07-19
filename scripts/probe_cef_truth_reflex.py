#!/usr/bin/env python3
"""Measure the CEF renderer truth -> native input -> receipt loop without CDP."""

from __future__ import annotations

import argparse
import json
import math
import os
import pathlib
import shutil
import socket
import statistics
import subprocess
import tempfile
import time
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_FIXTURE = ROOT / "test_pages" / "chrome_truth_reflex" / "index.html"
SENSITIVE_SENTINELS = ("123-45-6789", "correct-horse-battery")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure CEF renderer facts and native browser input without CDP."
    )
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--targets", type=int, default=100)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--receipt-p95-ms", type=float, default=20.0)
    parser.add_argument("--headed", action="store_true")
    return parser.parse_args()


def percentile(values: list[float], percentile_value: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, math.ceil(percentile_value * len(ordered)) - 1)
    return round(ordered[index], 3)


def metric(values: list[float]) -> dict[str, float | int | None]:
    return {
        "count": len(values),
        "p50_ms": round(statistics.median(values), 3) if values else None,
        "p95_ms": percentile(values, 0.95),
        "max_ms": round(max(values), 3) if values else None,
    }


class EngineControl:
    def __init__(self, socket_path: pathlib.Path, capability: str) -> None:
        self.socket_path = socket_path
        self.capability = capability
        self.request_id = 0
        self.public_transcript: list[dict[str, Any]] = []

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self.request_id += 1
        request = {
            "id": self.request_id,
            "method": method,
            "params": params or {},
            "capability": self.capability,
        }
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
            # Screenshot audit has a bounded 10 second server-side deadline.
            # Keep the client deadline longer so typed server errors are not
            # collapsed into an unhelpful socket timeout.
            stream.settimeout(12.0)
            stream.connect(str(self.socket_path))
            stream.sendall(json.dumps(request, separators=(",", ":")).encode() + b"\n")
            response_bytes = b""
            while b"\n" not in response_bytes:
                chunk = stream.recv(65536)
                if not chunk:
                    break
                response_bytes += chunk
        if not response_bytes:
            raise RuntimeError(f"CEF control closed during {method}")
        response = json.loads(response_bytes.split(b"\n", 1)[0])
        if not response.get("ok"):
            error = response.get("error") or {}
            raise RuntimeError(
                f"{method} failed: {error.get('code', 'INTERNAL')}: "
                f"{error.get('detail', 'unknown error')}"
            )
        result = response.get("result") or {}
        self.public_transcript.append({"method": method, "result": result})
        return result


def wait_for_grant(path: pathlib.Path, process: subprocess.Popen[bytes], timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"CEF exited before grant, status={process.returncode}")
        try:
            if path.stat().st_size > 0:
                grant = json.loads(path.read_text())
                if grant.get("url"):
                    return grant
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.02)
    raise TimeoutError("timed out waiting for CEF owner grant")


def wait_for_collector(control: EngineControl, timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("truth")
        if last.get("collector_ready"):
            return last
        time.sleep(0.02)
    raise TimeoutError(f"renderer collector did not become ready: {last}")


def main() -> int:
    args = parse_args()
    if args.targets <= 0:
        raise SystemExit("--targets must be positive")
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    fixture = args.fixture.resolve()
    if not fixture.is_file():
        raise SystemExit(f"missing fixture: {fixture}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    browser_log_path = args.output_dir / "browser.log"
    report_path = args.output_dir / "report.json"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-day3-"))
    os.chmod(session, 0o700)
    profile = session / "profile"
    profile.mkdir(mode=0o700)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    fixture_url = f"{fixture.as_uri()}?count={args.targets}"
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
        f"--url={fixture_url}",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--saccade-reflex-gate",
        f"--window-size={args.width},{args.height}",
    ]
    if not args.headed:
        command.extend(["--use-views", "--initial-show-state=hidden"])

    started = time.monotonic()
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
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
            adapter = grant.get("engine_adapter") or {}
            endpoint = grant.get("control_endpoint") or {}
            capability = grant.get("control_capability") or {}
            advertised = adapter.get("capabilities") or []
            required = {
                "truth",
                "actions",
                "next_fact",
                "act",
                "next_receipt",
                "reflex_start",
            }
            if not required.issubset(set(advertised)):
                raise RuntimeError(f"CEF adapter missing Day 3 capabilities: {advertised}")
            if endpoint.get("scheme") != "unix":
                raise RuntimeError("CEF Day 3 requires owner-only Unix transport")
            control = EngineControl(pathlib.Path(endpoint["path"]), capability["token"])
            ping = control.call("ping")
            initial_truth = wait_for_collector(control, args.timeout_sec)
            fields = initial_truth.get("fields") or []
            if len(fields) != 2 or not all(field.get("sensitive") for field in fields):
                raise RuntimeError(f"sensitive control inventory was not redacted: {fields}")
            if not all(field.get("complete") for field in fields):
                raise RuntimeError(f"sensitive completion truth was missing: {fields}")
            control.call("reflex_start")

            samples: list[dict[str, Any]] = []
            last_receipt: dict[str, Any] = {}
            for index in range(args.targets):
                fact = control.call("next_fact", {"timeout_ms": 5000})
                host_received_epoch_ms = time.time_ns() / 1_000_000
                host_received_monotonic_ns = time.monotonic_ns()
                if index == 0:
                    action_map = control.call("actions")
                    if not any(
                        action.get("action_id") == fact.get("action_id")
                        for action in action_map.get("actions", [])
                    ):
                        raise RuntimeError("first renderer fact was absent from action map")
                dispatch_started_ns = time.monotonic_ns()
                accepted = control.call(
                    "act",
                    {
                        "action_id": fact["action_id"],
                        "basis_page_revision": fact["page_revision"],
                    },
                )
                receipt = control.call("next_receipt", {"timeout_ms": 5000})
                if accepted.get("status") != "accepted" or not receipt.get("verified"):
                    raise RuntimeError(f"unverified input receipt: {accepted} {receipt}")
                if receipt.get("action_id") != fact.get("action_id"):
                    raise RuntimeError(
                        f"receipt mismatch: {fact.get('action_id')} != {receipt.get('action_id')}"
                    )
                renderer_fact_ms = float(fact["renderer_epoch_ms"])
                renderer_receipt_ms = float(receipt["renderer_epoch_ms"])
                samples.append(
                    {
                        "target": index + 1,
                        "action_id": fact["action_id"],
                        "renderer_fact_to_host_receive_ms": round(
                            host_received_epoch_ms - renderer_fact_ms, 3
                        ),
                        "host_receive_to_dispatch_start_ms": round(
                            (dispatch_started_ns - host_received_monotonic_ns) / 1_000_000,
                            3,
                        ),
                        "renderer_fact_to_input_receipt_ms": round(
                            renderer_receipt_ms - renderer_fact_ms, 3
                        ),
                        "hits": receipt.get("hits"),
                        "misses": receipt.get("misses"),
                    }
                )
                last_receipt = receipt

            full_loop = [sample["renderer_fact_to_input_receipt_ms"] for sample in samples]
            transport = [sample["renderer_fact_to_host_receive_ms"] for sample in samples]
            dispatch = [sample["host_receive_to_dispatch_start_ms"] for sample in samples]
            full_p95 = percentile(full_loop, 0.95)
            hits_ok = (
                last_receipt.get("hits") == args.targets
                and last_receipt.get("misses") == 0
                and bool(last_receipt.get("finished"))
            )
            latency_ok = full_p95 is not None and full_p95 <= args.receipt_p95_ms
            serialized_boundary = json.dumps(
                {
                    "ping": ping,
                    "truth": initial_truth,
                    "transcript": control.public_transcript,
                },
                sort_keys=True,
            )
            leak_matches = [
                sentinel for sentinel in SENSITIVE_SENTINELS if sentinel in serialized_boundary
            ]
            redaction_ok = not leak_matches and not initial_truth.get(
                "sensitive_values_exposed", True
            )
            ok = hits_ok and latency_ok and redaction_ok and len(samples) == args.targets
            report = {
                "schema": "saccade-cef-truth-reflex-v1",
                "verdict": "PASS" if ok else "FAIL",
                "engine": "cef",
                "contract_version": adapter.get("contract_version"),
                "fixture": str(fixture),
                "targets_requested": args.targets,
                "targets_receipted": len(samples),
                "hits": last_receipt.get("hits"),
                "misses": last_receipt.get("misses"),
                "finished": last_receipt.get("finished"),
                "route": {
                    "truth": "cef_renderer_pre_page_collector_to_process_message",
                    "host": "owner_only_unix_v1",
                    "input": "cef_browser_host_send_mouse_event",
                    "receipt": "renderer_capture_listener_v1",
                    "cdp_used": False,
                    "screenshots_used": False,
                    "page_dom_mutated": False,
                },
                "redaction": {
                    "sensitive_controls": len(fields),
                    "completion_only": True,
                    "sentinel_matches": leak_matches,
                    "pass": redaction_ok,
                },
                "latency": {
                    "renderer_fact_to_host_receive": metric(transport),
                    "host_receive_to_dispatch_start": metric(dispatch),
                    "renderer_fact_to_input_receipt": metric(full_loop),
                    "receipt_p95_ms_max": args.receipt_p95_ms,
                    "receipt_p95_pass": latency_ok,
                },
                "elapsed_ms": round((time.monotonic() - started) * 1000, 3),
                "samples": samples,
            }
            report_text = json.dumps(report, indent=2, sort_keys=True) + "\n"
            if any(sentinel in report_text for sentinel in SENSITIVE_SENTINELS):
                raise RuntimeError("sensitive sentinel reached the report")
            report_path.write_text(report_text)
            if not ok:
                raise RuntimeError(f"CEF Day 3 gate failed; see {report_path}")
        except Exception as error:
            if "report" in locals() and report.get("latency"):
                report["error"] = str(error)
            else:
                report = {
                    "schema": "saccade-cef-truth-reflex-v1",
                    "verdict": "FAIL",
                    "error": str(error),
                    "public_transcript": control.public_transcript if control else [],
                    "elapsed_ms": round((time.monotonic() - started) * 1000, 3),
                }
            report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
            raise
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
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=3)
            shutil.rmtree(session, ignore_errors=True)

    print(
        "CEF_TRUTH_REFLEX "
        f"verdict={report['verdict']} targets={report.get('targets_receipted', 0)} "
        f"hits={report.get('hits', 0)} misses={report.get('misses', 0)} "
        f"p95_ms={report.get('latency', {}).get('renderer_fact_to_input_receipt', {}).get('p95_ms')} "
        f"report={report_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
