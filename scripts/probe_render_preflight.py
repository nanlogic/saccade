#!/usr/bin/env python3
"""Exercise structural human/agent agreement through the live ServoShell bridge."""

from __future__ import annotations

import argparse
import json
import queue
import subprocess
import threading
from pathlib import Path

from probe_generic_form_plan import (
    DEFAULT_BIN,
    DEFAULT_SERVOSHELL,
    ROOT,
    call,
    load_control_capability,
    read_lines,
    wait_ready,
)


PROTECTED_FIXTURE_VALUES = [
    "fixture-secret",
    "USER-42",
    "Keep this exact note.",
    "do-not-read",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", type=Path, default=DEFAULT_BIN)
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "runs/agreement_gate/live_structural_preflight",
    )
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    grant_path = output_dir / "current_tab_grant.json"
    bridge_output = output_dir / "bridge"
    url = (ROOT / "test_pages/form_plan/index.html").resolve().as_uri()
    command = [
        str(args.bin.resolve()),
        "bridge",
        "--servoshell",
        str(args.servoshell.resolve()),
        "--url",
        url,
        "--output-dir",
        str(bridge_output),
        "--grant-path",
        str(grant_path),
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
    startup: list[str] = []
    response: dict = {}
    try:
        endpoint, startup = wait_ready(proc, lines, args.timeout_sec)
        load_control_capability(grant_path)
        response = call(endpoint, "render_preflight", {}, 10)["result"]
        agreement = response.get("agreement", {})
        observations = response.get("observations", {})
        if response.get("engine") != "saccade-render-preflight-v1":
            failures.append("live bridge did not return render preflight v1")
        if response.get("verdict") != "green" or not response.get("agent_input_allowed"):
            failures.append("local actionable form did not receive a structural green verdict")
        if agreement.get("scope") != "structural_preflight":
            failures.append("agreement scope was not structural_preflight")
        if agreement.get("full_agreement_measured") is not False:
            failures.append("structural preflight claimed full agreement")
        if observations.get("observation_base_consistent") is not True:
            failures.append("inventory and editor probes used different page revisions")
        if agreement.get("visual_evidence", {}).get("status") != "not_captured":
            failures.append("structural preflight captured visual evidence by default")
        serialized = json.dumps(response, sort_keys=True)
        leaked = [value for value in PROTECTED_FIXTURE_VALUES if value in serialized]
        if leaked:
            failures.append(f"protected fixture values leaked: {len(leaked)}")
        call(endpoint, "shutdown", {}, 10)
    finally:
        try:
            proc.wait(timeout=12)
        except subprocess.TimeoutExpired:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()

    report = {
        "ok": not failures,
        "engine": "saccade-render-preflight-live-probe-v1",
        "url": url,
        "bridge_response": response,
        "protected_fixture_values_returned": False
        if response and not any(value in json.dumps(response) for value in PROTECTED_FIXTURE_VALUES)
        else None,
        "startup_tail": startup[-12:],
        "failures": failures,
    }
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"RENDER PREFLIGHT {'PASS' if report['ok'] else 'FAIL'} "
        f"verdict={response.get('verdict')} route={response.get('recommended_route')} "
        f"report={report_path}"
    )
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
