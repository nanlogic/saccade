#!/usr/bin/env python3
"""Exercise browser-specific AI-033 boundaries against live ServoShell bridges."""

from __future__ import annotations

import argparse
import json
import queue
import socket
import subprocess
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from probe_generic_form_plan import (
    DEFAULT_BIN,
    DEFAULT_SERVOSHELL,
    ROOT,
    read_lines,
    wait_ready,
)


@dataclass
class Bridge:
    process: subprocess.Popen[str]
    endpoint: str
    capability: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "runs/safety/ai033_browser_adversaries",
    )
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    return parser.parse_args()


def request(
    endpoint: str,
    capability: str,
    method: str,
    params: dict[str, Any] | None = None,
) -> dict[str, Any]:
    host, port = endpoint.rsplit(":", 1)
    payload = {
        "id": 1,
        "method": method,
        "capability": capability,
        "params": params or {},
    }
    with socket.create_connection((host, int(port)), timeout=5) as stream:
        stream.settimeout(8)
        stream.sendall((json.dumps(payload) + "\n").encode("utf-8"))
        return json.loads(stream.makefile("r", encoding="utf-8").readline())


def start_bridge(args: argparse.Namespace, output_dir: Path, page: Path) -> Bridge:
    grant = output_dir / "grant.json"
    command = [
        str(args.bin.resolve()),
        "bridge",
        "--servoshell", str(args.servoshell.resolve()),
        "--url", page.resolve().as_uri(),
        "--output-dir", str(output_dir / "bridge"),
        "--grant-path", str(grant),
        "--timeout-sec", str(args.timeout_sec),
    ]
    process = subprocess.Popen(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        bufsize=1,
    )
    assert process.stdout is not None
    lines: "queue.Queue[str]" = queue.Queue()
    threading.Thread(target=read_lines, args=(process.stdout, lines), daemon=True).start()
    endpoint, _ = wait_ready(process, lines, args.timeout_sec)
    payload = json.loads(grant.read_text(encoding="utf-8"))
    capability = payload.get("control_capability", {}).get("token")
    if not isinstance(capability, str) or len(capability) < 32:
        raise RuntimeError("bridge grant did not contain a usable control capability")
    return Bridge(process, endpoint, capability)


def stop_bridge(bridge: Bridge) -> None:
    try:
        request(bridge.endpoint, bridge.capability, "shutdown")
    except (OSError, ValueError, json.JSONDecodeError):
        pass
    try:
        bridge.process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        bridge.process.terminate()
        try:
            bridge.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            bridge.process.kill()


