#!/usr/bin/env python3
"""Run the same-model fair matrix (3 tasks x 2 orders) and print the summary."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DRIVER = ROOT / "scripts/benchmark_same_model_fair.py"
TASKS = ROOT / "benchmarks/tasks"
ORDERS = ("saccade-first", "playwright-first")


def prepare_output(output: Path) -> Path | None:
    """Archive a prior attempt instead of mixing old and new reports."""
    if not output.exists() or not any(output.iterdir()):
        output.mkdir(parents=True, exist_ok=True)
        return None
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    archived = output.with_name(f"{output.name}.previous-{stamp}")
    suffix = 1
    while archived.exists():
        archived = output.with_name(f"{output.name}.previous-{stamp}-{suffix}")
        suffix += 1
    os.replace(output, archived)
    output.mkdir(parents=True)
    return archived


def summarize(output: Path) -> int:
    rows = []
    verdicts = []
    for report in sorted(output.glob("*/report.json")):
        data = json.loads(report.read_text(encoding="utf-8"))
        verdicts.append(data["verdict"])
        print(f"{report.parent.name:34} {data['verdict']:8} "
              f"errors={json.dumps(data['evidence_errors'])}")
        for name, lane in data["lanes"].items():
            metrics = lane.get("browser_metrics") or {}
            usage = lane.get("usage") or {}
            rows.append((report.parent.name, name, lane["passed"], round(lane["elapsed_ms"]),
                         lane["tool_calls"], metrics.get("initial_transfer_bytes"),
                         metrics.get("discovery_view_modes"),
                         metrics.get("action_return_to_delta_read_ms"),
                         metrics.get("post_initial_reobservation_calls"),
                         metrics.get("model_logical_input_tokens"), usage.get("output_tokens")))
    if not rows:
        print("no reports found")
        return 1
    print()
    header = (f"{'run':30} {'lane':10} {'ok':5} {'ms':>7} {'tools':>5} {'disc_bytes':>10} "
              f"{'delta_ms':>9} {'reobs':>5} {'in_tok':>8} {'out_tok':>8}")
    print(header)
    for row in rows:
        print(f"{row[0]:30} {row[1]:10} {str(row[2]):5} {row[3]:7} {row[4]:5} "
              f"{str(row[5]):>10} {str(row[7]):>9} {str(row[8]):>5} "
              f"{str(row[9]):>8} {str(row[10]):>8}")
    print()
    for lane in ("saccade", "playwright"):
        subset = [r for r in rows if r[1] == lane]
        done = sum(1 for r in subset if r[2])
        # Mirror the validator: a non-positive count is missing evidence, not a zero.
        byte_values = [r[5] for r in subset if isinstance(r[5], int) and r[5] > 0]
        token_values = [r[9] for r in subset if isinstance(r[9], int) and r[9] > 0]
        modes = sorted({m for r in subset for m in (r[6] or [])})
        print(f"{lane}: completed {done}/{len(subset)}"
              f" | mean discovery bytes {sum(byte_values)//len(byte_values) if byte_values else 'UNMEASURED'}"
              f" | mean input tokens {sum(token_values)//len(token_values) if token_values else 'UNMEASURED'}"
              f" | view modes chosen {modes or 'n/a'}")
    failed = any(v != "PASS" for v in verdicts)
    if failed:
        print("\nAt least one run is not PASS. No superiority claim is authorized.")
    return 1 if failed else 0


def assert_attached_browser(runtime: Path, runtime_dir: Path, expected: str = "chrome") -> None:
    """Refuse to benchmark against the wrong browser.

    A managed Edge left over from a lifecycle run once held the Native Host
    while the comparison assumed Chrome, and every health check still passed.
    """
    requests = [
        {
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "matrix", "version": "1"}},
        },
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "saccade.system.capabilities", "arguments": {}},
        },
    ]
    probe = subprocess.run(
        [str(runtime), "mcp"],
        input="".join(json.dumps(request) + "\n" for request in requests),
        capture_output=True, text=True, timeout=60,
        env={**os.environ, "SACCADE_RUNTIME_DIR": str(runtime_dir)},
    )
    capabilities = None
    for line in probe.stdout.splitlines():
        try:
            response = json.loads(line)
        except json.JSONDecodeError:
            continue
        if response.get("id") == 2:
            capabilities = response.get("result", {}).get("structuredContent")
            break
    connected = isinstance(capabilities, dict) and capabilities.get("extension_connected") is True
    attached = capabilities.get("attached_browser") if isinstance(capabilities, dict) else None
    if not connected or attached != expected:
        raise SystemExit(
            f"Saccade is not attached to {expected}; refusing to benchmark. "
            "Run ./scripts/dev.sh down then ./scripts/dev.sh attach."
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", help="one model id used identically by both lanes")
    parser.add_argument("--effort", help="one effort level used identically by both lanes")
    parser.add_argument("--browser", choices=("chrome", "edge"), default="chrome",
                        help="browser family used identically by both lanes")
    parser.add_argument("--summarize-only", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    if not args.summarize_only:
        assert_attached_browser(args.runtime.resolve(), args.runtime_dir.resolve(), args.browser)
        archived = prepare_output(output)
        if archived:
            print(f"Archived previous attempt at {archived}", flush=True)
        for task in sorted(TASKS.glob("*.json")):
            for order in ORDERS:
                slug = f"{task.stem}-{order}"
                command = [sys.executable, str(DRIVER), "--task", str(task),
                           "--runtime", str(args.runtime), "--runtime-dir", str(args.runtime_dir),
                           "--order", order, "--browser", args.browser,
                           "--output", str(output / slug)]
                if args.model:
                    command.extend(["--model", args.model])
                if args.effort:
                    command.extend(["--effort", args.effort])
                print(f"=== {slug} ===", flush=True)
                subprocess.run(command, check=False)
    elif not output.exists():
        print(f"output does not exist: {output}")
        return 1
    return summarize(output)


if __name__ == "__main__":
    raise SystemExit(main())
