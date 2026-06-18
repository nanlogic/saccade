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
        "saccade_screenshot_mode": args.saccade_screenshot_mode,
        "run_dir": str(run_dir),
        "status": "running",
    }

    try:
        saccade = capture_saccade(args, run_dir)
        chrome = capture_chrome(args, run_dir)
        metric_images = normalize_metric_images(chrome, saccade, run_dir)
        metrics = compare_gameplay_layer(
            metric_images["chrome_screenshot"],
            metric_images["saccade_screenshot"],
            args,
        )
        metrics["normalization"] = metric_images
        diagnosis = diagnose_game_probe(chrome, saccade, metrics)
        result.update(
            {
                "status": metrics["route"],
                "summary": metrics["summary"],
                "saccade": saccade,
                "chrome": chrome,
                "metrics": metrics,
                "diagnosis": diagnosis,
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
            f"saccade_screenshot_method={saccade['screenshot_method']} "
            f"gl_warning={saccade['gl_warning']} "
            f"diagnosis={diagnosis['route']} "
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
    parser.add_argument("--min-smooth-channel-range", type=float, default=10.0)
    parser.add_argument("--min-smooth-luma-range", type=float, default=4.0)
    parser.add_argument(
        "--saccade-screenshot-mode",
        choices=["take-local", "take", "manual"],
        default="take-local",
        help=(
            "Saccade screenshot source for metric comparison. take-local uses "
            "Servo WebView::take_screenshot() only for file/localhost URLs and "
            "falls back to the existing manual audit path elsewhere."
        ),
    )
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
    screenshot_method = resolve_saccade_screenshot_method(args.saccade_screenshot_mode, args.url)
    screenshot_request = "take_screenshot_audit" if screenshot_method == "take_screenshot" else "audit"
    input_text = (
        f'{{"id":1,"method":"ping"}}\n'
        f'{{"id":2,"method":"{screenshot_request}"}}\n'
        f'{{"id":3,"method":"webgl_page_probe"}}\n'
        f'{{"id":4,"method":"close"}}\n'
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
    screenshot_response = json_response_by_id(stdout, 2)
    if not screenshot_response or screenshot_response.get("ok") is not True:
        raise RuntimeError(
            f"Saccade worker output did not include an ok {screenshot_request} response\n{stdout}"
        )

    page_probe_response = json_response_by_id(stdout, 3)
    if not page_probe_response or page_probe_response.get("ok") is not True:
        raise RuntimeError(f"Saccade worker output did not include an ok WebGL page probe\n{stdout}")
    saccade_page_probe = page_probe_response.get("result", {})
    saccade_page_probe_path = run_dir / "saccade_webgl_page_probe.json"
    saccade_page_probe_path.write_text(json.dumps(saccade_page_probe, indent=2, sort_keys=True) + "\n")

    screenshot = screenshot_path_from_response(screenshot_response, screenshot_method)
    if not screenshot:
        raise RuntimeError(f"Saccade {screenshot_request} did not produce a screenshot\n{stdout}")
    screenshot_path = pathlib.Path(screenshot)
    if not screenshot_path.is_absolute():
        screenshot_path = WORKSPACE / screenshot_path
    copied = run_dir / "saccade_page.png"
    shutil.copy2(screenshot_path, copied)
    output = stdout + "\n" + stderr
    return {
        "screenshot": str(copied),
        "source_screenshot": str(screenshot_path),
        "screenshot_method": screenshot_method,
        "screenshot_request": screenshot_request,
        "response": screenshot_response.get("result", {}),
        "webgl_page_probe": str(saccade_page_probe_path),
        "webgl_page_probe_summary": summarize_page_probe(saccade_page_probe),
        "gl_warning": "GLD_TEXTURE" in output or "texture unloadable" in output,
        "stdout": str(run_dir / "saccade_stdout.log"),
        "stderr": str(run_dir / "saccade_stderr.log"),
    }


def resolve_saccade_screenshot_method(mode, url):
    if mode == "manual":
        return "manual_readback"
    if mode == "take":
        return "take_screenshot"
    return "take_screenshot" if is_local_diagnostic_url(url) else "manual_readback"


def is_local_diagnostic_url(url):
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme == "file":
        return True
    return parsed.scheme in ("http", "https") and parsed.hostname in {
        "127.0.0.1",
        "localhost",
        "::1",
    }


def screenshot_path_from_response(response, method):
    result = response.get("result", {})
    if method == "take_screenshot":
        return result.get("screenshot")
    return result.get("visual_health", {}).get("screenshot")


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
        "--webgl-page-probe",
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
        "webgl_page_probe": str(chrome_dir / "chrome_webgl_page_probe.json"),
        "webgl_page_probe_summary": summarize_page_probe_path(chrome_dir / "chrome_webgl_page_probe.json"),
    }


def summarize_page_probe_path(path):
    probe_path = pathlib.Path(path)
    if not probe_path.exists():
        return {"status": "missing", "path": str(probe_path)}
    return summarize_page_probe(json.loads(probe_path.read_text()))


def summarize_page_probe(response):
    page_probe = response.get("page_probe") if isinstance(response, dict) else None
    if not isinstance(page_probe, dict):
        page_probe = response if isinstance(response, dict) else {}
    canvases = page_probe.get("canvases") or []
    visible_canvases = [canvas for canvas in canvases if canvas.get("visible")]
    webgl_canvases = [
        canvas
        for canvas in canvases
        if ((canvas.get("context") or {}).get("type") in ("webgl", "webgl2"))
    ]
    largest_canvas = None
    for canvas in canvases:
        rect = canvas.get("rect") or {}
        area = float(rect.get("width") or 0) * float(rect.get("height") or 0)
        if largest_canvas is None or area > largest_canvas["area"]:
            largest_canvas = {
                "area": round(area, 2),
                "label": canvas.get("label", ""),
                "rect": rect,
                "backing": canvas.get("backing") or {},
                "context_type": (canvas.get("context") or {}).get("type", "unknown"),
                "pixel_probe": canvas.get("pixelProbe") or {},
            }
    return {
        "status": "ok" if page_probe.get("ok") else "unknown",
        "viewport": page_probe.get("viewport") or {},
        "canvas_count": len(canvases),
        "visible_canvas_count": len(visible_canvases),
        "webgl_canvas_count": len(webgl_canvases),
        "largest_canvas": largest_canvas,
        "visible_layer_count": len(page_probe.get("visibleLayers") or []),
    }


def diagnose_game_probe(chrome, saccade, metrics):
    chrome_summary = chrome.get("webgl_page_probe_summary") or {}
    saccade_summary = saccade.get("webgl_page_probe_summary") or {}
    route = "no_clear_dom_diagnosis"
    reasons = []
    if metrics.get("route") != "blocked_missing_gameplay_layer":
        route = "pixels_not_missing"
        reasons.append("pixel gate did not classify the gameplay layer as missing")
    elif chrome_summary.get("visible_canvas_count", 0) > 0 and saccade_summary.get("visible_canvas_count", 0) == 0:
        route = "dom_or_script_not_ready"
        reasons.append("Chrome has visible canvas nodes while Saccade reports none")
    elif chrome_summary.get("visible_canvas_count", 0) > 0 and saccade_summary.get("visible_canvas_count", 0) > 0:
        route = "render_pipeline_after_dom_ready"
        reasons.append("both engines report visible canvas nodes, but Saccade gameplay pixels are missing")
        saccade_pixel_probe = (saccade_summary.get("largest_canvas") or {}).get("pixel_probe") or {}
        if page_canvas_has_foreground_signal(saccade_pixel_probe, metrics):
            route = "screenshot_readback_after_canvas_backing"
            reasons.append("Saccade page canvas backing has foreground-like pixels, but screenshot readback loses them")
    if saccade.get("gl_warning"):
        reasons.append("Saccade emitted GL texture warning")
    return {
        "route": route,
        "reasons": reasons,
        "chrome_page_probe_summary": chrome_summary,
        "saccade_page_probe_summary": saccade_summary,
    }


def page_canvas_has_foreground_signal(pixel_probe, metrics):
    if not isinstance(pixel_probe, dict) or pixel_probe.get("status") != "ok":
        return False
    thresholds = metrics.get("thresholds") or {}
    min_edge = float(thresholds.get("min_edge_ratio") or 0.010)
    min_sat = float(thresholds.get("min_saturated_ratio") or 0.0015)
    return (
        float(pixel_probe.get("edgeRatio") or 0.0) >= min_edge
        and float(pixel_probe.get("saturatedRatio") or 0.0) >= min_sat
    )


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
    chrome_smooth_layer_present = (
        chrome_metrics["max_channel_range"] >= args.min_smooth_channel_range
        and chrome_metrics["luma_range"] >= args.min_smooth_luma_range
    )
    saccade_smooth_layer_present = (
        saccade_metrics["max_channel_range"] >= args.min_smooth_channel_range
        and saccade_metrics["luma_range"] >= args.min_smooth_luma_range
    )
    missing = chrome_layer_present and not saccade_layer_present
    smooth_missing = (
        not chrome_layer_present
        and chrome_smooth_layer_present
        and not saccade_smooth_layer_present
    )
    severe_delta = (
        chrome_layer_present
        and saccade_metrics["edge_ratio"] < chrome_metrics["edge_ratio"] * 0.55
        and saccade_metrics["saturated_ratio"] < chrome_metrics["saturated_ratio"] * 0.65
    )
    severe_smooth_delta = (
        not chrome_layer_present
        and chrome_smooth_layer_present
        and saccade_metrics["max_channel_range"] < chrome_metrics["max_channel_range"] * 0.35
        and saccade_metrics["luma_range"] < chrome_metrics["luma_range"] * 0.45
    )
    route = (
        "blocked_missing_gameplay_layer"
        if missing or severe_delta or smooth_missing or severe_smooth_delta
        else "green_or_needs_review"
    )
    summary = (
        "Chrome gameplay layer has high-frequency structure that Saccade is missing."
        if missing or severe_delta
        else "Chrome smooth gradient has color variation that Saccade is missing."
        if smooth_missing or severe_smooth_delta
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
        "chrome_smooth_layer_present": chrome_smooth_layer_present,
        "saccade_smooth_layer_present": saccade_smooth_layer_present,
        "thresholds": {
            "edge_threshold": args.edge_threshold,
            "saturation_threshold": args.saturation_threshold,
            "min_edge_ratio": args.min_edge_ratio,
            "min_saturated_ratio": args.min_saturated_ratio,
            "min_smooth_channel_range": args.min_smooth_channel_range,
            "min_smooth_luma_range": args.min_smooth_luma_range,
        },
    }


def normalize_metric_images(chrome, saccade, run_dir):
    chrome_summary = chrome.get("webgl_page_probe_summary") or {}
    saccade_summary = saccade.get("webgl_page_probe_summary") or {}
    chrome_viewport = chrome_summary.get("viewport") or {}
    saccade_viewport = saccade_summary.get("viewport") or {}
    common_width = int(
        min(
            positive_number(chrome_viewport.get("width")),
            positive_number(saccade_viewport.get("width")),
        )
    )
    common_height = int(
        min(
            positive_number(chrome_viewport.get("height")),
            positive_number(saccade_viewport.get("height")),
        )
    )
    if common_width <= 0 or common_height <= 0:
        return {
            "applied": False,
            "reason": "missing_viewport_probe",
            "chrome_screenshot": chrome["screenshot"],
            "saccade_screenshot": saccade["screenshot"],
        }

    chrome_metric = run_dir / "chrome_page_metric.png"
    saccade_metric = run_dir / "saccade_page_metric.png"
    write_css_metric_image(chrome["screenshot"], chrome_metric, chrome_viewport, common_width, common_height)
    write_css_metric_image(
        saccade["screenshot"],
        saccade_metric,
        saccade_viewport,
        common_width,
        common_height,
    )
    return {
        "applied": True,
        "mode": "css_viewport_common_crop",
        "common_css": {"width": common_width, "height": common_height},
        "chrome_viewport": chrome_viewport,
        "saccade_viewport": saccade_viewport,
        "chrome_screenshot": str(chrome_metric),
        "saccade_screenshot": str(saccade_metric),
    }


def positive_number(value):
    try:
        return max(0.0, float(value))
    except (TypeError, ValueError):
        return 0.0


def write_css_metric_image(source_path, target_path, viewport, common_width, common_height):
    source_width, source_height, rgb = parity.read_png_rgb(source_path)
    viewport_width = max(1, int(round(positive_number(viewport.get("width")))))
    viewport_height = max(1, int(round(positive_number(viewport.get("height")))))
    css_rgb = resize_source_to_css(rgb, source_width, source_height, viewport_width, viewport_height)
    cropped = crop_rgb(css_rgb, viewport_width, viewport_height, common_width, common_height)
    parity.write_png_rgb(target_path, common_width, common_height, cropped)


def resize_source_to_css(rgb, source_width, source_height, css_width, css_height):
    if source_width == css_width and source_height == css_height:
        return bytes(rgb)
    return parity.resize_rgb_nearest(rgb, source_width, source_height, css_width, css_height)


def crop_rgb(rgb, source_width, source_height, target_width, target_height):
    target_width = min(source_width, target_width)
    target_height = min(source_height, target_height)
    output = bytearray(target_width * target_height * 3)
    for y in range(target_height):
        source_i = y * source_width * 3
        target_i = y * target_width * 3
        output[target_i : target_i + target_width * 3] = rgb[source_i : source_i + target_width * 3]
    return bytes(output)


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
    min_r = 255
    min_g = 255
    min_b = 255
    max_r = 0
    max_g = 0
    max_b = 0
    min_luma = 255.0
    max_luma = 0.0
    sum_luma = 0.0
    sum_luma_sq = 0.0
    for y in range(top, bottom):
        for x in range(left, right):
            i = (y * width + x) * 3
            r, g, b = rgb[i], rgb[i + 1], rgb[i + 2]
            min_r = min(min_r, r)
            min_g = min(min_g, g)
            min_b = min(min_b, b)
            max_r = max(max_r, r)
            max_g = max(max_g, g)
            max_b = max(max_b, b)
            luma = (r + g + b) / 3.0
            min_luma = min(min_luma, luma)
            max_luma = max(max_luma, luma)
            sum_luma += luma
            sum_luma_sq += luma * luma
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
    luma_mean = sum_luma / pixels
    luma_variance = max(0.0, (sum_luma_sq / pixels) - (luma_mean * luma_mean))
    channel_ranges = [max_r - min_r, max_g - min_g, max_b - min_b]
    return {
        "edge_ratio": round(edge / max(1, samples), 6),
        "saturated_ratio": round(saturated / pixels, 6),
        "dark_ratio": round(dark / pixels, 6),
        "max_channel_range": round(max(channel_ranges), 6),
        "luma_range": round(max_luma - min_luma, 6),
        "luma_stdev": round(luma_variance**0.5, 6),
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
