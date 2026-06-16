#!/usr/bin/env python3
import argparse
import html
import json
import os
import pathlib
import subprocess
import sys
import time


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_FIXTURES = [
    "font_control_metrics",
    "form_control_width_modes",
    "textarea_default_height",
]
DEFAULT_WIDTHS = [1280, 1600]
VIEWPORT_TOLERANCE_PX = 2
RECT_YELLOW_PX = 8
RECT_RED_PX = 24
TEXT_YELLOW_PX = 4
TEXT_RED_PX = 12
FONT_KEYS = ["fontSize", "fontWeight", "lineHeight", "letterSpacing"]


def main():
    args = parse_args()
    run_dir = (WORKSPACE / "runs" / "browser_compat_metrics" / f"metrics_{unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    rows = []
    for width in args.widths:
        parity_dir = run_visual_parity(width, args, run_dir)
        manifest = json.loads((parity_dir / "visual_parity_manifest.json").read_text())
        for fixture in manifest.get("fixtures", []):
            rows.append(analyze_fixture(width, args.height, parity_dir, fixture))

    summary = summarize(rows)
    report = {
        "engine": "saccade-browser-compat-metrics-v0",
        "created_at_unix_ms": unix_ms(),
        "rendering_profile": args.rendering_profile,
        "widths": args.widths,
        "height": args.height,
        "fixtures": args.fixtures,
        "thresholds": {
            "viewport_tolerance_px": VIEWPORT_TOLERANCE_PX,
            "rect_yellow_px": RECT_YELLOW_PX,
            "rect_red_px": RECT_RED_PX,
            "text_yellow_px": TEXT_YELLOW_PX,
            "text_red_px": TEXT_RED_PX,
        },
        "summary": summary,
        "rows": rows,
    }
    (run_dir / "browser_compat_metrics.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n"
    )
    write_html(run_dir, report)

    print(
        "BROWSER COMPAT METRICS "
        f"invalid={summary['invalid']} red={summary['red']} yellow={summary['yellow']} green={summary['green']} "
        f"report={run_dir / 'index.html'}"
    )
    if args.fail_on_red and (summary["invalid"] or summary["red"]):
        raise SystemExit(1)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Measure Chrome-vs-Saccade font/control metrics and viewport validity."
    )
    parser.add_argument("fixtures", nargs="*", default=DEFAULT_FIXTURES)
    parser.add_argument("--widths", nargs="+", type=int, default=DEFAULT_WIDTHS)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--timeout-sec", type=float, default=70)
    parser.add_argument(
        "--rendering-profile",
        choices=("servo-safe", "servo-modern"),
        default="servo-modern",
    )
    parser.add_argument(
        "--fail-on-red",
        action="store_true",
        help="Exit non-zero when any row is invalid or red.",
    )
    return parser.parse_args()


def run_visual_parity(width, args, run_dir):
    cmd = [
        sys.executable,
        str(WORKSPACE / "scripts" / "visual_parity_compare.py"),
        *args.fixtures,
        "--width",
        str(width),
        "--height",
        str(args.height),
        "--timeout-sec",
        str(args.timeout_sec),
        "--rendering-profile",
        args.rendering_profile,
    ]
    print(f"BROWSER COMPAT width={width} fixtures={','.join(args.fixtures)}")
    proc = subprocess.run(
        cmd,
        cwd=WORKSPACE,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=args.timeout_sec * max(1, len(args.fixtures)) + 120,
    )
    (run_dir / f"visual_parity_width_{width}.log").write_text(proc.stdout)
    if proc.returncode != 0:
        raise SystemExit(f"visual parity failed for width {width}\n{proc.stdout}")
    for line in reversed(proc.stdout.splitlines()):
        if "report=" in line:
            return pathlib.Path(line.split("report=", 1)[1].strip()).resolve().parent
    raise SystemExit(f"could not find visual parity report for width {width}\n{proc.stdout}")


