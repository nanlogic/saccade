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
DEFAULT_MCP_BIN = ROOT / "target/debug/saccade-mcp"
DEFAULT_SERVOSHELL = Path("/Applications/Servo.app/Contents/MacOS/servoshell")
SENTINELS = ["FORMPLAN_TEAM_SECRET", "FORMPLAN_SSN_SECRET", "FORMPLAN_NOTE_SECRET"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--mcp-bin", type=Path, default=DEFAULT_MCP_BIN)
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


def mcp_rpc(proc: subprocess.Popen[str], request_id: int, method: str, params: dict[str, Any]) -> dict[str, Any]:
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n")
    proc.stdin.flush()
    line = proc.stdout.readline()
    if not line:
        stderr = proc.stderr.read() if proc.stderr else ""
        raise RuntimeError(f"MCP exited during {method}: {stderr}")
    response = json.loads(line)
    if response.get("error"):
        raise RuntimeError(f"MCP {method} failed: {response['error']}")
    return response


def mcp_tool(proc: subprocess.Popen[str], request_id: int, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    response = mcp_rpc(proc, request_id, "tools/call", {"name": name, "arguments": arguments})
    content = response.get("result", {}).get("structuredContent")
    if not isinstance(content, dict) or content.get("status") != "ok":
        raise RuntimeError(f"MCP tool {name} failed: {response}")
    return content


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
    mcp_proc: subprocess.Popen[str] | None = None
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
        assignments["id:region"] = "US West"
        assignments["id:instances"] = 24
        assignments["id:launch-date"] = "2026-08-15"
        assignments["id:include-staging"] = True
        assignments["id:summary"] = "Public fixture summary"
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
        if "non-null scalars" not in str(structured_value.get("error")):
            failures.append(f"structured assignment error missing: {structured_value}")

        safe_policy = {"block_sensitive": True, "preserve_existing": True, "no_submit": True}
        wrong_plan = call(
            endpoint,
            "form_execute_plan",
            {
                "basis_page_revision": revision,
                "expected_plan_id": "wrong-plan-id",
                "assignments": assignments,
                "policy": safe_policy,
            },
            args.timeout_sec,
            expect_ok=False,
        )
        if "plan id mismatch" not in str(wrong_plan.get("error")):
            failures.append(f"wrong plan id error missing: {wrong_plan}")

        mcp_proc = subprocess.Popen(
            [str(args.mcp_bin.resolve()), "serve-stdio"],
            cwd=ROOT,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        mcp_rpc(mcp_proc, 1, "initialize", {})
        granted = mcp_tool(
            mcp_proc,
            2,
            "saccade.tabs.grant_current",
            {"grant_path": str(grant), "reason": "AI-031 generic form execution probe"},
        )
        tab_id = int(granted["tab"]["tab_id"])
        mcp_inventory = mcp_tool(mcp_proc, 3, "saccade.web.form_inventory", {"tab_id": tab_id})
        if mcp_inventory.get("field_count") != 17:
            failures.append(f"MCP inventory field_count={mcp_inventory.get('field_count')}")
        mcp_tool(
            mcp_proc,
            4,
            "saccade.web.form_compile_plan",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "assignments": {"id:team": SENTINELS[0]},
                "policy": safe_policy,
            },
        )
        mcp_plan = mcp_tool(
            mcp_proc,
            5,
            "saccade.web.form_compile_plan",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "assignments": assignments,
                "policy": safe_policy,
            },
        )
        if mcp_plan.get("plan_id") != plan.get("plan_id"):
            failures.append("MCP and bridge plan IDs differ")
        execution = mcp_tool(
            mcp_proc,
            6,
            "saccade.web.form_execute_plan",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "expected_plan_id": mcp_plan.get("plan_id"),
                "assignments": assignments,
                "policy": safe_policy,
            },
        )
        filled_ids = {field["field_id"] for field in execution.get("filled", [])}
        if filled_ids != expected_eligible:
            failures.append(f"execution filled mismatch: {sorted(filled_ids)}")
        if execution.get("failed"):
            failures.append(f"execution failures: {execution.get('failed')}")
        if execution.get("repair"):
            failures.append(f"unexpected repair items: {execution.get('repair')}")
        if execution.get("receipt_verified") is not True:
            failures.append("execution receipt was not verified")
        preserved_ids = {field["field_id"] for field in execution.get("preserved", [])}
        expected_preserved = {"id:user-note", "id:project-code", "id:password", "id:hidden-token"}
        if preserved_ids != expected_preserved:
            failures.append(f"preserved mismatch: {sorted(preserved_ids)}")

        post_inventory = call(endpoint, "form_inventory", {}, args.timeout_sec)["result"]
        post_by_id = {field.get("field_id"): field for field in post_inventory.get("fields", [])}
        for field_id in expected_eligible:
            if post_by_id.get(field_id, {}).get("value_state") != "present_redacted":
                failures.append(f"{field_id} did not reach present_redacted")
        if post_by_id.get("id:ssn", {}).get("value_state") != "requires_user_input":
            failures.append("SSN completion state changed during execution")
        post_ping = call(endpoint, "ping", {}, args.timeout_sec)["result"]
        if int(post_ping.get("page_revision", -1)) != revision + 1:
            failures.append(f"page revision did not advance once: {post_ping.get('page_revision')}")
        stale_execution = call(
            endpoint,
            "form_execute_plan",
            {
                "basis_page_revision": revision,
                "expected_plan_id": plan.get("plan_id"),
                "assignments": assignments,
                "policy": safe_policy,
            },
            args.timeout_sec,
            expect_ok=False,
        )
        if "stale form execution basis" not in str(stale_execution.get("error")):
            failures.append(f"stale execution error missing: {stale_execution}")

        serialized = json.dumps({
            "inventory": inventory,
            "plan": plan,
            "execution": execution,
            "post_inventory": post_inventory,
            "startup": startup,
        })
        leaked = [sentinel for sentinel in SENTINELS if sentinel in serialized]
        if leaked:
            failures.append(f"result leaked assignment values: {leaked}")
        report = {
            "ok": not failures,
            "engine": "saccade-servoshell-generic-form-execution-probe-v0",
            "url": url,
            "field_count": inventory.get("field_count"),
            "eligible_count": inventory.get("eligible_count"),
            "sensitive_count": inventory.get("sensitive_count"),
            "planned_count": len(plan.get("eligible", [])),
            "rejected_count": len(plan.get("rejected", [])),
            "filled_count": len(execution.get("filled", [])),
            "preserved_count": len(execution.get("preserved", [])),
            "failed_count": len(execution.get("failed", [])),
            "repair_count": len(execution.get("repair", [])),
            "receipt_verified": execution.get("receipt_verified"),
            "mcp_attached": granted.get("same_webview_attached") is True,
            "mcp_tab_id": tab_id,
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
            "wrong_plan_blocked": wrong_plan.get("ok") is False,
            "stale_execution_blocked": stale_execution.get("ok") is False,
            "values_logged": False,
            "writes_executed": execution.get("policy", {}).get("writes_executed"),
            "failures": failures,
        }
        (output_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        call(endpoint, "shutdown", {}, args.timeout_sec)
        print(
            f"GENERIC FORM EXECUTION {'PASS' if report['ok'] else 'FAIL'} "
            f"fields={report['field_count']} eligible={report['eligible_count']} "
            f"filled={report['filled_count']} preserved={report['preserved_count']} "
            f"rejected={report['rejected_count']} report={output_dir / 'report.json'}"
        )
        return 0 if report["ok"] else 1
    finally:
        try:
            if mcp_proc is not None and mcp_proc.poll() is None:
                mcp_proc.terminate()
                mcp_proc.wait(timeout=3)
        except Exception:
            if mcp_proc is not None and mcp_proc.poll() is None:
                mcp_proc.kill()
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
        remaining = list(startup)
        while True:
            try:
                remaining.append(lines.get_nowait())
            except queue.Empty:
                break
        (output_dir / "process_output.log").write_text("\n".join(remaining) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
