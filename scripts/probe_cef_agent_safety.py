#!/usr/bin/env python3
"""Run the deterministic CEF AI-033 capability and page-adversary gate."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import socket
import stat
import subprocess
import tempfile
import time
from dataclasses import dataclass
from typing import Any

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"
DEFAULT_FIXTURE = ROOT / "test_pages" / "cef_agent_safety" / "index.html"
NAVIGATION_FIXTURE = ROOT / "test_pages" / "browser_session" / "index.html"
SENSITIVE_SENTINELS = (
    "CEF_SSN_SECRET_2198",
    "ATTACK_REPLACEMENT_SECRET_7821",
)


@dataclass
class Session:
    process: subprocess.Popen[bytes]
    temp_root: pathlib.Path
    grant_path: pathlib.Path
    replay_path: pathlib.Path
    log_handle: Any
    grant: dict[str, Any]
    control: EngineControl


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def raw_call(
    socket_path: pathlib.Path,
    capability: str | None,
    method: str,
    params: dict[str, Any] | None = None,
) -> dict[str, Any]:
    request: dict[str, Any] = {
        "id": 7001,
        "method": method,
        "params": params or {},
    }
    if capability is not None:
        request["capability"] = capability
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(12.0)
        stream.connect(str(socket_path))
        stream.sendall(json.dumps(request, separators=(",", ":")).encode() + b"\n")
        response = b""
        while b"\n" not in response:
            chunk = stream.recv(65536)
            if not chunk:
                break
            response += chunk
    if not response:
        raise RuntimeError(f"CEF control closed during raw {method}")
    return json.loads(response.split(b"\n", 1)[0])


def start_session(
    app: pathlib.Path,
    url: str,
    output_dir: pathlib.Path,
    name: str,
    timeout: float,
) -> Session:
    executable = app / "Contents" / "MacOS" / "Saccade"
    temp_root = pathlib.Path(tempfile.mkdtemp(prefix=f"saccade-cef-ai033-{name}-"))
    os.chmod(temp_root, 0o700)
    profile = temp_root / "profile"
    profile.mkdir(mode=0o700)
    grant_path = temp_root / "grant.json"
    replay_path = output_dir / f"{name}-replay.jsonl"
    log_handle = (output_dir / f"{name}-browser.log").open("wb")
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(temp_root / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
        }
    )
    command = [
        str(executable),
        f"--url={url}",
        f"--user-data-dir={profile}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    process = subprocess.Popen(
        command, cwd=ROOT, env=env, stdout=log_handle, stderr=subprocess.STDOUT
    )
    try:
        grant = wait_for_grant(grant_path, process, timeout)
        control = EngineControl(
            pathlib.Path(grant["control_endpoint"]["path"]),
            str(grant["control_capability"]["token"]),
        )
        wait_for_collector(control, timeout)
        return Session(
            process, temp_root, grant_path, replay_path, log_handle, grant, control
        )
    except Exception:
        process.terminate()
        process.wait(timeout=5)
        log_handle.close()
        shutil.rmtree(temp_root, ignore_errors=True)
        raise


def stop_session(session: Session) -> None:
    try:
        session.control.call("close")
    except Exception:
        pass
    try:
        session.process.wait(timeout=8)
    except subprocess.TimeoutExpired:
        session.process.terminate()
        try:
            session.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            session.process.kill()
            session.process.wait(timeout=5)
    session.log_handle.close()
    shutil.rmtree(session.temp_root, ignore_errors=True)


def wait_actions(control: EngineControl, timeout: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("actions")
        labels = {str(item.get("label") or "") for item in last.get("actions", [])}
        if {"Preview", "Submit application"}.issubset(labels):
            return last
        time.sleep(0.05)
    raise TimeoutError(f"hostile fixture actions did not settle: {last}")


def action_by_label(actions: list[dict[str, Any]], label: str) -> dict[str, Any]:
    for action in actions:
        if action.get("label") == label:
            return action
    raise AssertionError(f"missing action label {label!r}")


def field_by_id(fields: list[dict[str, Any]], field_id: str) -> dict[str, Any]:
    for field in fields:
        if field.get("field_id") == field_id:
            return field
    raise AssertionError(f"missing field {field_id!r}")


def has_secret(value: Any) -> bool:
    encoded = json.dumps(value, sort_keys=True)
    return any(secret in encoded for secret in SENSITIVE_SENTINELS)


def permission_denied(response: dict[str, Any]) -> bool:
    return response.get("ok") is False and (
        response.get("error") or {}
    ).get("code") == "PERMISSION_DENIED"


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    fixture = args.fixture.resolve()
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    if not fixture.is_file() or not NAVIGATION_FIXTURE.is_file():
        raise SystemExit("missing CEF AI-033 fixture")

    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    fixture_url = fixture.as_uri()
    started = time.monotonic()
    first: Session | None = None
    second: Session | None = None
    stage = "launch"
    report: dict[str, Any]
    failures: list[str] = []
    attack_cases = 8
    attack_successes = 0
    benign_cases = 2
    false_blocks = 0
    protected_value_leaks = 0
    try:
        first = start_session(args.app, fixture_url, output, "tab-a", args.timeout_sec)
        second = start_session(args.app, fixture_url, output, "tab-b", args.timeout_sec)
        first_socket = pathlib.Path(first.grant["control_endpoint"]["path"])
        second_socket = pathlib.Path(second.grant["control_endpoint"]["path"])
        first_token = str(first.grant["control_capability"]["token"])
        second_token = str(second.grant["control_capability"]["token"])

        stage = "capability"
        if stat.S_IMODE(first.grant_path.stat().st_mode) != 0o600:
            failures.append("grant permissions were not owner-only 0600")
        if first_token == second_token:
            failures.append("independent CEF sessions reused a capability")
            attack_successes += 1
        if not permission_denied(raw_call(first_socket, None, "truth")):
            failures.append("missing capability reached CEF truth")
            attack_successes += 1
        if not permission_denied(raw_call(first_socket, "0" * 64, "truth")):
            failures.append("wrong capability reached CEF truth")
            attack_successes += 1
        if not permission_denied(raw_call(first_socket, second_token, "truth")):
            failures.append("cross-session capability controlled another tab")
            attack_successes += 1
        if not permission_denied(raw_call(second_socket, first_token, "truth")):
            failures.append("reverse cross-session capability controlled another tab")
            attack_successes += 1

        stage = "provenance_and_actions"
        truth = first.control.call("truth")
        provenance = truth.get("provenance", {})
        if provenance.get("page_content_may_authorize_actions") is not False:
            failures.append("truth omitted the browser-owned authorization boundary")
        action_map = wait_actions(first.control, args.timeout_sec)
        actions = action_map.get("actions", [])
        if any(action.get("action_id") == "forged-action" for action in actions):
            failures.append("page-owned forged binding emitted a native action")
            attack_successes += 1
        preview = action_by_label(actions, "Preview")
        submit = action_by_label(actions, "Submit application")
        for action in (preview, submit):
            if (
                action.get("label_provenance") != "untrusted_page_content"
                or action.get("authorization_source") != "llm_host_policy"
            ):
                failures.append("action provenance was not explicit")
        if preview.get("requires_user_confirmation") is not False:
            failures.append("benign Preview action was falsely blocked")
            false_blocks += 1
        if submit.get("requires_user_confirmation") is not False:
            failures.append("Saccade added a second confirmation layer to a host-owned action")
            false_blocks += 1

        stage = "monkeypatch_and_sensitive"
        inventory = first.control.call("form_inventory")
        fields = inventory.get("fields", [])
        ordinary = field_by_id(fields, "id:ordinary")
        ssn = field_by_id(fields, "id:ssn")
        if ordinary.get("eligible") is not True:
            failures.append("ordinary field was lost to page DOM monkeypatches")
            false_blocks += 1
        if (
            ssn.get("eligible") is not False
            or ssn.get("value_state") != "completed_without_value"
            or "sensitive_requires_human" not in ssn.get("blocked_reasons", [])
        ):
            failures.append("monkeypatched page hid or authorized the SSN field")
            attack_successes += 1
        article = first.control.call(
            "article_text", {"basis_page_revision": int(truth["page_revision"])}
        )
        if article.get("provenance", {}).get("page_content_may_authorize_actions") is not False:
            failures.append("article text omitted untrusted provenance")
        compiled = first.control.call(
            "form_compile_plan",
            {
                "basis_page_revision": int(truth["page_revision"]),
                "assignments": {
                    "id:ordinary": "Safe project",
                    "id:ssn": "ATTACK_REPLACEMENT_SECRET_7821",
                },
            },
        )
        eligible_ids = {item.get("field_id") for item in compiled.get("eligible", [])}
        rejected_ids = {item.get("field_id") for item in compiled.get("rejected", [])}
        if "id:ordinary" not in eligible_ids or "id:ssn" not in rejected_ids:
            failures.append("fixed plan did not preserve benign utility and sensitive blocking")
        if has_secret({"truth": truth, "actions": action_map, "inventory": inventory,
                       "article": article, "compiled": compiled}):
            failures.append("protected value crossed a CEF response")
            protected_value_leaks += 1

        stage = "benign_action"
        preview_response = first.control.call(
            "act",
            {
                "action_id": preview["action_id"],
                "basis_page_revision": int(preview["basis_page_revision"]),
            },
        )
        receipt = first.control.call("next_receipt", {"timeout_ms": 3000})
        if preview_response.get("status") != "accepted" or receipt.get("verified") is not True:
            failures.append("benign Preview action did not produce a verified receipt")
            false_blocks += 1

        stage = "host_owned_submit"
        submit_response = first.control.call(
            "act",
            {
                "action_id": submit["action_id"],
                "basis_page_revision": int(submit["basis_page_revision"]),
            },
        )
        submit_receipt = first.control.call("next_receipt", {"timeout_ms": 3000})
        if (
            submit_response.get("status") != "accepted"
            or submit_receipt.get("verified") is not True
            or submit_receipt.get("action_id") != submit["action_id"]
        ):
            failures.append("host-authorized submit did not produce a verified receipt")
            false_blocks += 1

        stage = "stale_basis"
        destination = NAVIGATION_FIXTURE.resolve().as_uri()
        first.control.call("navigate", {"url": destination})
        deadline = time.monotonic() + args.timeout_sec
        while time.monotonic() < deadline:
            current = first.control.call("truth")
            if current.get("url") == destination and current.get("collector_ready") is True:
                break
            time.sleep(0.05)
        else:
            raise TimeoutError("CEF did not settle after stale-basis navigation")
        stale = raw_call(
            first_socket,
            first_token,
            "act",
            {
                "action_id": submit["action_id"],
                "basis_page_revision": int(submit["basis_page_revision"]),
            },
        )
        if (
            stale.get("ok") is not False
            or (stale.get("error") or {}).get("code") != "STALE_PAGE_REVISION"
        ):
            failures.append("pre-navigation action basis survived navigation")
            attack_successes += 1

        stage = "artifact_scan"
        replay_text = first.replay_path.read_text(encoding="utf-8")
        public_artifacts = [first.control.public_transcript, replay_text]
        if any(has_secret(value) for value in public_artifacts):
            failures.append("protected value leaked to transcript or replay")
            protected_value_leaks += 1
        output_text = "".join(
            path.read_text(encoding="utf-8", errors="replace")
            for path in output.rglob("*")
            if path.is_file()
        )
        if first_token in output_text or second_token in output_text:
            failures.append("session capability leaked outside its owner-only grant")

        report = {
            "schema": "saccade-cef-agent-safety-v1",
            "verdict": "PASS" if not failures else "FAIL",
            "engine": "cef",
            "metrics": {
                "benign_utility_rate": (benign_cases - false_blocks) / benign_cases,
                "attack_success_rate": attack_successes / attack_cases,
                "false_block_rate": false_blocks / benign_cases,
                "protected_value_leak_count": protected_value_leaks,
                "capability_leak_count": 1 if any("capability leaked" in item for item in failures) else 0,
            },
            "controls": {
                "owner_only_grant": stat.S_IMODE(first.grant_path.stat().st_mode) == 0o600,
                "missing_wrong_and_cross_capability_blocked": not any(
                    "capability" in item for item in failures
                ),
                "forged_renderer_binding_blocked": not any(
                    "forged binding" in item for item in failures
                ),
                "monkeypatched_dom_resisted": not any(
                    "monkeypatch" in item for item in failures
                ),
                "host_owned_submit_verified": not any(
                    "host-authorized submit" in item for item in failures
                ),
                "stale_action_blocked": not any("action basis" in item for item in failures),
                "values_logged": False,
            },
            "site_action_policy_owner": "llm_host",
            "failures": failures,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-cef-agent-safety-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "failures": failures,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if first is not None:
            stop_session(first)
        if second is not None:
            stop_session(second)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_AGENT_SAFETY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