def analyze_fixture(width, height, parity_dir, fixture):
    case_dir = pathlib.Path(fixture["case_dir"]).resolve()
    saccade_result = json.loads(pathlib.Path(fixture["artifacts"]["saccade_worker_result"]).read_text())
    chrome_truth = json.loads(pathlib.Path(fixture["artifacts"]["chrome_truth"]).read_text())
    saccade_truth = saccade_result.get("truth") or {}
    viewport = viewport_status(width, height, chrome_truth, saccade_truth, saccade_result)
    probe_metrics = compare_probe_metrics(chrome_truth, saccade_truth)
    reasons = []
    verdict = "GREEN"
    if not viewport["valid"]:
        verdict = "INVALID_VIEWPORT"
        reasons.extend(viewport["reasons"])
    elif probe_metrics["max_control_rect_delta"] > RECT_RED_PX:
        verdict = "RED"
        reasons.append(f"control rect delta {probe_metrics['max_control_rect_delta']}px")
    elif probe_metrics["max_text_rect_delta"] > TEXT_RED_PX:
        verdict = "RED"
        reasons.append(f"text rect delta {probe_metrics['max_text_rect_delta']}px")
    elif fixture.get("diff_classification", {}).get("verdict") in ("FAIL_ACTION_MAP", "FAIL_LAYOUT"):
        verdict = "RED"
        reasons.append(f"visual parity verdict {fixture.get('diff_classification', {}).get('verdict')}")
    elif (
        probe_metrics["max_control_rect_delta"] > RECT_YELLOW_PX
        or probe_metrics["max_text_rect_delta"] > TEXT_YELLOW_PX
        or probe_metrics["font_style_mismatches"] > 0
        or probe_metrics["scroll_metric_mismatches"] > 0
    ):
        verdict = "YELLOW"
        reasons.extend(probe_metrics["top_reasons"][:5])

    return {
        "width": width,
        "height": height,
        "fixture": fixture["fixture"],
        "verdict": verdict,
        "reasons": reasons,
        "viewport": viewport,
        "probe_metrics": probe_metrics,
        "visual_parity_verdict": fixture.get("diff_classification", {}).get("verdict"),
        "diff_ratio": fixture.get("metrics", {}).get("diff_ratio"),
        "case_dir": str(case_dir),
        "parity_report": rel(parity_dir / "index.html"),
        "case_result": rel(case_dir / "case_result.json"),
        "chrome_screenshot": rel(case_dir / "chrome_page.png"),
        "saccade_screenshot": rel(case_dir / "saccade_page.png"),
        "diff_image": rel(case_dir / "diff.png"),
    }


def viewport_status(width, height, chrome_truth, saccade_truth, saccade_result):
    chrome_viewport = chrome_truth.get("viewport") or {}
    saccade_viewport = saccade_truth.get("viewport") or {}
    runtime_geometry = saccade_result.get("runtime_geometry") or {}
    reasons = []
    chrome_valid = near(chrome_viewport.get("width"), width) and near(chrome_viewport.get("height"), height)
    saccade_valid = near(saccade_viewport.get("width"), width) and near(saccade_viewport.get("height"), height)
    if not chrome_valid:
        reasons.append(
            f"Chrome CSS viewport {chrome_viewport.get('width')}x{chrome_viewport.get('height')} != requested {width}x{height}"
        )
    if not saccade_valid:
        reasons.append(
            f"Saccade CSS viewport {saccade_viewport.get('width')}x{saccade_viewport.get('height')} != requested {width}x{height}"
        )
    hidpi = float(runtime_geometry.get("hidpi_scale_factor") or 1)
    context = runtime_geometry.get("rendering_context_device") or {}
    logical_context = {
        "width": round(float(context.get("width") or 0) / hidpi, 3) if hidpi else 0,
        "height": round(float(context.get("height") or 0) / hidpi, 3) if hidpi else 0,
    }
    if logical_context["width"] and not near(logical_context["width"], width):
        reasons.append(
            f"Saccade logical context width {logical_context['width']} != requested {width}"
        )
    if logical_context["height"] and not near(logical_context["height"], height):
        reasons.append(
            f"Saccade logical context height {logical_context['height']} != requested {height}"
        )
    return {
        "valid": not reasons,
        "requested": {"width": width, "height": height},
        "chrome_css": chrome_viewport,
        "saccade_css": saccade_viewport,
        "saccade_runtime_geometry": runtime_geometry,
        "saccade_logical_context": logical_context,
        "reasons": reasons,
    }


