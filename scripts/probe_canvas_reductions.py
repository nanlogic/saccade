#!/usr/bin/env python3
import argparse
import json
import pathlib
import subprocess
import sys
import time
import urllib.parse


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_VARIANTS = ["static", "dpr", "animated", "hud"]
SIZING_VARIANTS = [
    "static",
    "small-static",
    "small-dpr",
    "small-attribute",
    "alpha-false",
    "dom-background",
    "dpr-no-transform",
]
THRESHOLD_VARIANTS = [
    "small-static",
    "size-960x540",
    "size-1152x648",
    "size-1280x720",
    "static",
    "dpr-size-360x210",
    "small-dpr",
]
BARE_THRESHOLD_VARIANTS = [
    "bare-size-960x540",
    "bare-size-1024x576",
    "bare-size-1088x612",
    "bare-size-1152x648",
    "dpr-bare-size-360x210",
    "dpr-bare-size-480x270",
]
VARIANT_PRESETS = {
    "base": DEFAULT_VARIANTS,
    "sizing": SIZING_VARIANTS,
    "threshold": THRESHOLD_VARIANTS,
    "threshold-bare": BARE_THRESHOLD_VARIANTS,
    "all": (
        DEFAULT_VARIANTS
        + [variant for variant in SIZING_VARIANTS if variant not in DEFAULT_VARIANTS]
        + [
            variant
            for variant in THRESHOLD_VARIANTS
            if variant not in DEFAULT_VARIANTS + SIZING_VARIANTS
        ]
        + [
            variant
            for variant in BARE_THRESHOLD_VARIANTS
            if variant not in DEFAULT_VARIANTS + SIZING_VARIANTS + THRESHOLD_VARIANTS
        ]
    ),
}


def main():
    args = parse_args()
    variants = args.variants or VARIANT_PRESETS[args.preset]
    run_dir = (WORKSPACE / "runs" / "webgl_runtime" / f"canvas_reductions_{unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    fixture = (WORKSPACE / "test_pages" / "canvas_runtime" / "index.html").resolve()
    results = []
    for variant in variants:
        for iteration in range(args.repeat):
            results.append(run_variant(args, fixture, run_dir, variant, iteration))

    aggregate = {
        "engine": "saccade-canvas-reductions-v0",
        "fixture": str(fixture),
        "repeat": args.repeat,
        "run_dir": str(run_dir),
        "variant_count": len(results),
        "variants": results,
        "summary": summarize(results),
    }
    report = run_dir / "report.json"
    report.write_text(json.dumps(aggregate, indent=2, sort_keys=True) + "\n")
    print(
        "CANVAS_REDUCTIONS "
        f"variants={len(results)} "
        f"blocked={aggregate['summary']['blocked']} "
        f"green_or_review={aggregate['summary']['green_or_review']} "
        f"errors={aggregate['summary']['errors']} "
        f"report={report}"
    )
    return 1 if aggregate["summary"]["errors"] else 0


def parse_args():
    parser = argparse.ArgumentParser(
        description="Run Chrome-vs-Saccade probes over local Canvas2D runtime reductions."
    )
    parser.add_argument("--preset", choices=sorted(VARIANT_PRESETS), default="base")
    parser.add_argument("--variants", nargs="+")
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--wait-sec", type=float, default=3.0)
    parser.add_argument("--timeout-sec", type=float, default=75.0)
    parser.add_argument("--repeat", type=int, default=1)
    args = parser.parse_args()
    if args.repeat < 1:
        parser.error("--repeat must be >= 1")
    return args


def run_variant(args, fixture, run_dir, variant, iteration):
    url = fixture_url(fixture, variant)
    variant_dir = run_dir / variant if args.repeat == 1 else run_dir / f"{variant}_{iteration + 1:02d}"
    cmd = [
        sys.executable,
        str(WORKSPACE / "scripts" / "probe_webgl_game_runtime.py"),
        "--url",
        url,
        "--wait-sec",
        str(args.wait_sec),
        "--timeout-sec",
        str(args.timeout_sec),
        "--width",
        str(args.width),
        "--height",
        str(args.height),
    ]
    output = subprocess.run(
        cmd,
        cwd=WORKSPACE,
        text=True,
        capture_output=True,
        timeout=args.timeout_sec + 45,
    )
    summary = {
        "variant": variant,
        "iteration": iteration + 1,
        "url": url,
        "returncode": output.returncode,
        "stdout": output.stdout.strip(),
        "stderr": output.stderr.strip(),
        "status": "error",
    }
    report_path = report_path_from_stdout(output.stdout)
    if report_path:
        summary["report"] = report_path
        report = json.loads(pathlib.Path(report_path).read_text())
        summary["status"] = report.get("status", "unknown")
        summary["diagnosis"] = report.get("diagnosis", {}).get("route")
        summary["gl_warning"] = report.get("saccade", {}).get("gl_warning")
        summary["metrics"] = {
            "chrome_edge": report.get("metrics", {}).get("chrome", {}).get("edge_ratio"),
            "saccade_edge": report.get("metrics", {}).get("saccade", {}).get("edge_ratio"),
            "chrome_sat": report.get("metrics", {}).get("chrome", {}).get("saturated_ratio"),
            "saccade_sat": report.get("metrics", {}).get("saccade", {}).get("saturated_ratio"),
        }
        diagnosis = report.get("diagnosis", {})
        summary["canvas"] = {
            "chrome": largest_canvas_summary(diagnosis.get("chrome_page_probe_summary")),
            "saccade": largest_canvas_summary(diagnosis.get("saccade_page_probe_summary")),
        }
    if output.returncode != 0:
        summary["status"] = "error"
    variant_dir.mkdir(parents=True, exist_ok=True)
    (variant_dir / "stdout.log").write_text(output.stdout)
    (variant_dir / "stderr.log").write_text(output.stderr)
    return summary


def fixture_url(path, variant):
    query = urllib.parse.urlencode({"variant": variant})
    return f"{path.as_uri()}?{query}"


def report_path_from_stdout(stdout):
    for token in stdout.split():
        if token.startswith("report="):
            return token.split("=", 1)[1]
    return None


def summarize(results):
    blocked = sum(1 for result in results if result.get("status") == "blocked_missing_gameplay_layer")
    green_or_review = sum(1 for result in results if result.get("status") == "green_or_needs_review")
    errors = sum(1 for result in results if result.get("status") == "error")
    return {
        "blocked": blocked,
        "green_or_review": green_or_review,
        "errors": errors,
    }


def largest_canvas_summary(page_probe_summary):
    if not isinstance(page_probe_summary, dict):
        return None
    largest = page_probe_summary.get("largest_canvas")
    if not isinstance(largest, dict):
        return None
    rect = largest.get("rect") or {}
    backing = largest.get("backing") or {}
    return {
        "rect": {
            "width": rect.get("width"),
            "height": rect.get("height"),
        },
        "backing": {
            "width": backing.get("width"),
            "height": backing.get("height"),
        },
    }


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    sys.exit(main())
