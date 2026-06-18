#!/usr/bin/env python3
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time
import urllib.parse

import probe_webgl_game_runtime as game_probe


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_VARIANTS = [
    "bare-gradient2-size-1152x648",
    "bare-solid-size-1152x648",
]


def main():
    args = parse_args()
    run_dir = (WORKSPACE / "runs" / "webgl_runtime" / f"canvas_screenshot_paths_{unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    fixture = (WORKSPACE / "test_pages" / "canvas_runtime" / "index.html").resolve()
    results = []
    for variant in args.variants:
        results.append(run_variant(args, fixture, run_dir, variant))

    report = {
        "engine": "saccade-canvas-screenshot-paths-v0",
        "fixture": str(fixture),
        "run_dir": str(run_dir),
        "viewport": {"width": args.width, "height": args.height},
        "wait_sec": args.wait_sec,
        "variants": results,
        "summary": summarize(results),
    }
    report_path = run_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        "CANVAS_SCREENSHOT_PATHS "
        f"variants={len(results)} "
        f"errors={report['summary']['errors']} "
        f"manual_blocked={report['summary']['manual_blocked']} "
        f"take_blocked={report['summary']['take_blocked']} "
        f"route={report['summary']['route']} "
        f"report={report_path}"
    )
    return 1 if report["summary"]["errors"] else 0


def parse_args():
    parser = argparse.ArgumentParser(
        description=(
            "Compare Chrome, Saccade manual readback, and Servo WebView::take_screenshot "
            "on local Canvas2D reductions."
        )
    )
    parser.add_argument("--variants", nargs="+", default=DEFAULT_VARIANTS)
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--wait-sec", type=float, default=3.0)
    parser.add_argument("--timeout-sec", type=float, default=75.0)
    parser.add_argument("--edge-threshold", type=int, default=18)
    parser.add_argument("--saturation-threshold", type=int, default=45)
    parser.add_argument("--min-edge-ratio", type=float, default=0.010)
    parser.add_argument("--min-saturated-ratio", type=float, default=0.0015)
    parser.add_argument("--min-smooth-channel-range", type=float, default=10.0)
    parser.add_argument("--min-smooth-luma-range", type=float, default=4.0)
    return parser.parse_args()


