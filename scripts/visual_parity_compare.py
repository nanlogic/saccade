#!/usr/bin/env python3
import argparse
import html
import json
import math
import os
import pathlib
import shutil
import struct
import subprocess
import sys
import time
import zlib


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
FIXTURE_ROOT = WORKSPACE / "test_pages" / "visual_parity"
DEFAULT_FIXTURES = [
    "layout_probe",
    "dashboard",
    "form_controls",
    "modal_overlay",
    "scroll_sticky",
    "canvas_svg",
    "responsive_cards",
]
ACTION_CLICK_ESCAPE_FAIL_PX = 8
ACTION_RECT_WARNING_PX = 24
LAYOUT_RECT_FAIL_PX = 8


def main():
    args = parse_args()
    fixtures = DEFAULT_FIXTURES if args.fixtures == ["all"] else args.fixtures
    run_dir = (WORKSPACE / "runs" / "visual_parity" / f"parity_{unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    results = []
    for fixture in fixtures:
        fixture_dir = FIXTURE_ROOT / fixture
        index = fixture_dir / "index.html"
        if not index.exists():
            raise SystemExit(f"unknown visual parity fixture: {fixture}")
        case_dir = run_dir / fixture
        case_dir.mkdir(parents=True, exist_ok=True)
        url = index.resolve().as_uri()
        print(f"VISUAL PARITY fixture={fixture} url={url}")
        result = run_case(fixture, url, case_dir, args)
        results.append(result)

    manifest = {
        "engine": "saccade-visual-parity-v0",
        "created_at_unix_ms": unix_ms(),
        "viewport": {"width": args.width, "height": args.height, "device_scale_factor": 1},
        "rendering_profile": resolved_rendering_profile(args),
        "saccade_grid": args.saccade_grid,
        "fixtures": results,
    }
    (run_dir / "visual_parity_manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    write_html(run_dir, results, args)
    print(f"VISUAL PARITY PASS fixtures={len(results)} report={run_dir / 'index.html'}")


def parse_args():
    parser = argparse.ArgumentParser(
        description="Capture Chrome and Saccade screenshots for local fixtures and compare pixels."
    )
    parser.add_argument(
        "fixtures",
        nargs="*",
        default=["all"],
        help="Fixture names under test_pages/visual_parity, or all.",
    )
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--timeout-sec", type=float, default=45)
    parser.add_argument(
        "--diff-threshold",
        type=int,
        default=24,
        help="Per-channel pixel threshold for diff ratio.",
    )
    parser.add_argument(
        "--rendering-profile",
        choices=("servo-safe", "servo-modern", "chrome-reference"),
        default=None,
        help="Saccade rendering profile for the worker.",
    )
    parser.add_argument(
        "--saccade-grid",
        choices=("default", "off", "on"),
        default="default",
        help="Legacy Grid override for the Saccade worker.",
    )
    return parser.parse_args()


def resolved_rendering_profile(args):
    if args.rendering_profile:
        return args.rendering_profile
    if args.saccade_grid == "on":
        return "servo-modern"
    return "servo-safe"


def run_case(fixture, url, case_dir, args):
    saccade = capture_saccade(
        url,
        case_dir,
        args.timeout_sec,
        resolved_rendering_profile(args),
        args.saccade_grid,
    )
    saccade_truth = saccade.get("result", {})
    (case_dir / "saccade_worker_result.json").write_text(
        json.dumps(saccade_truth, indent=2, sort_keys=True) + "\n"
    )
    saccade_actions_for_chrome = case_dir / "saccade_actions_for_chrome.json"
    verifiable_actions = verifiable_saccade_actions(saccade_truth.get("actions", []))
    saccade_actions_for_chrome.write_text(
        json.dumps(verifiable_actions, indent=2, sort_keys=True) + "\n"
    )
    saccade_src = pathlib.Path(saccade["screenshot"])
    if not saccade_src.is_absolute():
        saccade_src = WORKSPACE / saccade_src
    saccade_png = case_dir / "saccade_page.png"
    shutil.copy2(saccade_src, saccade_png)

    chrome_dir = case_dir / "chrome"
    chrome_dir.mkdir(parents=True, exist_ok=True)
    chrome_cmd = [
        str(WORKSPACE / "scripts" / "capture_chrome_reference.sh"),
        url,
        str(chrome_dir),
        str(args.width),
        str(args.height),
        "--timeout-sec",
        str(args.timeout_sec),
        "--verify-actions-file",
        str(saccade_actions_for_chrome),
    ]
    run_with_retry(chrome_cmd, args.timeout_sec + 10, attempts=2)
    chrome_src = chrome_dir / "chrome_page.png"
    chrome_png = case_dir / "chrome_page.png"
    shutil.copy2(chrome_src, chrome_png)

    metrics, diff_rgb = compare_pngs(chrome_png, saccade_png, args.diff_threshold)
    diff_png = case_dir / "diff.png"
    write_png_rgb(diff_png, metrics["width"], metrics["height"], diff_rgb)

    chrome_truth = json.loads((chrome_dir / "chrome_truth.json").read_text())
    chrome_click_verification = json.loads(
        (chrome_dir / "chrome_click_verification.json").read_text()
    )
    chrome_click_verification["candidate_policy"] = {
        "scope": "enabled_non_sensitive_saccade_actions",
        "saccade_actions_total": len(saccade_truth.get("actions", [])),
        "saccade_actions_verified": len(verifiable_actions),
        "saccade_actions_skipped": len(saccade_truth.get("actions", [])) - len(verifiable_actions),
    }
    layout_probe_metrics = compare_layout_probes(chrome_truth, saccade_truth)
    action_map_metrics = compare_action_maps(chrome_truth, saccade_truth)
    actions_delta = abs(
        len(chrome_truth.get("actions", [])) - len(saccade_truth.get("actions", []))
    )
    result = {
        "fixture": fixture,
        "url": url,
        "case_dir": str(case_dir),
        "chrome_screenshot": rel(run_dir=case_dir.parent, path=chrome_png),
        "saccade_screenshot": rel(run_dir=case_dir.parent, path=saccade_png),
        "diff_image": rel(run_dir=case_dir.parent, path=diff_png),
        "chrome_actions": len(chrome_truth.get("actions", [])),
        "saccade_actions": len(saccade_truth.get("actions", [])),
        "actions_delta": actions_delta,
        "chrome_title": chrome_truth.get("title", ""),
        "saccade_title": saccade_truth.get("title", ""),
        "metrics": metrics,
        "layout_probe_metrics": layout_probe_metrics,
        "action_map_metrics": action_map_metrics,
        "chrome_click_verification": chrome_click_verification,
        "artifacts": {
            "chrome_manifest": str(chrome_dir / "chrome_reference_manifest.json"),
            "chrome_truth": str(chrome_dir / "chrome_truth.json"),
            "chrome_network": str(chrome_dir / "chrome_network.json"),
            "chrome_click_verification": str(chrome_dir / "chrome_click_verification.json"),
            "saccade_actions_for_chrome": str(saccade_actions_for_chrome),
            "saccade_worker_result": str(case_dir / "saccade_worker_result.json"),
        },
    }
    result["diff_classification"] = classify_diff(result)
    (case_dir / "case_result.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def verifiable_saccade_actions(actions):
    return [
        action
        for action in actions
        if action.get("enabled") is True
        and action.get("sensitivity", {}).get("kind", "none") == "none"
    ]


def capture_saccade(url, case_dir, timeout, rendering_profile, saccade_grid):
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
        "--rendering-profile",
        rendering_profile,
    ]
    env = os.environ.copy()
    env["RUST_LOG"] = "error"
    if saccade_grid == "on":
        env["SACCADE_SERVO_GRID"] = "1"
    elif saccade_grid == "off":
        env["SACCADE_SERVO_GRID"] = "0"
    input_text = '{"id":1,"method":"audit"}\n{"id":2,"method":"close"}\n'
    proc = subprocess.Popen(
        cmd,
        cwd=WORKSPACE,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        stdout, _ = proc.communicate(input=input_text, timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, _ = proc.communicate(timeout=5)
        raise RuntimeError(f"Saccade worker timed out for {url}\n{stdout}")
    (case_dir / "saccade_worker_stdout.log").write_text(stdout)
    if proc.returncode != 0:
        raise RuntimeError(f"Saccade worker failed for {url}\n{stdout}")

    audit_response = None
    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if value.get("id") == 1 and value.get("ok") is True:
            audit_response = value
            break
    if not audit_response:
        raise RuntimeError(f"Saccade worker output did not include audit response\n{stdout}")
    result = audit_response["result"]
    screenshot = result.get("visual_health", {}).get("screenshot")
    if not screenshot:
        raise RuntimeError(f"Saccade audit did not produce a screenshot for {url}")
    return {"screenshot": screenshot, "result": result}


def compare_layout_probes(chrome_truth, saccade_truth):
    chrome = {probe.get("name", ""): probe for probe in extract_layout_probes(chrome_truth)}
    saccade = {probe.get("name", ""): probe for probe in extract_layout_probes(saccade_truth)}
    names = sorted(name for name in set(chrome) | set(saccade) if name)
    items = []
    max_rect_delta = 0.0
    display_mismatches = 0
    grid_template_mismatches = 0
    missing = 0
    for name in names:
        c = chrome.get(name)
        s = saccade.get(name)
        if not c or not s:
            missing += 1
            items.append({"name": name, "missing": "chrome" if not c else "saccade"})
            continue
        rect_delta = rect_max_delta(c.get("rect", {}), s.get("rect", {}))
        max_rect_delta = max(max_rect_delta, rect_delta)
        display_match = c.get("display") == s.get("display")
        grid_match = c.get("gridTemplateColumns") == s.get("gridTemplateColumns")
        if not display_match:
            display_mismatches += 1
        if not grid_match:
            grid_template_mismatches += 1
        items.append(
            {
                "name": name,
                "rect_max_delta": round(rect_delta, 3),
                "chrome_display": c.get("display", ""),
                "saccade_display": s.get("display", ""),
                "chrome_grid_template_columns": c.get("gridTemplateColumns", ""),
                "saccade_grid_template_columns": s.get("gridTemplateColumns", ""),
                "chrome_rect": c.get("rect", {}),
                "saccade_rect": s.get("rect", {}),
            }
        )
    return {
        "probe_count": len(names),
        "missing": missing,
        "max_rect_delta": round(max_rect_delta, 3),
        "display_mismatches": display_mismatches,
        "grid_template_mismatches": grid_template_mismatches,
        "items": items,
    }


def extract_layout_probes(truth):
    if not isinstance(truth, dict):
        return []
    direct = truth.get("layoutProbes")
    if isinstance(direct, list):
        return direct
    nested = truth.get("truth", {}).get("layout_probes")
    if isinstance(nested, list):
        return nested
    return []


def rect_max_delta(a, b):
    return max(
        abs(float(a.get(key, 0) or 0) - float(b.get(key, 0) or 0))
        for key in ("left", "top", "width", "height")
    )


def compare_action_maps(chrome_truth, saccade_truth):
    chrome_actions = chrome_truth.get("actions", [])
    saccade_actions = saccade_truth.get("actions", [])
    chrome_sorted = sorted(chrome_actions, key=action_sort_key)
    saccade_sorted = sorted(saccade_actions, key=action_sort_key)
    chrome_labels = [action_label(action) for action in chrome_sorted]
    saccade_labels = [action_label(action) for action in saccade_sorted]
    max_rect_delta = 0.0
    max_center_delta = 0.0
    max_click_escape_delta = 0.0
    matched_items = []
    if chrome_labels == saccade_labels:
        for chrome_action, saccade_action in zip(chrome_sorted, saccade_sorted):
            rect_delta = rect_max_delta(
                chrome_action.get("rect", {}),
                saccade_action.get("rect", {}),
            )
            center_delta = center_distance(
                chrome_action.get("rect", {}),
                saccade_action.get("rect", {}),
            )
            click_escape_delta = point_escape_distance(
                rect_center(saccade_action.get("rect", {})),
                chrome_action.get("rect", {}),
            )
            max_rect_delta = max(max_rect_delta, rect_delta)
            max_center_delta = max(max_center_delta, center_delta)
            max_click_escape_delta = max(max_click_escape_delta, click_escape_delta)
            matched_items.append(
                {
                    "label": action_label(chrome_action),
                    "rect_max_delta": round(rect_delta, 3),
                    "center_delta": round(center_delta, 3),
                    "click_escape_delta": round(click_escape_delta, 3),
                    "chrome_rect": chrome_action.get("rect", {}),
                    "saccade_rect": saccade_action.get("rect", {}),
                }
            )
    return {
        "chrome_actions": len(chrome_actions),
        "saccade_actions": len(saccade_actions),
        "count_delta": abs(len(chrome_actions) - len(saccade_actions)),
        "labels_match": chrome_labels == saccade_labels,
        "missing_in_saccade": sorted(set(chrome_labels) - set(saccade_labels)),
        "extra_in_saccade": sorted(set(saccade_labels) - set(chrome_labels)),
        "max_rect_delta": round(max_rect_delta, 3),
        "max_center_delta": round(max_center_delta, 3),
        "max_click_escape_delta": round(max_click_escape_delta, 3),
        "matched_items": matched_items,
    }


def action_sort_key(action):
    rect = action.get("rect", {})
    return (
        action_label(action),
        str(action.get("kind", "")),
        str(action.get("tag", "")),
        round(float(rect.get("top", 0) or 0), 1),
        round(float(rect.get("left", 0) or 0), 1),
    )


def action_label(action):
    label = action.get("label") or action.get("action_id") or action.get("tag") or ""
    return " ".join(str(label).strip().lower().split())


def center_distance(a, b):
    ax, ay = rect_center(a)
    bx, by = rect_center(b)
    return math.sqrt((ax - bx) ** 2 + (ay - by) ** 2)


def rect_center(rect):
    return (
        float(rect.get("left", 0) or 0) + float(rect.get("width", 0) or 0) / 2,
        float(rect.get("top", 0) or 0) + float(rect.get("height", 0) or 0) / 2,
    )


def point_escape_distance(point, rect):
    x, y = point
    left = float(rect.get("left", 0) or 0)
    top = float(rect.get("top", 0) or 0)
    right = float(rect.get("right", left + float(rect.get("width", 0) or 0)) or 0)
    bottom = float(rect.get("bottom", top + float(rect.get("height", 0) or 0)) or 0)
    dx = max(left - x, 0, x - right)
    dy = max(top - y, 0, y - bottom)
    return math.sqrt(dx * dx + dy * dy)


def classify_diff(result):
    metrics = result["metrics"]
    layout = result.get("layout_probe_metrics", {})
    actions = result.get("action_map_metrics", {})
    chrome_clicks = result.get("chrome_click_verification", {})
    diff_classes = {
        "layout_rect_style_diff": [],
        "text_font_diff": [],
        "raster_canvas_diff": [],
        "action_map_diff": [],
        "action_geometry_warning": [],
        "viewport_dpr_diff": [],
        "policy_diff": [],
    }

    if not metrics.get("dimension_match"):
        diff_classes["viewport_dpr_diff"].append(
            f"Screenshot dimensions differ: Chrome {metrics.get('chrome_width')}x{metrics.get('chrome_height')} vs Saccade {metrics.get('saccade_width')}x{metrics.get('saccade_height')}"
        )

    if actions.get("count_delta", 0) != 0:
        diff_classes["action_map_diff"].append(
            f"Action count differs: Chrome {actions.get('chrome_actions')} vs Saccade {actions.get('saccade_actions')}"
        )
    if chrome_clicks.get("failed", 0) != 0:
        failures = [
            f"{item.get('expected_label', '')}->{item.get('target_label', '') or item.get('reason', '')}"
            for item in chrome_clicks.get("results", [])
            if not item.get("ok")
        ][:5]
        diff_classes["action_map_diff"].append(
            f"Chrome hit-test failed for {chrome_clicks.get('failed')} Saccade click point(s): {failures}"
        )
    if actions.get("labels_match") is False:
        missing = actions.get("missing_in_saccade", [])[:5]
        extra = actions.get("extra_in_saccade", [])[:5]
        diff_classes["action_map_diff"].append(
            f"Action labels differ; missing={missing}, extra={extra}"
        )
    if actions.get("max_click_escape_delta", 0) > ACTION_CLICK_ESCAPE_FAIL_PX:
        diff_classes["action_map_diff"].append(
            f"Saccade click point escapes Chrome action rect by {actions.get('max_click_escape_delta')}px"
        )
    elif actions.get("max_rect_delta", 0) > ACTION_RECT_WARNING_PX:
        diff_classes["action_geometry_warning"].append(
            f"Action rect geometry delta {actions.get('max_rect_delta')}px; click point remains within tolerance"
        )

    if layout.get("missing", 0):
        diff_classes["layout_rect_style_diff"].append(
            f"{layout.get('missing')} layout probe(s) missing"
        )
    if layout.get("display_mismatches", 0):
        diff_classes["layout_rect_style_diff"].append(
            f"{layout.get('display_mismatches')} display mismatch(es)"
        )
    if layout.get("grid_template_mismatches", 0):
        diff_classes["layout_rect_style_diff"].append(
            f"{layout.get('grid_template_mismatches')} grid-template mismatch(es)"
        )
    if layout.get("max_rect_delta", 0) > LAYOUT_RECT_FAIL_PX:
        diff_classes["layout_rect_style_diff"].append(
            f"Max layout probe rect delta {layout.get('max_rect_delta')}px"
        )

    diff_ratio = metrics.get("diff_ratio", 0)
    mean_abs = metrics.get("mean_abs_channel_delta", 0)
    if diff_ratio > 0.08:
        diff_classes["raster_canvas_diff"].append(
            f"High pixel diff ratio {diff_ratio:.3%}; inspect raster/canvas/SVG/font rendering"
        )
    elif diff_ratio > 0.03:
        diff_classes["text_font_diff"].append(
            f"Moderate visual diff ratio {diff_ratio:.3%}; likely text/font/spacing/raster delta"
        )
    elif mean_abs > 2.5:
        diff_classes["text_font_diff"].append(
            f"Low-area visual delta with mean_abs_channel_delta={mean_abs}"
        )

    has_action_diff = bool(diff_classes["action_map_diff"])
    has_viewport_diff = bool(diff_classes["viewport_dpr_diff"])
    has_layout_diff = bool(diff_classes["layout_rect_style_diff"])
    has_raster_diff = bool(diff_classes["raster_canvas_diff"])
    has_text_diff = bool(diff_classes["text_font_diff"])
    has_action_geometry_warning = bool(diff_classes["action_geometry_warning"])

    if has_action_diff or has_viewport_diff:
        verdict = "FAIL_ACTION_MAP"
        recommendation = "Do not trust this run for agent action without investigation; use chrome-reference for user-visible review."
    elif has_layout_diff:
        verdict = "FAIL_LAYOUT"
        recommendation = "Layout differs enough to affect coordinates; use chrome-reference or fix the Servo profile before agent action."
    elif has_raster_diff:
        verdict = "PASS_ACTION_YELLOW_RASTER"
        recommendation = "Action map and layout are acceptable; use chrome-reference for pixel UI review or raster/canvas judgement."
    elif has_text_diff or has_action_geometry_warning:
        verdict = "PASS_ACTION_YELLOW_VISUAL"
        recommendation = "Agent action is acceptable; use chrome-reference for polished visual review."
    else:
        verdict = "PASS_ACTION_GREEN"
        recommendation = "Servo profile is acceptable for agent action on this fixture; keep chrome-reference for public pixel parity."

    return {
        "verdict": verdict,
        "recommendation": recommendation,
        "diff_classes": diff_classes,
    }


def compare_pngs(chrome_path, saccade_path, threshold):
    cw, ch, chrome = read_png_rgb(chrome_path)
    sw, sh, saccade = read_png_rgb(saccade_path)
    width = min(cw, sw)
    height = min(ch, sh)
    total_pixels = max(1, width * height)
    diff_pixels = 0
    sum_abs = 0
    sum_sq = 0
    max_delta = 0
    diff_rgb = bytearray(width * height * 3)
    chrome_nonwhite = 0
    saccade_nonwhite = 0

    for y in range(height):
        for x in range(width):
            ci = (y * cw + x) * 3
            si = (y * sw + x) * 3
            di = (y * width + x) * 3
            cr, cg, cb = chrome[ci], chrome[ci + 1], chrome[ci + 2]
            sr, sg, sb = saccade[si], saccade[si + 1], saccade[si + 2]
            channel_diffs = (abs(cr - sr), abs(cg - sg), abs(cb - sb))
            pixel_delta = max(channel_diffs)
            if pixel_delta > threshold:
                diff_pixels += 1
                diff_rgb[di] = 255
                diff_rgb[di + 1] = max(0, 255 - pixel_delta)
                diff_rgb[di + 2] = max(0, 255 - pixel_delta)
            else:
                gray = int((cr + cg + cb + sr + sg + sb) / 6)
                diff_rgb[di] = gray
                diff_rgb[di + 1] = gray
                diff_rgb[di + 2] = gray
            for delta in channel_diffs:
                sum_abs += delta
                sum_sq += delta * delta
                max_delta = max(max_delta, delta)
            if (cr, cg, cb) < (245, 245, 245):
                chrome_nonwhite += 1
            if (sr, sg, sb) < (245, 245, 245):
                saccade_nonwhite += 1

    metrics = {
        "chrome_width": cw,
        "chrome_height": ch,
        "saccade_width": sw,
        "saccade_height": sh,
        "width": width,
        "height": height,
        "dimension_match": cw == sw and ch == sh,
        "diff_pixels": diff_pixels,
        "diff_ratio": round(diff_pixels / total_pixels, 6),
        "mean_abs_channel_delta": round(sum_abs / (total_pixels * 3), 3),
        "rms_channel_delta": round(math.sqrt(sum_sq / (total_pixels * 3)), 3),
        "max_channel_delta": max_delta,
        "chrome_nonwhite_ratio": round(chrome_nonwhite / total_pixels, 6),
        "saccade_nonwhite_ratio": round(saccade_nonwhite / total_pixels, 6),
    }
    return metrics, bytes(diff_rgb)


def read_png_rgb(path):
    data = pathlib.Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"not a PNG: {path}")
    offset = 8
    width = height = color_type = bit_depth = None
    idat = bytearray()
    while offset < len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_data = data[offset + 8 : offset + 8 + length]
        offset += 12 + length
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type = struct.unpack(">IIBB", chunk_data[:10])
        elif chunk_type == b"IDAT":
            idat.extend(chunk_data)
        elif chunk_type == b"IEND":
            break
    if bit_depth != 8 or color_type not in (2, 6):
        raise ValueError(f"unsupported PNG format for {path}: bit_depth={bit_depth} color={color_type}")
    channels = 3 if color_type == 2 else 4
    stride = width * channels
    raw = zlib.decompress(bytes(idat))
    rows = []
    index = 0
    prev = bytearray(stride)
    for _ in range(height):
        filter_type = raw[index]
        index += 1
        row = bytearray(raw[index : index + stride])
        index += stride
        unfilter(row, prev, filter_type, channels)
        rows.append(row)
        prev = row
    rgb = bytearray(width * height * 3)
    for y, row in enumerate(rows):
        for x in range(width):
            src = x * channels
            dst = (y * width + x) * 3
            if channels == 4:
                alpha = row[src + 3] / 255
                rgb[dst] = int(row[src] * alpha + 255 * (1 - alpha))
                rgb[dst + 1] = int(row[src + 1] * alpha + 255 * (1 - alpha))
                rgb[dst + 2] = int(row[src + 2] * alpha + 255 * (1 - alpha))
            else:
                rgb[dst : dst + 3] = row[src : src + 3]
    return width, height, bytes(rgb)


def unfilter(row, prev, filter_type, bpp):
    if filter_type == 0:
        return
    for i in range(len(row)):
        left = row[i - bpp] if i >= bpp else 0
        up = prev[i]
        up_left = prev[i - bpp] if i >= bpp else 0
        if filter_type == 1:
            row[i] = (row[i] + left) & 0xFF
        elif filter_type == 2:
            row[i] = (row[i] + up) & 0xFF
        elif filter_type == 3:
            row[i] = (row[i] + ((left + up) // 2)) & 0xFF
        elif filter_type == 4:
            row[i] = (row[i] + paeth(left, up, up_left)) & 0xFF
        else:
            raise ValueError(f"unsupported PNG filter: {filter_type}")


def paeth(a, b, c):
    p = a + b - c
    pa = abs(p - a)
    pb = abs(p - b)
    pc = abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def write_png_rgb(path, width, height, rgb):
    rows = bytearray()
    stride = width * 3
    for y in range(height):
        rows.append(0)
        rows.extend(rgb[y * stride : (y + 1) * stride])
    chunks = [
        png_chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)),
        png_chunk(b"IDAT", zlib.compress(bytes(rows), level=6)),
        png_chunk(b"IEND", b""),
    ]
    pathlib.Path(path).write_bytes(b"\x89PNG\r\n\x1a\n" + b"".join(chunks))


def png_chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def write_html(run_dir, results, args):
    rows = []
    figures = []
    for result in results:
        m = result["metrics"]
        lp = result.get("layout_probe_metrics", {})
        am = result.get("action_map_metrics", {})
        chrome_clicks = result.get("chrome_click_verification", {})
        classification = result.get("diff_classification", {})
        layout_summary = (
            f"max rect {lp.get('max_rect_delta', 0):.1f}px, "
            f"display {lp.get('display_mismatches', 0)}, "
            f"grid {lp.get('grid_template_mismatches', 0)}"
        )
        action_summary = (
            f"{result['chrome_actions']} / {result['saccade_actions']}, "
            f"hit {chrome_clicks.get('passed', 0)}/{chrome_clicks.get('total', 0)}, "
            f"escape {am.get('max_click_escape_delta', 0):.1f}px, "
            f"rect {am.get('max_rect_delta', 0):.1f}px"
        )
        rows.append(
            "<tr>"
            f"<td>{esc(result['fixture'])}</td>"
            f"<td>{m['dimension_match']}</td>"
            f"<td>{m['diff_ratio']:.3%}</td>"
            f"<td>{m['mean_abs_channel_delta']}</td>"
            f"<td>{esc(action_summary)}</td>"
            f"<td>{esc(layout_summary)}</td>"
            f"<td>{esc(classification.get('verdict', ''))}</td>"
            "</tr>"
        )
        recommendation = classification.get("recommendation", "")
        figures.append(
            f"""
            <section class="case">
              <h2>{esc(result['fixture'])}</h2>
              <p class="muted">diff_ratio={m['diff_ratio']:.3%}, mean_abs={m['mean_abs_channel_delta']}, rms={m['rms_channel_delta']}, actions {esc(action_summary)}, layout {esc(layout_summary)}</p>
              <p><strong>{esc(classification.get('verdict', ''))}</strong> · {esc(recommendation)}</p>
              <div class="compare">
                <figure><figcaption>Chrome</figcaption><img src="{esc(result['chrome_screenshot'])}"></figure>
                <figure><figcaption>Saccade</figcaption><img src="{esc(result['saccade_screenshot'])}"></figure>
                <figure><figcaption>Diff</figcaption><img src="{esc(result['diff_image'])}"></figure>
              </div>
            </section>
            """
        )
    page = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Saccade Visual Parity Report</title>
  <style>
    body {{ margin: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #f6f8fb; color: #1d2430; }}
    header {{ padding: 28px 34px; background: #ffffff; border-bottom: 1px solid #d8dee8; }}
    main {{ padding: 24px 34px; }}
    table {{ border-collapse: collapse; width: 100%; background: #ffffff; border: 1px solid #d8dee8; }}
    th, td {{ border-bottom: 1px solid #d8dee8; padding: 10px 12px; text-align: left; }}
    .muted {{ color: #64748b; }}
    .case {{ margin-top: 28px; }}
    .compare {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 14px; }}
    figure {{ margin: 0; background: #ffffff; border: 1px solid #d8dee8; border-radius: 8px; overflow: hidden; }}
    figcaption {{ padding: 10px 12px; border-bottom: 1px solid #d8dee8; font-weight: 700; }}
    img {{ display: block; width: 100%; height: auto; }}
  </style>
</head>
<body>
  <header>
    <h1>Saccade Visual Parity Report</h1>
    <p class="muted">Viewport {args.width}x{args.height}. Chrome CDP screenshots compared against Saccade live worker screenshots. Rendering profile: {esc(resolved_rendering_profile(args))}. Legacy Grid override: {esc(args.saccade_grid)}.</p>
  </header>
  <main>
    <table>
      <thead><tr><th>Fixture</th><th>Dimensions</th><th>Diff ratio</th><th>Mean abs</th><th>Actions / Chrome hit</th><th>Layout probes</th><th>Verdict</th></tr></thead>
      <tbody>{''.join(rows)}</tbody>
    </table>
    {''.join(figures)}
  </main>
</body>
</html>
"""
    (run_dir / "index.html").write_text(page)


def run(cmd, timeout):
    result = subprocess.run(
        cmd,
        cwd=WORKSPACE,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
    )
    if result.returncode != 0:
        raise RuntimeError(f"command failed: {' '.join(cmd)}\n{result.stdout}")
    return result.stdout


def run_with_retry(cmd, timeout, attempts):
    last_error = None
    for attempt in range(1, attempts + 1):
        try:
            return run(cmd, timeout)
        except Exception as error:
            last_error = error
            if attempt < attempts:
                time.sleep(1)
    raise last_error


def rel(run_dir, path):
    return os.path.relpath(path, run_dir)


def esc(value):
    return html.escape(str(value), quote=True)


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"visual parity compare failed: {error}", file=sys.stderr)
        sys.exit(1)