def compare_probe_metrics(chrome_truth, saccade_truth):
    chrome = {item.get("name", ""): item for item in chrome_truth.get("layoutProbes", [])}
    saccade = {item.get("name", ""): item for item in saccade_truth.get("layout_probes", [])}
    names = sorted(name for name in set(chrome) | set(saccade) if name)
    max_control_rect_delta = 0.0
    max_text_rect_delta = 0.0
    font_style_mismatches = 0
    scroll_metric_mismatches = 0
    missing = 0
    rows = []
    top_reasons = []
    for name in names:
        c = chrome.get(name)
        s = saccade.get(name)
        if not c or not s:
            missing += 1
            top_reasons.append(f"{name} missing in {'Chrome' if not c else 'Saccade'}")
            continue
        control = is_control_probe(name, c, s)
        rect_delta = max(rect_deltas(c.get("rect"), s.get("rect")).values())
        text_delta = text_rect_delta(c, s)
        scroll_delta = scroll_metric_delta(c, s)
        style_diffs = {
            key: {"chrome": c.get(key, ""), "saccade": s.get(key, "")}
            for key in FONT_KEYS
            if c.get(key, "") != s.get(key, "")
        }
        if control:
            max_control_rect_delta = max(max_control_rect_delta, rect_delta)
        max_text_rect_delta = max(max_text_rect_delta, text_delta)
        if style_diffs:
            font_style_mismatches += 1
        if scroll_delta > TEXT_YELLOW_PX:
            scroll_metric_mismatches += 1
        if rect_delta > RECT_YELLOW_PX or text_delta > TEXT_YELLOW_PX or style_diffs or scroll_delta > TEXT_YELLOW_PX:
            top_reasons.append(
                f"{name}: rect={round(rect_delta, 2)} text={round(text_delta, 2)} scroll={round(scroll_delta, 2)} styles={','.join(style_diffs)}"
            )
        rows.append(
            {
                "name": name,
                "tag": c.get("tag") or s.get("tag"),
                "control": control,
                "rect_delta": round(rect_delta, 3),
                "text_rect_delta": round(text_delta, 3),
                "scroll_metric_delta": round(scroll_delta, 3),
                "font_style_diffs": style_diffs,
                "chrome_rect": c.get("rect"),
                "saccade_rect": s.get("rect"),
                "chrome_text_metrics": c.get("textMetrics"),
                "saccade_text_metrics": s.get("textMetrics"),
            }
        )
    return {
        "probe_count": len(names),
        "missing": missing,
        "max_control_rect_delta": round(max_control_rect_delta, 3),
        "max_text_rect_delta": round(max_text_rect_delta, 3),
        "font_style_mismatches": font_style_mismatches,
        "scroll_metric_mismatches": scroll_metric_mismatches,
        "top_reasons": top_reasons[:12],
        "items": rows,
    }


def is_control_probe(name, chrome_probe, saccade_probe):
    tag = (chrome_probe.get("tag") or saccade_probe.get("tag") or "").lower()
    if tag in {"button", "input", "select", "textarea"}:
        return True
    return any(part in name for part in ("button", "input", "select", "textarea", "control"))


def rect_deltas(a, b):
    a = a or {}
    b = b or {}
    return {
        key: abs(float(a.get(key, 0) or 0) - float(b.get(key, 0) or 0))
        for key in ("left", "top", "width", "height")
    }


def text_rect_delta(chrome_probe, saccade_probe):
    c = ((chrome_probe.get("textMetrics") or {}).get("rangeRect") or {})
    s = ((saccade_probe.get("textMetrics") or {}).get("rangeRect") or {})
    if not c and not s:
        return 0.0
    return max(rect_deltas(c, s).values())


def scroll_metric_delta(chrome_probe, saccade_probe):
    c = chrome_probe.get("textMetrics") or {}
    s = saccade_probe.get("textMetrics") or {}
    return max(
        abs(float(c.get(key, 0) or 0) - float(s.get(key, 0) or 0))
        for key in ("clientWidth", "clientHeight", "scrollWidth", "scrollHeight")
    )


