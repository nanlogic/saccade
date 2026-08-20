#!/usr/bin/env python3
"""Run the generated 1/5/10/25/50 review queues in both lane orders."""

from __future__ import annotations

import argparse
import json
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from generate_long_horizon_benchmark import LENGTHS, MODES, build
from run_same_model_matrix import assert_attached_browser, prepare_output


ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts/benchmark_agent_fair.py"
ORDERS = ("saccade-first", "playwright-first")


def preflight_fixture(url: str) -> None:
    """Fail before model calls unless the freshly copied task is actually served."""
    last_error: Exception | None = None
    for _ in range(3):
        try:
            with urllib.request.urlopen(url, timeout=2) as response:
                if response.status == 200:
                    return
                raise RuntimeError(f"fixture returned HTTP {response.status}")
        except (OSError, urllib.error.URLError, RuntimeError) as error:
            last_error = error
            time.sleep(0.2)
    raise RuntimeError(f"fixture preflight failed for {url}: {last_error}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--fixture-root", required=True, type=Path)
    parser.add_argument("--base-url", default="http://127.0.0.1:8765/fixtures/benchmarks/long")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--browser", choices=("chrome", "edge"), default="chrome")
    parser.add_argument("--model")
    parser.add_argument("--effort", choices=("low", "medium", "high", "xhigh"), default="low")
    parser.add_argument("--resume", action="store_true", help="reuse PASS reports and generated seeds from an interrupted matrix")
    args = parser.parse_args()
    assert_attached_browser(args.runtime.resolve(), args.runtime_dir.resolve(), args.browser)
    output = args.output.resolve()
    if args.resume:
        output.mkdir(parents=True, exist_ok=True)
    else:
        archived = prepare_output(output)
        if archived:
            print(f"Archived previous attempt at {archived}", flush=True)
    live_root = args.fixture_root.resolve() / "fixtures" / "benchmarks" / "long"
    live_root.mkdir(parents=True, exist_ok=True)
    generated = output / "generated"
    generated.mkdir(parents=True, exist_ok=True)
    verdicts: dict[str, str] = {}
    curves: list[dict[str, object]] = []
    for mode in MODES:
        for length in LENGTHS:
            existing_tasks = sorted(generated.glob(f"{mode}-{length}-*/task.json")) if args.resume else []
            if len(existing_tasks) > 1:
                raise RuntimeError(f"resume found multiple generated tasks for {mode}/{length}")
            if existing_tasks:
                task_path = existing_tasks[0]
                archived_pages = task_path.parent
                slug = archived_pages.name
                task = json.loads(task_path.read_text(encoding="utf-8"))
                pages = {
                    page.name: page.read_text(encoding="utf-8")
                    for page in archived_pages.glob("*.html")
                }
            else:
                seed = secrets.token_hex(12)
                slug = f"{mode}-{length}-{seed[:8]}"
                url = f"{args.base_url.rstrip('/')}/{slug}/index.html"
                pages, task = build(seed, length, mode, url)
                archived_pages = generated / slug
                archived_pages.mkdir(parents=True, exist_ok=True)
                for name, page in pages.items():
                    (archived_pages / name).write_text(page, encoding="utf-8")
                task_path = archived_pages / "task.json"
                task_path.write_text(json.dumps(task, indent=2) + "\n", encoding="utf-8")
            live = live_root / slug
            live.mkdir(parents=True, exist_ok=True)
            for name, page in pages.items():
                (live / name).write_text(page, encoding="utf-8")
            preflight_fixture(task["url"])
            for order in ORDERS:
                run_slug = f"{slug}-{order}"
                report = output / run_slug / "report.json"
                prior = json.loads(report.read_text(encoding="utf-8")) if report.exists() else None
                if args.resume and prior and prior.get("verdict") == "PASS":
                    report_value = prior
                else:
                    command = [
                        sys.executable, str(DRIVER), "--task", str(task_path),
                        "--runtime", str(args.runtime), "--runtime-dir", str(args.runtime_dir),
                        "--effort", args.effort, "--order", order,
                        "--output", str(output / run_slug),
                    ]
                    if args.model:
                        command.extend(["--model", args.model])
                    subprocess.run(command, check=False)
                    report_value = json.loads(report.read_text(encoding="utf-8")) if report.exists() else None
                verdicts[run_slug] = report_value["verdict"] if report_value else "MISSING"
                if report_value:
                    for lane, lane_value in report_value["lanes"].items():
                        metrics = lane_value.get("browser_metrics") or {}
                        curves.append({
                            "mode": mode, "length": length, "order": order, "lane": lane,
                            "elapsed_ms": lane_value.get("elapsed_ms"),
                            "input_tokens": (lane_value.get("model_usage") or {}).get("input_tokens"),
                            "non_cached_input_tokens": (lane_value.get("model_usage") or {}).get("non_cached_input_tokens"),
                            "control_plane_bytes": (report_value.get("control_plane", {}).get(lane) or {}).get("combined_mcp_bytes"),
                            "discovery_bytes": (metrics.get("discovery") or {}).get("transfer_bytes"),
                            "steady_state_bytes": (metrics.get("steady_state") or {}).get("transfer_bytes"),
                            "tool_calls": lane_value.get("tool_calls"),
                            "passed": lane_value.get("passed"),
                        })
                if verdicts[run_slug] != "PASS":
                    summary = {"schema":"saccade-long-horizon-matrix/1","verdicts":verdicts,"curves":curves,"stopped_at":run_slug}
                    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
                    print(json.dumps(summary))
                    return 1
    summary = {"schema":"saccade-long-horizon-matrix/1","verdicts":verdicts,"curves":curves,"stopped_at":None}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
