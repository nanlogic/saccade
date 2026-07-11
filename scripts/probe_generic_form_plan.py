#!/usr/bin/env python3
"""Exercise generic form inventory and plan compilation through ServoShell."""

from __future__ import annotations

import argparse
import json
import queue
import re
import socket
import subprocess
import threading
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = ROOT / "target/debug/saccade-servoshell"
DEFAULT_SERVOSHELL = Path("/Applications/Servo.app/Contents/MacOS/servoshell")
SENTINELS = ["FORMPLAN_TEAM_SECRET", "FORMPLAN_SSN_SECRET", "FORMPLAN_NOTE_SECRET"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "runs/formmax/generic_plan")
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    return parser.parse_args()


def read_lines(stream: Any, output: "queue.Queue[str]") -> None:
    for line in iter(stream.readline, ""):
        output.put(line.rstrip("\n"))


def wait_ready(proc: subprocess.Popen[str], lines: "queue.Queue[str]", timeout: float) -> tuple[str, list[str]]:
    deadline = time.monotonic() + timeout
    seen: list[str] = []
    pattern = re.compile(r"SACCADE_SERVOSHELL_BRIDGE READY endpoint=(\S+)")
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"bridge exited before ready: {proc.returncode}; output={seen[-20:]}")
        try:
            line = lines.get(timeout=0.2)
        except queue.Empty:
            continue
        seen.append(line)
        match = pattern.search(line)
        if match:
            return match.group(1), seen
    raise TimeoutError(f"bridge not ready after {timeout}s; output={seen[-20:]}")


