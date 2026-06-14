#!/usr/bin/env python3
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time

import visual_parity_compare as parity


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]


def main():
    args = parse_args()
    run_dir = (WORKSPACE / "runs" / "webgl_runtime" / f"game_probe_{parity.unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    result = {
        "engine": "saccade-webgl-game-runtime-probe-v0",
        "url": args.url,
        "viewport": {"width": args.width, "height": args.height},
        "wait_sec": args.wait_sec,
        "run_dir": str(run_dir),
        "status": "running",
    }

    try:
        saccade = capture_saccade(args, run_dir)
        chrome = capture_chrome(args, run_dir)
        metrics = compare_gameplay_layer(chrome["screenshot"], saccade["screenshot"], args)
        result.update(
            {
                "status": metrics["route"],
                "summary": metrics["summary"],
                "saccade": saccade,
                "chrome": chrome,
                "metrics": metrics,
            }
        )
        report_path = write_report(run_dir, result)
        print(
            "WEBGL_GAME_PROBE "
            f"route={metrics['route']} "
            f"chrome_edge={metrics['chrome']['edge_ratio']:.6f} "
            f"saccade_edge={metrics['saccade']['edge_ratio']:.6f} "
            f"chrome_sat={metrics['chrome']['saturated_ratio']:.6f} "
            f"saccade_sat={metrics['saccade']['saturated_ratio']:.6f} "
            f"gl_warning={saccade['gl_warning']} "
            f"report={report_path}"
        )
    except Exception as error:
        result.update({"status": "error", "summary": str(error)})
        report_path = write_report(run_dir, result)
        print(f"WEBGL_GAME_PROBE route=error summary={error} report={report_path}")
        return 1
    return 0


def parse_args():
    parser = argparse.ArgumentParser(
        description="Compare a local WebGL game in Saccade versus Chrome and detect missing gameplay layers."
    )
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--wait-sec", type=float, default=3.0)
    parser.add_argument("--timeout-sec", type=float, default=45.0)
    parser.add_argument("--edge-threshold", type=int, default=18)
    parser.add_argument("--saturation-threshold", type=int, default=45)
    parser.add_argument("--min-edge-ratio", type=float, default=0.010)
    parser.add_argument("--min-saturated-ratio", type=float, default=0.0015)
    return parser.parse_args()


def capture_saccade(args, run_dir):
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "saccade-shell",
        "--",
        "browser-session-worker",
        "--url",
        args.url,
        "--width",
        str(args.width),
        "--height",
        str(args.height),
        "--rendering-profile",
        "servo-modern",
    ]
    env = os.environ.copy()
    env["RUST_LOG"] = "error"
    input_text = (
        f'{{"id":1,"method":"ping"}}\n'
        f'{{"id":2,"method":"audit"}}\n'
        f'{{"id":3,"method":"close"}}\n'
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
    (run_dir / "saccade_stdout.log").write_text(stdout)
    (run_dir / "saccade_stderr.log").write_text(stderr)
    if proc.returncode != 0:
        raise RuntimeError(
            f"Saccade worker failed with {proc.returncode}\nstdout={stdout}\nstderr={stderr}"
        )
    audit = json_response_by_id(stdout, 2)
    if not audit or audit.get("ok") is not True:
        raise RuntimeError(f"Saccade worker output did not include an ok audit response\n{stdout}")
    screenshot = audit.get("result", {}).get("visual_health", {}).get("screenshot")
    if not screenshot:
        raise RuntimeError(f"Saccade audit did not produce a screenshot\n{stdout}")
    screenshot_path = pathlib.Path(screenshot)
    if not screenshot_path.is_absolute():
        screenshot_path = WORKSPACE / screenshot_path
    copied = run_dir / "saccade_page.png"
    shutil.copy2(screenshot_path, copied)
    output = stdout + "\n" + stderr
    return {
        "screenshot": str(copied),
        "source_screenshot": str(screenshot_path),
        "response": audit.get("result", {}),
        "gl_warning": "GLD_TEXTURE" in output or "texture unloadable" in output,
        "stdout": str(run_dir / "saccade_stdout.log"),
        "stderr": str(run_dir / "saccade_stderr.log"),
    }


def capture_chrome(args, run_dir):
    chrome_dir = run_dir / "chrome"
    chrome_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(WORKSPACE / "scripts" / "capture_chrome_reference.sh"),
        args.url,
        str(chrome_dir),
        str(args.width),
        str(args.height),
        "--timeout-sec",
        str(args.timeout_sec),
        "--settle-ms",
        str(int(max(0.0, args.wait_sec) * 1000)),
        "--block-mode",
        "none",
    ]
    subprocess.run(cmd, cwd=WORKSPACE, check=True, text=True, capture_output=True)
    screenshot = chrome_dir / "chrome_page.png"
    if not screenshot.exists():
        raise RuntimeError("Chrome reference did not produce a screenshot")
    copied = run_dir / "chrome_page.png"
    shutil.copy2(screenshot, copied)
    return {
        "screenshot": str(copied),
        "manifest": str(chrome_dir / "chrome_reference_manifest.json"),
        "truth": str(chrome_dir / "chrome_truth.json"),
        "network": str(chrome_dir / "chrome_network.json"),
    }


