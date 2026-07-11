#!/usr/bin/env python3
"""Verify that a live bridge accepts only its grant-bound session capability."""

from __future__ import annotations

import argparse
import json
import queue
import socket
import stat
import subprocess
import threading
from pathlib import Path
from typing import Any

from probe_generic_form_plan import (
    DEFAULT_BIN,
    DEFAULT_SERVOSHELL,
    ROOT,
    read_lines,
    wait_ready,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "runs/safety/control_capability",
    )
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    return parser.parse_args()


def call(endpoint: str, capability: str | None, method: str) -> dict[str, Any]:
    host, port = endpoint.rsplit(":", 1)
    request: dict[str, Any] = {"id": 1, "method": method, "params": {}}
    if capability is not None:
        request["capability"] = capability
    with socket.create_connection((host, int(port)), timeout=5) as stream:
        stream.settimeout(5)
        stream.sendall((json.dumps(request) + "\n").encode("utf-8"))
        response = stream.makefile("r", encoding="utf-8").readline()
    return json.loads(response)


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    grant = output_dir / "grant.json"
    url = (ROOT / "test_pages/browser_session/index.html").resolve().as_uri()
    command = [
        str(args.bin.resolve()), "bridge",
        "--servoshell", str(args.servoshell.resolve()),
        "--url", url,
        "--output-dir", str(output_dir / "bridge"),
        "--grant-path", str(grant),
        "--timeout-sec", str(args.timeout_sec),
    ]
    proc = subprocess.Popen(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, bufsize=1,
    )
    assert proc.stdout is not None
    lines: "queue.Queue[str]" = queue.Queue()
    threading.Thread(target=read_lines, args=(proc.stdout, lines), daemon=True).start()
    endpoint = ""
    failures: list[str] = []
    try:
        endpoint, startup = wait_ready(proc, lines, args.timeout_sec)
        grant_payload = json.loads(grant.read_text(encoding="utf-8"))
        capability = grant_payload.get("control_capability", {}).get("token")
        if not isinstance(capability, str) or len(capability) < 32:
            failures.append("grant did not contain a usable session capability")
            capability = ""
        no_capability = call(endpoint, None, "truth")
        wrong_capability = call(endpoint, "wrong-capability", "truth")
        correct_capability = call(endpoint, capability, "truth")
        if no_capability.get("ok") is not False or "capability" not in str(no_capability.get("error")):
            failures.append(f"missing capability was not rejected: {no_capability}")
        if wrong_capability.get("ok") is not False or "capability" not in str(wrong_capability.get("error")):
            failures.append(f"wrong capability was not rejected: {wrong_capability}")
        if correct_capability.get("ok") is not True:
            failures.append(f"correct capability did not receive truth: {correct_capability}")
        if grant_payload.get("control_endpoint", {}).get("protocol") != "saccade-dogfood-control-v1":
            failures.append("grant did not advertise control protocol v1")
        if stat.S_IMODE(grant.stat().st_mode) != 0o600:
            failures.append("grant file permissions are not owner-only")

        shutdown = call(endpoint, capability, "shutdown")
        if shutdown.get("ok") is not True:
            failures.append(f"capability-authenticated shutdown failed: {shutdown}")
    finally:
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()

    control_dir = output_dir / "bridge" / "control"
    report_text = (control_dir / "report.json").read_text(errors="replace") if (control_dir / "report.json").exists() else ""
    replay_text = (control_dir / "replay.jsonl").read_text(errors="replace") if (control_dir / "replay.jsonl").exists() else ""
    if capability and (capability in report_text or capability in replay_text):
        failures.append("control capability leaked into report or replay")
    report = {
        "ok": not failures,
        "engine": "saccade-control-capability-probe-v1",
        "protocol": grant_payload.get("control_endpoint", {}).get("protocol"),
        "missing_capability_rejected": no_capability.get("ok") is False,
        "wrong_capability_rejected": wrong_capability.get("ok") is False,
        "correct_capability_accepted": correct_capability.get("ok") is True,
        "grant_mode": oct(stat.S_IMODE(grant.stat().st_mode)),
        "capability_in_report_or_replay": bool(capability and (capability in report_text or capability in replay_text)),
        "values_logged": False,
        "failures": failures,
    }
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"CONTROL CAPABILITY {'PASS' if report['ok'] else 'FAIL'} "
        f"protocol={report['protocol']} report={report_path}"
    )
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