def call(endpoint: str, method: str, params: dict[str, Any], timeout: float, expect_ok: bool = True) -> dict[str, Any]:
    host, port = endpoint.rsplit(":", 1)
    request = json.dumps({"id": 1, "method": method, "params": params}) + "\n"
    with socket.create_connection((host, int(port)), timeout=timeout) as sock:
        sock.settimeout(timeout)
        sock.sendall(request.encode())
        chunks: list[bytes] = []
        while True:
            chunk = sock.recv(65536)
            if not chunk:
                break
            chunks.append(chunk)
            if b"\n" in chunk:
                break
    response = json.loads(b"".join(chunks).decode().strip())
    if expect_ok and response.get("ok") is not True:
        raise RuntimeError(f"{method} failed: {response.get('error')}")
    if not expect_ok and response.get("ok") is True:
        raise RuntimeError(f"{method} unexpectedly succeeded")
    return response


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    grant = output_dir / "grant.json"
    url = (ROOT / "test_pages/form_plan/index.html").resolve().as_uri()
    command = [
        str(args.bin.resolve()),
        "bridge",
        "--servoshell",
        str(args.servoshell.resolve()),
        "--url",
        url,
        "--output-dir",
        str(output_dir / "bridge"),
        "--grant-path",
        str(grant),
        "--timeout-sec",
        str(args.timeout_sec),
    ]
    proc = subprocess.Popen(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        bufsize=1,
    )
    assert proc.stdout is not None
    lines: "queue.Queue[str]" = queue.Queue()
    threading.Thread(target=read_lines, args=(proc.stdout, lines), daemon=True).start()
    failures: list[str] = []
    endpoint = ""
    startup: list[str] = []
    try:
        endpoint, startup = wait_ready(proc, lines, args.timeout_sec)
        ping = call(endpoint, "ping", {}, args.timeout_sec)["result"]
        inventory = call(endpoint, "form_inventory", {}, args.timeout_sec)["result"]
        fields = inventory.get("fields", [])
        by_id = {field.get("field_id"): field for field in fields}

        expected_eligible = {
            "id:team", "id:region", "id:instances", "id:launch-date",
            "id:include-staging", "id:summary",
        }
        observed_eligible = {field["field_id"] for field in fields if field.get("eligible")}
        if observed_eligible != expected_eligible:
            failures.append(f"eligible mismatch: {sorted(observed_eligible)}")
        if inventory.get("field_count") != 17:
            failures.append(f"field_count={inventory.get('field_count')} expected=17")
        if inventory.get("eligible_count") != 6:
            failures.append(f"eligible_count={inventory.get('eligible_count')} expected=6")
        if inventory.get("sensitive_count") != 2:
            failures.append(f"sensitive_count={inventory.get('sensitive_count')} expected=2")
        if any("value" in field for field in fields):
            failures.append("inventory returned a raw value key")
        if by_id.get("id:ssn", {}).get("sensitivity") != "government_or_tax_id":
            failures.append("SSN was not classified sensitive")
        if by_id.get("id:user-note", {}).get("value_state") != "present_redacted":
            failures.append("existing user note was not redacted")

        assignments = {field["field_id"]: f"value-{index}" for index, field in enumerate(fields)}
        assignments["id:team"] = SENTINELS[0]
        assignments["id:ssn"] = SENTINELS[1]
        assignments["id:user-note"] = SENTINELS[2]
        assignments["id:not-found"] = "missing-value"
        revision = int(ping["page_revision"])
        plan = call(
            endpoint,
            "form_compile_plan",
            {
                "basis_page_revision": revision,
                "assignments": assignments,
                "policy": {"block_sensitive": True, "preserve_existing": True, "no_submit": True},
            },
            args.timeout_sec,
        )["result"]
        planned = {field["field_id"] for field in plan.get("eligible", [])}
        if planned != expected_eligible:
            failures.append(f"planned eligible mismatch: {sorted(planned)}")
        planned_by_id = {field["field_id"]: field for field in plan.get("eligible", [])}
        if planned_by_id.get("id:team", {}).get("owner") != "agent":
            failures.append("explicit agent owner was not preserved")
        if planned_by_id.get("id:region", {}).get("owner") != "explicit_plan":
            failures.append("unknown owner was not marked explicit_plan")
        rejected = {field["field_id"]: field for field in plan.get("rejected", [])}
        for field_id, reason in {
            "id:ssn": "sensitive_requires_human",
            "id:user-note": "human_owned",
            "id:project-code": "preserve_existing_value",
            "id:disabled-field": "disabled",
            "id:file-upload": "unsupported_type",
            "id:ambiguous": "ambiguous_label",
            "id:not-found": "not_found",
        }.items():
            blocked = rejected.get(field_id, {})
            reasons = blocked.get("blocked_reasons", [blocked.get("reason")])
            if reason not in reasons:
                failures.append(f"{field_id} missing rejection {reason}: {blocked}")
        stale = call(
            endpoint,
            "form_compile_plan",
            {"basis_page_revision": revision + 1, "assignments": {"id:team": "stale"}},
            args.timeout_sec,
            expect_ok=False,
        )
        if "stale form plan basis" not in str(stale.get("error")):
            failures.append(f"stale plan error missing: {stale}")
        unsafe_policy = call(
            endpoint,
            "form_compile_plan",
            {
                "basis_page_revision": revision,
                "assignments": {"id:team": "unsafe"},
                "policy": {"block_sensitive": False, "preserve_existing": True, "no_submit": True},
            },
            args.timeout_sec,
            expect_ok=False,
        )
        if "block_sensitive=true" not in str(unsafe_policy.get("error")):
            failures.append(f"unsafe policy error missing: {unsafe_policy}")
        structured_value = call(
            endpoint,
            "form_compile_plan",
            {
                "basis_page_revision": revision,
                "assignments": {"id:team": {"unexpected": "object"}},
                "policy": {"block_sensitive": True, "preserve_existing": True, "no_submit": True},
            },
            args.timeout_sec,
            expect_ok=False,
        )
        if "must be scalar" not in str(structured_value.get("error")):
            failures.append(f"structured assignment error missing: {structured_value}")

        serialized = json.dumps({"inventory": inventory, "plan": plan, "startup": startup})
        leaked = [sentinel for sentinel in SENTINELS if sentinel in serialized]
        if leaked:
            failures.append(f"result leaked assignment values: {leaked}")
        report = {
            "ok": not failures,
            "engine": "saccade-servoshell-generic-form-plan-probe-v0",
            "url": url,
            "field_count": inventory.get("field_count"),
            "eligible_count": inventory.get("eligible_count"),
            "sensitive_count": inventory.get("sensitive_count"),
            "planned_count": len(plan.get("eligible", [])),
            "rejected_count": len(plan.get("rejected", [])),
            "field_decisions": [
                {
                    "field_id": field.get("field_id"),
                    "label": field.get("label"),
                    "owner": field.get("owner"),
                    "sensitivity": field.get("sensitivity"),
                    "value_state": field.get("value_state"),
                    "eligible": field.get("eligible"),
                    "blocked_reasons": field.get("blocked_reasons"),
                }
                for field in fields
            ],
            "stale_plan_blocked": stale.get("ok") is False,
            "unsafe_policy_blocked": unsafe_policy.get("ok") is False,
            "structured_assignment_blocked": structured_value.get("ok") is False,
            "values_logged": False,
            "writes_executed": plan.get("policy", {}).get("writes_executed"),
            "failures": failures,
        }
        (output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        call(endpoint, "shutdown", {}, args.timeout_sec)
        print(
            f"GENERIC FORM PLAN {'PASS' if report['ok'] else 'FAIL'} "
            f"fields={report['field_count']} eligible={report['eligible_count']} "
            f"rejected={report['rejected_count']} report={output_dir / 'report.json'}"
        )
        return 0 if report["ok"] else 1
    finally:
        try:
            if endpoint:
                call(endpoint, "shutdown", {}, 2.0)
        except Exception:
            pass
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()


if __name__ == "__main__":
    raise SystemExit(main())
