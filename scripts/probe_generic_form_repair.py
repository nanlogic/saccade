#!/usr/bin/env python3
"""Prove generic form execution reports postcondition failures without leaking values."""

from __future__ import annotations

import argparse
import json
import queue
import subprocess
import threading
from pathlib import Path

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


NORMAL_SENTINEL = "FORMREPAIR_NORMAL_SECRET"
NORMALIZED_SENTINEL = "formrepair_normalized_secret"
SSN_SENTINEL = "FORMREPAIR_SSN_SECRET"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--mcp-bin", type=Path, default=DEFAULT_MCP_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "runs/formmax/generic_repair")
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    grant = output_dir / "grant.json"
    url = (ROOT / "test_pages/form_repair/index.html").resolve().as_uri()
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
    startup: list[str] = []
    mcp_proc: subprocess.Popen[str] | None = None
    failures: list[str] = []
    try:
        endpoint, startup = wait_ready(proc, lines, args.timeout_sec)
        load_control_capability(grant)
        ping = call(endpoint, "ping", {}, args.timeout_sec)["result"]
        revision = int(ping["page_revision"])
        safe_policy = {"block_sensitive": True, "preserve_existing": True, "no_submit": True}
        assignments = {
            "id:normal-field": NORMAL_SENTINEL,
            "id:normalized-code": NORMALIZED_SENTINEL,
            "id:existing-field": "overwrite-attempt",
            "id:ssn": SSN_SENTINEL,
        }

        mcp_proc = subprocess.Popen(
            [str(args.mcp_bin.resolve()), "serve-stdio"], cwd=ROOT, text=True,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, bufsize=1,
        )
        mcp_rpc(mcp_proc, 1, "initialize", {})
        granted = mcp_tool(
            mcp_proc, 2, "saccade.tabs.grant_current",
            {"grant_path": str(grant), "reason": "AI-031 postcondition repair gate"},
        )
        tab_id = int(granted["tab"]["tab_id"])
        inventory = mcp_tool(
            mcp_proc,
            3,
            "saccade.web.form_inventory",
            {"tab_id": tab_id, "mode": "full"},
        )
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

        filled = {item.get("field_id") for item in execution.get("filled", [])}
        failed = {item.get("field_id"): item.get("reason") for item in execution.get("failed", [])}
        repair = {item.get("field_id"): item.get("action") for item in execution.get("repair", [])}
        preserved = {item.get("field_id") for item in execution.get("preserved", [])}
        if filled != {"id:normal-field"}:
            failures.append(f"unexpected filled fields: {sorted(filled)}")
        if failed != {"id:normalized-code": "postcondition_mismatch"}:
            failures.append(f"unexpected failures: {failed}")
        if repair != {"id:normalized-code": "human_review_or_remap"}:
            failures.append(f"repair may loop or is wrong: {repair}")
        if preserved != {"id:existing-field"}:
            failures.append(f"existing value was not preserved: {sorted(preserved)}")
        if execution.get("receipt_verified") is not False:
            failures.append("partial failure incorrectly produced a verified receipt")
        if execution.get("write_attempted_count") != 2:
            failures.append(f"write_attempted_count={execution.get('write_attempted_count')} expected=2")
        if execution.get("policy", {}).get("writes_executed") is not True:
            failures.append("write attempts were not reported")

        post_inventory = mcp_tool(
            mcp_proc,
            6,
            "saccade.web.form_inventory",
            {"tab_id": tab_id, "mode": "full"},
        )
        post_fields = {field.get("field_id"): field for field in post_inventory.get("fields", [])}
        if post_fields.get("id:ssn", {}).get("value_state") != "requires_user_input":
            failures.append("sensitive field completion state changed")
        if post_fields.get("id:existing-field", {}).get("value_state") != "present_redacted":
            failures.append("existing field was not retained")
        post_ping = call(endpoint, "ping", {}, args.timeout_sec)["result"]
        if int(post_ping.get("page_revision", -1)) != revision + 1:
            failures.append(f"write attempt did not advance revision: {post_ping.get('page_revision')}")
        stale = call(
            endpoint, "form_execute_plan",
            {
                "basis_page_revision": revision,
                "expected_plan_id": plan.get("plan_id"),
                "assignments": assignments,
                "policy": safe_policy,
            },
            args.timeout_sec, expect_ok=False,
        )
        if "stale form execution basis" not in str(stale.get("error")):
            failures.append(f"original plan was not made stale: {stale}")

        value_free = json.dumps({
            "inventory": inventory, "plan": plan, "execution": execution,
            "post_inventory": post_inventory, "startup": startup,
        })
        sentinels = [NORMAL_SENTINEL, NORMALIZED_SENTINEL, SSN_SENTINEL]
        leaks = [sentinel for sentinel in sentinels if sentinel in value_free]
        if leaks:
            failures.append(f"response leaked assignment values: {leaks}")

        report = {
            "ok": not failures,
            "engine": "saccade-servoshell-generic-form-repair-probe-v0",
            "url": url,
            "field_count": inventory.get("field_count"),
            "planned_count": len(plan.get("eligible", [])),
            "filled_count": len(execution.get("filled", [])),
            "preserved_count": len(execution.get("preserved", [])),
            "failed_count": len(execution.get("failed", [])),
            "repair_count": len(execution.get("repair", [])),
            "receipt_verified": execution.get("receipt_verified"),
            "write_attempted_count": execution.get("write_attempted_count"),
            "revision_advanced": int(post_ping.get("page_revision", -1)) == revision + 1,
            "stale_execution_blocked": stale.get("ok") is False,
            "mcp_attached": granted.get("same_webview_attached") is True,
            "repair": execution.get("repair", []),
            "values_logged": False,
            "failures": failures,
        }
        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        call(endpoint, "shutdown", {}, args.timeout_sec)
        print(
            f"GENERIC FORM REPAIR {'PASS' if report['ok'] else 'FAIL'} "
            f"filled={report['filled_count']} failed={report['failed_count']} "
            f"repair={report['repair_count']} report={report_path}"
        )
        return 0 if report["ok"] else 1
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
