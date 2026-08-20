#!/usr/bin/env python3
"""Run 3 generated unknown tasks in both lane orders with one model and browser."""

from __future__ import annotations

import argparse
import json
import secrets
import subprocess
import sys
from pathlib import Path

from generate_unknown_benchmark import KINDS, build
from run_same_model_matrix import prepare_output, summarize, assert_attached_browser

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts/benchmark_same_model_fair.py"
ORDERS = ("saccade-first", "playwright-first")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--fixture-root", required=True, type=Path)
    parser.add_argument("--base-url", default="http://127.0.0.1:8765/fixtures/benchmarks")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--effort", required=True)
    args = parser.parse_args()

    assert_attached_browser(args.runtime.resolve(), args.runtime_dir.resolve(), args.browser)
    output = args.output.resolve()
    archived = prepare_output(output)
    if archived:
        print(f"Archived previous attempt at {archived}", flush=True)
    generated = output / "generated"
    generated.mkdir(parents=True, exist_ok=True)
    live = args.fixture_root.resolve() / "fixtures" / "benchmarks"
    live.mkdir(parents=True, exist_ok=True)

    for kind in KINDS:
        # Freeze one task per kind across both lane orders. Otherwise an order
        # effect is confounded with a different generated page and goal.
        seed = secrets.token_hex(12)
        slug = f"unknown-{kind}-{seed[:8]}"
        url = f"{args.base_url.rstrip('/')}/{slug}.html"
        page, task = build(kind, seed, url)
        (live / f"{slug}.html").write_text(page, encoding="utf-8")
        (generated / f"{slug}.html").write_text(page, encoding="utf-8")
        task_path = generated / f"{slug}.json"
        task_path.write_text(json.dumps(task, ensure_ascii=False, indent=2) + "\n",
                             encoding="utf-8")
        for order in ORDERS:
            run_slug = f"{slug}-{order}"
            command = [
                sys.executable, str(DRIVER), "--task", str(task_path),
                "--runtime", str(args.runtime), "--runtime-dir", str(args.runtime_dir),
                "--browser", args.browser, "--model", args.model, "--effort", args.effort,
                "--order", order, "--output", str(output / run_slug),
            ]
            print(f"=== {run_slug} ===", flush=True)
            subprocess.run(command, check=False)
    return summarize(output)


if __name__ == "__main__":
    raise SystemExit(main())