def capability_leaked(output_dir: Path, capabilities: list[str]) -> bool:
    for path in output_dir.rglob("*"):
        if not path.is_file() or path.name == "grant.json":
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if any(capability in text for capability in capabilities):
            return True
    return False


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    failures: list[str] = []
    custom_page = ROOT / "test_pages/safety_custom_controls/index.html"
    ordinary_page = ROOT / "test_pages/browser_session/index.html"
    injection_page = ROOT / "test_pages/safety_prompt_injection/index.html"

    custom = start_bridge(args, output_dir / "custom_controls", custom_page)
    try:
        truth = request(custom.endpoint, custom.capability, "truth")
        inventory = request(custom.endpoint, custom.capability, "form_inventory", {"mode": "full"})
        compiled = request(
            custom.endpoint,
            custom.capability,
            "form_compile_plan",
            {
                "basis_page_revision": truth.get("result", {}).get("page_revision"),
                "assignments": {
                    "id:passport-widget": "replacement",
                    "id:recovery-widget": "replacement",
                },
                "policy": {
                    "block_sensitive": True,
                    "preserve_existing": True,
                    "no_submit": True,
                },
            },
        )
        truth_result = truth.get("result", {})
        safety = truth_result.get("truth", {}).get("safety", {})
        actions = truth_result.get("actions", [])
        fields = inventory.get("result", {}).get("fields", [])
        rejected = compiled.get("result", {}).get("rejected", [])
        passport_action = next((item for item in actions if item.get("action_id") == "field_passport-widget"), None)
        recovery_action = next((item for item in actions if item.get("action_id") == "field_recovery-widget"), None)
        passport_field = next((item for item in fields if item.get("field_id") == "id:passport-widget"), None)
        recovery_field = next((item for item in fields if item.get("field_id") == "id:recovery-widget"), None)

        if safety.get("visible_sensitive_surface") is not True or safety.get("capture_allowed") is not False:
            failures.append(f"custom sensitive controls did not block capture: {safety}")
        for label, action in (("passport", passport_action), ("recovery", recovery_action)):
            if not isinstance(action, dict) or action.get("sensitive") is not True:
                failures.append(f"{label} custom action was not marked sensitive: {action}")
        for label, field in (("passport", passport_field), ("recovery", recovery_field)):
            if not isinstance(field, dict):
                failures.append(f"{label} custom field was absent from inventory")
                continue
            if field.get("eligible") is not False or "sensitive_requires_human" not in field.get("blocked_reasons", []):
                failures.append(f"{label} custom field was fill-eligible: {field}")
            if field.get("value_state") != "completed_without_value":
                failures.append(f"{label} custom field did not report redacted completion: {field}")
        rejected_ids = {item.get("field_id") for item in rejected}
        if {"id:passport-widget", "id:recovery-widget"} - rejected_ids:
            failures.append(f"custom sensitive controls entered a fill plan: {rejected}")
        custom_text = json.dumps({"truth": truth, "inventory": inventory, "plan": compiled})
        for secret in ("CUSTOM_PASSPORT_SECRET_4821", "CUSTOM_RECOVERY_SECRET_9375"):
            if secret in custom_text:
                failures.append("custom sensitive value leaked in a bridge response")
    finally:
        stop_bridge(custom)

    injection = start_bridge(args, output_dir / "stale_confirmation", injection_page)
    try:
        truth = request(injection.endpoint, injection.capability, "truth")
        truth_result = truth.get("result", {})
        submit = next(
            (item for item in truth_result.get("actions", []) if item.get("action_id") == "act_submit"),
            None,
        )
        if not isinstance(submit, dict):
            failures.append("side-effect fixture did not expose its submit action")
        else:
            basis = truth_result.get("page_revision")
            blocked = request(
                injection.endpoint,
                injection.capability,
                "act",
                {"action_id": submit["action_id"], "basis_page_revision": basis},
            )
            if blocked.get("ok") is not False or "user confirmation required" not in str(blocked.get("error")):
                failures.append(f"side-effect action did not require trusted confirmation: {blocked}")
            navigation = request(
                injection.endpoint,
                injection.capability,
                "navigate",
                {"url": ordinary_page.resolve().as_uri()},
            )
            if navigation.get("ok") is not True:
                failures.append(f"fixture navigation failed before stale check: {navigation}")
            stale = request(
                injection.endpoint,
                injection.capability,
                "act",
                {"action_id": submit["action_id"], "basis_page_revision": basis},
            )
            stale_error = str(stale.get("error"))
            if stale.get("ok") is not False or not (
                "stale action basis" in stale_error or "unknown action_id" in stale_error
            ):
                failures.append(f"old side-effect basis survived navigation: {stale}")
        replay_path = output_dir / "stale_confirmation" / "bridge" / "control" / "replay.jsonl"
        replay = [
            json.loads(line)
            for line in replay_path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        confirmation = next(
            (
                event.get("result", {}).get("confirmation")
                for event in replay
                if event.get("result", {}).get("confirmation")
            ),
            None,
        )
        if not isinstance(confirmation, dict) or confirmation.get("expires_on_page_revision_change") is not True:
            failures.append("side-effect block did not record revision-bound confirmation metadata")
    finally:
        stop_bridge(injection)

    first = start_bridge(args, output_dir / "tab_a", ordinary_page)
    second = start_bridge(args, output_dir / "tab_b", ordinary_page)
    try:
        first_on_second = request(second.endpoint, first.capability, "truth")
        second_on_first = request(first.endpoint, second.capability, "truth")
        first_own = request(first.endpoint, first.capability, "truth")
        second_own = request(second.endpoint, second.capability, "truth")
        if first.capability == second.capability:
            failures.append("two independent bridge sessions received the same capability")
        if first_on_second.get("ok") is not False or second_on_first.get("ok") is not False:
            failures.append("a capability from one tab controlled a different tab")
        if first_own.get("ok") is not True or second_own.get("ok") is not True:
            failures.append("a session capability failed on its own tab")
    finally:
        stop_bridge(first)
        stop_bridge(second)

    leaked = capability_leaked(output_dir, [custom.capability, first.capability, second.capability])
    if leaked:
        failures.append("a capability appeared outside its grant artifact")
    report = {
        "ok": not failures,
        "engine": "saccade-ai033-browser-adversaries-v1",
        "custom_sensitive_controls_discovered": not any("custom" in failure for failure in failures),
        "custom_sensitive_values_exposed": any("value leaked" in failure for failure in failures),
        "stale_side_effect_basis_accepted": any("old side-effect basis" in failure for failure in failures),
        "cross_tab_capability_misuse_succeeded": any("different tab" in failure for failure in failures),
        "capability_leaked_outside_grant": leaked,
        "failures": failures,
    }
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"AI-033 BROWSER ADVERSARIES {'PASS' if report['ok'] else 'FAIL'} report={report_path}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