def run_variant(args, fixture, run_dir, variant):
    case_dir = run_dir / safe_name(variant)
    case_dir.mkdir(parents=True, exist_ok=True)
    url = fixture_url(fixture, variant)
    summary = {
        "variant": variant,
        "url": url,
        "status": "running",
        "case_dir": str(case_dir),
    }
    try:
        saccade = capture_saccade(args, url, case_dir)
        chrome_args = argparse.Namespace(**vars(args), url=url)
        chrome = game_probe.capture_chrome(chrome_args, case_dir)
        manual = compare_path(args, case_dir, "manual_readback", chrome, saccade["manual"], saccade)
        take = compare_path(args, case_dir, "take_screenshot", chrome, saccade["take_screenshot"], saccade)
        route = diagnose(manual, take, saccade)
        summary.update(
            {
                "status": "ok",
                "route": route,
                "chrome": chrome,
                "saccade": saccade,
                "comparisons": {
                    "manual_readback": manual,
                    "take_screenshot": take,
                },
            }
        )
    except Exception as error:
        summary.update({"status": "error", "summary": str(error)})
    (case_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    return summary


def capture_saccade(args, url, case_dir):
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "saccade-shell",
        "--",
        "browser-session-worker",
        "--url",
        url,
        "--width",
        str(args.width),
        "--height",
        str(args.height),
        "--rendering-profile",
        "servo-modern",
    ]
    env = os.environ.copy()
    env["RUST_LOG"] = "error"
    input_text = "\n".join(
        [
            json.dumps({"id": 1, "method": "ping"}),
            json.dumps({"id": 2, "method": "audit"}),
            json.dumps({"id": 3, "method": "take_screenshot_audit"}),
            json.dumps({"id": 4, "method": "webgl_page_probe"}),
            json.dumps({"id": 5, "method": "close"}),
            "",
        ]
    )
    proc = subprocess.Popen(
        cmd,
        cwd=WORKSPACE,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    time.sleep(max(0.0, args.wait_sec))
    try:
        stdout, stderr = proc.communicate(input=input_text, timeout=args.timeout_sec)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, stderr = proc.communicate(timeout=5)
        raise RuntimeError(f"Saccade worker timed out\nstdout={stdout}\nstderr={stderr}")
    (case_dir / "saccade_stdout.log").write_text(stdout)
    (case_dir / "saccade_stderr.log").write_text(stderr)
    if proc.returncode != 0:
        raise RuntimeError(f"Saccade worker failed with {proc.returncode}\nstdout={stdout}\nstderr={stderr}")

    audit = json_response(stdout, 2, "audit")
    take = json_response(stdout, 3, "take_screenshot_audit")
    page_probe_response = json_response(stdout, 4, "webgl_page_probe")
    page_probe = page_probe_response.get("result", {})
    page_probe_path = case_dir / "saccade_webgl_page_probe.json"
    page_probe_path.write_text(json.dumps(page_probe, indent=2, sort_keys=True) + "\n")
    page_probe_summary = game_probe.summarize_page_probe(page_probe)

    manual_source = resolve_workspace_path(
        audit.get("result", {}).get("visual_health", {}).get("screenshot")
    )
    take_source = resolve_workspace_path(take.get("result", {}).get("screenshot"))
    if not manual_source or not manual_source.exists():
        raise RuntimeError(f"Saccade manual audit screenshot missing: {manual_source}")
    if not take_source or not take_source.exists():
        raise RuntimeError(f"Saccade take_screenshot image missing: {take_source}")

    manual_copy = case_dir / "saccade_manual_readback.png"
    take_copy = case_dir / "saccade_take_screenshot.png"
    shutil.copy2(manual_source, manual_copy)
    shutil.copy2(take_source, take_copy)
    output = stdout + "\n" + stderr
    return {
        "manual": {
            "screenshot": str(manual_copy),
            "source_screenshot": str(manual_source),
            "webgl_page_probe_summary": page_probe_summary,
        },
        "take_screenshot": {
            "screenshot": str(take_copy),
            "source_screenshot": str(take_source),
            "webgl_page_probe_summary": page_probe_summary,
        },
        "webgl_page_probe": str(page_probe_path),
        "webgl_page_probe_summary": page_probe_summary,
        "gl_warning": "GLD_TEXTURE" in output or "texture unloadable" in output,
        "stdout": str(case_dir / "saccade_stdout.log"),
        "stderr": str(case_dir / "saccade_stderr.log"),
        "audit_response": audit.get("result", {}),
        "take_screenshot_response": take.get("result", {}),
    }


def compare_path(args, case_dir, label, chrome, saccade_path, saccade):
    metric_dir = case_dir / f"{label}_metric"
    metric_dir.mkdir(parents=True, exist_ok=True)
    normalized = game_probe.normalize_metric_images(chrome, saccade_path, metric_dir)
    metrics = game_probe.compare_gameplay_layer(
        normalized["chrome_screenshot"],
        normalized["saccade_screenshot"],
        args,
    )
    metrics["normalization"] = normalized
    pixel_probe = (
        (saccade.get("webgl_page_probe_summary") or {})
        .get("largest_canvas", {})
        .get("pixel_probe", {})
    )
    return {
        "label": label,
        "status": metrics["route"],
        "summary": metrics["summary"],
        "metrics": metrics,
        "page_backing_has_foreground": game_probe.page_canvas_has_foreground_signal(pixel_probe, metrics),
    }


def diagnose(manual, take, saccade):
    manual_blocked = manual["status"] == "blocked_missing_gameplay_layer"
    take_blocked = take["status"] == "blocked_missing_gameplay_layer"
    backing_has_signal = manual.get("page_backing_has_foreground") or take.get("page_backing_has_foreground")
    if manual_blocked and not take_blocked:
        return "manual_readback_only"
    if manual_blocked and take_blocked and backing_has_signal:
        return "take_screenshot_and_manual_lose_after_canvas_backing"
    if manual_blocked and take_blocked:
        return "take_screenshot_and_manual_red"
    if not manual_blocked and take_blocked:
        return "take_screenshot_only_red"
    if saccade.get("gl_warning"):
        return "both_green_with_gl_warning"
    return "both_green_or_review"


def summarize(results):
    errors = sum(1 for result in results if result.get("status") == "error")
    manual_blocked = 0
    take_blocked = 0
    routes = {}
    for result in results:
        route = result.get("route") or result.get("status")
        routes[route] = routes.get(route, 0) + 1
        comparisons = result.get("comparisons") or {}
        if comparisons.get("manual_readback", {}).get("status") == "blocked_missing_gameplay_layer":
            manual_blocked += 1
        if comparisons.get("take_screenshot", {}).get("status") == "blocked_missing_gameplay_layer":
            take_blocked += 1
    dominant_route = max(routes.items(), key=lambda item: item[1])[0] if routes else "none"
    return {
        "errors": errors,
        "manual_blocked": manual_blocked,
        "take_blocked": take_blocked,
        "routes": routes,
        "route": dominant_route,
    }


def json_response(stdout, response_id, label):
    response = game_probe.json_response_by_id(stdout, response_id)
    if not response or response.get("ok") is not True:
        raise RuntimeError(f"Saccade worker output did not include ok {label} response\n{stdout}")
    return response


def resolve_workspace_path(value):
    if not value:
        return None
    path = pathlib.Path(str(value))
    if path.is_absolute():
        return path
    return WORKSPACE / path


def fixture_url(path, variant):
    query = urllib.parse.urlencode({"variant": variant})
    return f"{path.as_uri()}?{query}"


def safe_name(value):
    return "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in value)


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    sys.exit(main())
