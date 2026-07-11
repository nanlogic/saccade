#!/usr/bin/env python3
"""Measure safe generic form execution on an explicitly public automation test page."""

from __future__ import annotations

import argparse
import json
import queue
import subprocess
import threading
import time
from pathlib import Path
from typing import Any

from probe_generic_form_plan import (
    DEFAULT_BIN,
    DEFAULT_MCP_BIN,
    DEFAULT_SERVOSHELL,
    ROOT,
    call,
    load_control_capability,
    mcp_rpc,
    mcp_tool,
    read_lines,
    wait_ready,
)


SENTINEL = "SACCADE_PUBLIC_FORM_PROBE"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("url")
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--mcp-bin", type=Path, default=DEFAULT_MCP_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=45.0)
    return parser.parse_args()


def assignment_for(field: dict[str, Any]) -> Any | None:
    field_type = field.get("type")
    field_id = str(field.get("field_id", ""))
    if not field.get("eligible") or field.get("sensitivity") != "none":
        return None
    if field.get("value_state") not in {"empty", "requires_user_input"}:
        return None
    if field_type in {"checkbox"}:
        return True
    if field_type in {"radio", "select", "file", "hidden"}:
        return None
    if field_type == "number":
        return 7
    if field_type in {"date", "datetime-local"}:
        return "2026-08-15" if field_type == "date" else "2026-08-15T12:00"
    if field_type == "month":
        return "2026-08"
    if field_type == "week":
        return "2026-W33"
    if field_type == "time":
        return "12:00"
    if field_type == "email":
        return "saccade-probe@example.invalid"
    if field_type == "url":
        return "https://example.invalid/saccade-probe"
    if field_type == "tel":
        return "5550100"
    if field_type in {"text", "search", "textarea", "contenteditable"}:
        return f"{SENTINEL}_{field_id}"
    return None


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    grant = output_dir / "grant.json"
    command = [
        str(args.bin.resolve()), "bridge",
        "--servoshell", str(args.servoshell.resolve()),
        "--url", args.url,
        "--output-dir", str(output_dir / "bridge"),
        "--grant-path", str(grant),
        "--timeout-sec", str(args.timeout_sec),
    ]
    started = time.monotonic()
    proc = subprocess.Popen(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, bufsize=1,
    )
    assert proc.stdout is not None
    lines: "queue.Queue[str]" = queue.Queue()
    threading.Thread(target=read_lines, args=(proc.stdout, lines), daemon=True).start()
    endpoint = ""
    startup: list[str] = []
    mcp_proc: subprocess.Popen[str] | None = None
    failures: list[str] = []
    try:
        endpoint, startup = wait_ready(proc, lines, args.timeout_sec)
        load_control_capability(grant)
        ready_seconds = round(time.monotonic() - started, 3)
        ping = call(endpoint, "ping", {}, args.timeout_sec)["result"]
        revision = int(ping["page_revision"])
        safe_policy = {"block_sensitive": True, "preserve_existing": True, "no_submit": True}

        mcp_proc = subprocess.Popen(
            [str(args.mcp_bin.resolve()), "serve-stdio"], cwd=ROOT, text=True,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=1,
        )
        mcp_rpc(mcp_proc, 1, "initialize", {})
        granted = mcp_tool(
            mcp_proc, 2, "saccade.tabs.grant_current",
            {"grant_path": str(grant), "reason": "AI-031 public automation test form probe"},
        )
        tab_id = int(granted["tab"]["tab_id"])
        inventory_started = time.monotonic()
        inventory = mcp_tool(
            mcp_proc, 3, "saccade.web.form_inventory",
            {"tab_id": tab_id, "mode": "actionable"},
        )
        (output_dir / "inventory_diagnostic.json").write_text(
            json.dumps(inventory, indent=2) + "\n"
        )
        assignments = {
            field["field_id"]: value
            for field in inventory.get("fields", [])
            if (value := assignment_for(field)) is not None
        }
        inventory_seconds = round(time.monotonic() - inventory_started, 3)
        if not assignments:
            failures.append("no safe scalar fields selected")

        plan_started = time.monotonic()
        plan = mcp_tool(
            mcp_proc, 4, "saccade.web.form_compile_plan",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "assignments": assignments,
                "policy": safe_policy,
            },
        )
        execution = mcp_tool(
            mcp_proc, 5, "saccade.web.form_execute_plan",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "expected_plan_id": plan.get("plan_id"),
                "assignments": assignments,
                "policy": safe_policy,
            },
        )
        execute_seconds = round(time.monotonic() - plan_started, 3)
        if execution.get("receipt_verified") is not True:
            failures.append(f"execution receipt failed: {execution.get('failed')}")
        if len(execution.get("filled", [])) != len(assignments):
            failures.append(
                f"filled={len(execution.get('filled', []))} selected={len(assignments)}"
            )
        if execution.get("repair"):
            failures.append(f"unexpected repairs: {execution.get('repair')}")

        response_bundle = json.dumps({"inventory": inventory, "plan": plan, "execution": execution})
        if SENTINEL in response_bundle or "saccade-probe@example.invalid" in response_bundle:
            failures.append("tool response leaked an assignment value")

        report = {
            "ok": not failures,
            "engine": "saccade-public-test-form-probe-v0",
            "url": args.url,
            "ready_seconds": ready_seconds,
            "inventory_seconds": inventory_seconds,
            "plan_execute_seconds": execute_seconds,
            "field_count": inventory.get("field_count"),
            "eligible_count": inventory.get("eligible_count"),
            "sensitive_count": inventory.get("sensitive_count"),
            "selected_count": len(assignments),
            "planned_count": len(plan.get("eligible", [])),
            "filled_count": len(execution.get("filled", [])),
            "preserved_count": len(execution.get("preserved", [])),
            "rejected_count": len(plan.get("rejected", [])),
            "failed_count": len(execution.get("failed", [])),
            "repair_count": len(execution.get("repair", [])),
            "receipt_verified": execution.get("receipt_verified"),
            "write_attempted_count": execution.get("write_attempted_count"),
            "response_chars": len(response_bundle),
            "mcp_attached": granted.get("same_webview_attached") is True,
            "submitted": False,
            "values_logged": False,
            "selected_fields": [
                {
                    "field_id": field.get("field_id"),
                    "label": field.get("label"),
                    "type": field.get("type"),
                }
                for field in inventory.get("fields", [])
                if field.get("field_id") in assignments
            ],
            "failures": failures,
        }
        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        call(endpoint, "shutdown", {}, args.timeout_sec)
        print(
            f"PUBLIC FORM {'PASS' if report['ok'] else 'FAIL'} "
            f"fields={report['field_count']} selected={report['selected_count']} "
            f"filled={report['filled_count']} ready={ready_seconds}s report={report_path}"
        )
        return 0 if report["ok"] else 1
    except Exception as error:
        report = {
            "ok": False,
            "engine": "saccade-public-test-form-probe-v0",
            "url": args.url,
            "ready_seconds": round(time.monotonic() - started, 3),
            "submitted": False,
            "values_logged": False,
            "failures": [str(error)],
        }
        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        print(f"PUBLIC FORM FAIL report={report_path}: {error}")
        return 1
    finally:
        if mcp_proc is not None and mcp_proc.poll() is None:
            mcp_proc.terminate()
            try:
                mcp_proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                mcp_proc.kill()
        if endpoint:
            try:
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