def summarize(rows):
    return {
        "invalid": sum(1 for row in rows if row["verdict"] == "INVALID_VIEWPORT"),
        "red": sum(1 for row in rows if row["verdict"] == "RED"),
        "yellow": sum(1 for row in rows if row["verdict"] == "YELLOW"),
        "green": sum(1 for row in rows if row["verdict"] == "GREEN"),
        "total": len(rows),
    }


def write_html(run_dir, report):
    rows_html = []
    for row in report["rows"]:
        cls = {
            "GREEN": "green",
            "YELLOW": "yellow",
            "RED": "red",
            "INVALID_VIEWPORT": "invalid",
        }.get(row["verdict"], "")
        probe = row["probe_metrics"]
        viewport = row["viewport"]
        rows_html.append(
            "<tr>"
            f"<td>{row['width']}x{row['height']}</td>"
            f"<td>{esc(row['fixture'])}</td>"
            f"<td class='{cls}'>{esc(row['verdict'])}</td>"
            f"<td>{esc(row['visual_parity_verdict'])}<br>diff {pct(row['diff_ratio'])}</td>"
            f"<td>control {probe['max_control_rect_delta']}px<br>text {probe['max_text_rect_delta']}px<br>font styles {probe['font_style_mismatches']}<br>scroll {probe['scroll_metric_mismatches']}</td>"
            f"<td>{esc(viewport_summary(viewport))}</td>"
            f"<td>{esc('; '.join(row['reasons'][:4]) or '; '.join(probe['top_reasons'][:4]))}</td>"
            "<td>"
            f"<a href='{html_path(run_dir, row['case_result'])}'>case</a> "
            f"<a href='{html_path(run_dir, row['parity_report'])}'>parity</a>"
            "</td>"
            "</tr>"
        )
    page = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Saccade Browser Compat Metrics</title>
  <style>
    body {{ margin: 24px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #111827; background: #f8fafc; }}
    table {{ border-collapse: collapse; width: 100%; background: #fff; }}
    th, td {{ border: 1px solid #d9e1ec; padding: 8px; text-align: left; vertical-align: top; font-size: 13px; }}
    th {{ background: #eef2f7; }}
    .green {{ color: #166534; font-weight: 700; }}
    .yellow {{ color: #92400e; font-weight: 700; }}
    .red, .invalid {{ color: #b91c1c; font-weight: 700; }}
    code {{ font-size: 12px; }}
  </style>
</head>
<body>
  <h1>Saccade Browser Compat Metrics</h1>
  <p>Profile: <code>{esc(report['rendering_profile'])}</code>. Summary: <code>{esc(report['summary'])}</code>.</p>
  <table>
    <thead><tr><th>Viewport</th><th>Fixture</th><th>Verdict</th><th>Visual</th><th>Probe Max</th><th>Viewport Validity</th><th>Reasons</th><th>Artifacts</th></tr></thead>
    <tbody>{''.join(rows_html)}</tbody>
  </table>
</body>
</html>
"""
    (run_dir / "index.html").write_text(page)


def viewport_summary(viewport):
    requested = viewport.get("requested") or {}
    chrome_css = viewport.get("chrome_css") or {}
    saccade_css = viewport.get("saccade_css") or {}
    logical = viewport.get("saccade_logical_context") or {}
    return (
        f"requested {requested.get('width')}x{requested.get('height')}; "
        f"Chrome {chrome_css.get('width')}x{chrome_css.get('height')}; "
        f"Saccade CSS {saccade_css.get('width')}x{saccade_css.get('height')}; "
        f"context {logical.get('width')}x{logical.get('height')}"
    )


def near(value, expected):
    try:
        return abs(float(value) - float(expected)) <= VIEWPORT_TOLERANCE_PX
    except (TypeError, ValueError):
        return False


def pct(value):
    return "" if value is None else f"{float(value) * 100:.2f}%"


def rel(path):
    return pathlib.Path(path).resolve().relative_to(WORKSPACE).as_posix()


def html_path(run_dir, workspace_relative_path):
    target = WORKSPACE / workspace_relative_path
    return html.escape(os.path.relpath(target, run_dir), quote=True)


def esc(value):
    return html.escape(str(value), quote=True)


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    main()