def compare_gameplay_layer(chrome_path, saccade_path, args):
    cw, ch, chrome = parity.read_png_rgb(chrome_path)
    sw, sh, saccade = parity.read_png_rgb(saccade_path)
    width = min(cw, sw)
    height = min(ch, sh)
    roi = {
        "left": 0,
        "top": min(height, max(120, int(height * 0.16))),
        "right": width,
        "bottom": min(height, int(height * 0.92)),
    }
    chrome_metrics = gameplay_metrics(chrome, cw, ch, roi, args)
    saccade_metrics = gameplay_metrics(saccade, sw, sh, roi, args)
    edge_ratio_delta = chrome_metrics["edge_ratio"] - saccade_metrics["edge_ratio"]
    saturated_ratio_delta = chrome_metrics["saturated_ratio"] - saccade_metrics["saturated_ratio"]
    saccade_layer_present = (
        saccade_metrics["edge_ratio"] >= args.min_edge_ratio
        and saccade_metrics["saturated_ratio"] >= args.min_saturated_ratio
    )
    chrome_layer_present = (
        chrome_metrics["edge_ratio"] >= args.min_edge_ratio
        and chrome_metrics["saturated_ratio"] >= args.min_saturated_ratio
    )
    missing = chrome_layer_present and not saccade_layer_present
    severe_delta = (
        chrome_layer_present
        and saccade_metrics["edge_ratio"] < chrome_metrics["edge_ratio"] * 0.55
        and saccade_metrics["saturated_ratio"] < chrome_metrics["saturated_ratio"] * 0.65
    )
    route = "blocked_missing_gameplay_layer" if missing or severe_delta else "green_or_needs_review"
    summary = (
        "Chrome gameplay layer has high-frequency structure that Saccade is missing."
        if route == "blocked_missing_gameplay_layer"
        else "Saccade gameplay-layer metrics are not clearly missing versus Chrome."
    )
    return {
        "route": route,
        "summary": summary,
        "roi": roi,
        "chrome": chrome_metrics,
        "saccade": saccade_metrics,
        "edge_ratio_delta": round(edge_ratio_delta, 6),
        "saturated_ratio_delta": round(saturated_ratio_delta, 6),
        "chrome_layer_present": chrome_layer_present,
        "saccade_layer_present": saccade_layer_present,
        "thresholds": {
            "edge_threshold": args.edge_threshold,
            "saturation_threshold": args.saturation_threshold,
            "min_edge_ratio": args.min_edge_ratio,
            "min_saturated_ratio": args.min_saturated_ratio,
        },
    }


def gameplay_metrics(rgb, width, height, roi, args):
    left = roi["left"]
    top = roi["top"]
    right = roi["right"]
    bottom = roi["bottom"]
    pixels = max(1, (right - left) * (bottom - top))
    saturated = 0
    dark = 0
    edge = 0
    samples = 0
    for y in range(top, bottom):
        for x in range(left, right):
            i = (y * width + x) * 3
            r, g, b = rgb[i], rgb[i + 1], rgb[i + 2]
            if max(r, g, b) - min(r, g, b) >= args.saturation_threshold:
                saturated += 1
            if r + g + b < 210:
                dark += 1
            if x + 1 < right and y + 1 < bottom:
                j = (y * width + x + 1) * 3
                k = ((y + 1) * width + x) * 3
                delta = max(
                    abs(r - rgb[j]),
                    abs(g - rgb[j + 1]),
                    abs(b - rgb[j + 2]),
                    abs(r - rgb[k]),
                    abs(g - rgb[k + 1]),
                    abs(b - rgb[k + 2]),
                )
                if delta >= args.edge_threshold:
                    edge += 1
                samples += 1
    return {
        "edge_ratio": round(edge / max(1, samples), 6),
        "saturated_ratio": round(saturated / pixels, 6),
        "dark_ratio": round(dark / pixels, 6),
        "pixels": pixels,
    }


def json_response_by_id(stdout, response_id):
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if value.get("id") == response_id:
            return value
    return None


def write_report(run_dir, result):
    report = run_dir / "report.json"
    report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return report


if __name__ == "__main__":
    sys.exit(main())
