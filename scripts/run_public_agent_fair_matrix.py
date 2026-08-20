#!/usr/bin/env python3
"""Run the six frozen public read-only tasks in both Codex lane orders."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from run_same_model_matrix import assert_attached_browser, prepare_output


ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts/benchmark_agent_fair.py"
TASK_ROOT = ROOT / "benchmarks/tasks/heavy_public"
ORDERS = ("saccade-first", "playwright-first")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--browser", choices=("chrome", "edge"), default="chrome")
    parser.add_argument("--model")
    parser.add_argument("--effort", choices=("low", "medium", "high", "xhigh"), default="low")
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()

    assert_attached_browser(args.runtime.resolve(), args.runtime_dir.resolve(), args.browser)
    output = args.output.resolve()
    if args.resume:
        output.mkdir(parents=True, exist_ok=True)
    else:
        archived = prepare_output(output)
        if archived:
            print(f"Archived previous attempt at {archived}", flush=True)

    verdicts: dict[str, str] = {}
    for task in sorted(TASK_ROOT.glob("*.json")):
        for order in ORDERS:
            slug = f"{task.stem}-{order}"
            report = output / slug / "report.json"
            prior = json.loads(report.read_text(encoding="utf-8")) if report.exists() else None
            if args.resume and prior and prior.get("verdict") == "PASS":
                verdict = "PASS"
            else:
                command = [
                    sys.executable, str(DRIVER), "--task", str(task),
                    "--runtime", str(args.runtime), "--runtime-dir", str(args.runtime_dir),
                    "--effort", args.effort, "--order", order,
                    "--output", str(output / slug),
                ]
                if args.model:
                    command.extend(["--model", args.model])
                subprocess.run(command, check=False)
                current = json.loads(report.read_text(encoding="utf-8")) if report.exists() else None
                verdict = current.get("verdict", "MISSING") if current else "MISSING"
            verdicts[slug] = verdict
            (output / "summary.json").write_text(
                json.dumps({"schema": "saccade-public-agent-matrix/1", "verdicts": verdicts}, indent=2) + "\n",
                encoding="utf-8",
            )
            if verdict != "PASS":
                return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
