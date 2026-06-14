#!/usr/bin/env python3
import argparse
import html
import json
import os
import pathlib
import subprocess
import time


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_FIXTURES = [
    "grid_percent_100_50",
    "form_controls",
    "form_control_width_modes",
    "responsive_cards",
]
DEFAULT_WIDTHS = [390, 768, 1000, 1280, 1600]


def main():
    args = parse_args()
    run_dir = (WORKSPACE / "runs" / "visual_parity_width_matrix" / f"matrix_{unix_ms()}").resolve()
    run_dir.mkdir(parents=True, exist_ok=True)

    rows = []
    for width in args.widths:
        cmd = [
            "python3",
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
        print(f"WIDTH MATRIX width={width} fixtures={','.join(args.fixtures)}")
        proc = subprocess.run(
            cmd,
            cwd=WORKSPACE,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=args.timeout_sec * max(1, len(args.fixtures)) + 120,
        )
        (run_dir / f"width_{width}.log").write_text(proc.stdout)
        if proc.returncode != 0:
            raise SystemExit(f"visual parity failed for width {width}\n{proc.stdout}")
        report = parse_report_path(proc.stdout)
        manifest = json.loads((report.parent / "visual_parity_manifest.json").read_text())
        for fixture in manifest.get("fixtures", []):
            rows.append(row_from_fixture(width, report.parent, fixture))

    manifest = {
        "engine": "saccade-visual-parity-width-matrix-v0",
        "created_at_unix_ms": unix_ms(),
        "height": args.height,
        "widths": args.widths,
        "fixtures": args.fixtures,
        "rendering_profile": args.rendering_profile,
        "rows": rows,
    }
    (run_dir / "width_matrix_manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    )
    write_html(run_dir, manifest)
    print(f"WIDTH MATRIX PASS rows={len(rows)} report={run_dir / 'index.html'}")


def parse_args():
    parser = argparse.ArgumentParser(
        description="Run Chrome-vs-Saccade visual parity at multiple viewport widths."
    )
    parser.add_argument("fixtures", nargs="*", default=DEFAULT_FIXTURES)
    parser.add_argument("--widths", nargs="+", type=int, default=DEFAULT_WIDTHS)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--timeout-sec", type=float, default=60)
    parser.add_argument(
        "--rendering-profile",
        choices=("servo-safe", "servo-modern"),
        default="servo-modern",
    )
    return parser.parse_args()


def parse_report_path(stdout):
    for line in reversed(stdout.splitlines()):
        marker = "report="
        if marker in line:
            return pathlib.Path(line.split(marker, 1)[1].strip()).resolve()
    raise RuntimeError(f"could not find report path in output\n{stdout}")


def row_from_fixture(width, parity_dir, fixture):
    case_dir = pathlib.Path(fixture["case_dir"]).resolve()
    metrics = fixture.get("metrics", {})
    action = fixture.get("action_map_metrics", {})
    layout = fixture.get("layout_probe_metrics", {})
    clicks = fixture.get("chrome_click_verification", {})
    normalized = fixture.get("saccade_screenshot_normalized", {})
    return {
        "width": width,
        "fixture": fixture["fixture"],
        "verdict": fixture.get("diff_classification", {}).get("verdict"),
        "diff_ratio": metrics.get("diff_ratio"),
        "mean_abs_channel_delta": metrics.get("mean_abs_channel_delta"),
        "chrome_actions": fixture.get("chrome_actions"),
        "saccade_actions": fixture.get("saccade_actions"),
        "action_rect_delta": action.get("max_rect_delta"),
        "click_escape_delta": action.get("max_click_escape_delta"),
        "chrome_hit_passed": clicks.get("passed"),
        "chrome_hit_total": clicks.get("total"),
        "layout_rect_delta": layout.get("max_rect_delta"),
        "layout_display_mismatches": layout.get("display_mismatches"),
        "layout_grid_template_mismatches": layout.get("grid_template_mismatches"),
        "layout_style_mismatches": layout.get("style_mismatches"),
        "saccade_normalized": normalized,
        "parity_report": rel(parity_dir / "index.html", case_dir=parity_dir.parent.parent),
        "case_result": rel(case_dir / "case_result.json", case_dir=parity_dir.parent.parent),
        "chrome_screenshot": rel(case_dir / "chrome_page.png", case_dir=parity_dir.parent.parent),
        "saccade_screenshot": rel(case_dir / "saccade_page.png", case_dir=parity_dir.parent.parent),
        "diff_image": rel(case_dir / "diff.png", case_dir=parity_dir.parent.parent),
    }


def rel(path, case_dir):
    return pathlib.Path(path).resolve().relative_to(WORKSPACE).as_posix()


def write_html(run_dir, manifest):
    rows = manifest["rows"]
    lines = [
        "<!doctype html>",
        "<html><head><meta charset='utf-8'>",
        "<meta name='viewport' content='width=device-width, initial-scale=1'>",
        "<title>Saccade Width Matrix</title>",
        "<style>",
        "body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;margin:24px;color:#111827;background:#f8fafc}",
        "table{border-collapse:collapse;width:100%;background:white}th,td{border:1px solid #d9e1ec;padding:8px;text-align:left;font-size:13px}th{background:#eef2f7}",
        ".pass{color:#166534;font-weight:700}.warn{color:#92400e;font-weight:700}.fail{color:#b91c1c;font-weight:700}",
        ".thumbs{display:flex;gap:8px;align-items:flex-start}.thumbs a{display:block}.thumbs img{max-width:180px;border:1px solid #d9e1ec;background:white}",
        "code{font-size:12px}",
        "</style></head><body>",
        "<h1>Saccade Width Matrix</h1>",
        f"<p>Profile: <code>{esc(manifest['rendering_profile'])}</code>. Height: <code>{manifest['height']}</code>. Widths: <code>{', '.join(map(str, manifest['widths']))}</code>.</p>",
        "<table>",
        "<tr><th>Width</th><th>Fixture</th><th>Verdict</th><th>Diff</th><th>Actions</th><th>Rect Delta</th><th>Layout Delta</th><th>Capture</th><th>Images</th></tr>",
    ]
    for row in rows:
        verdict = row.get("verdict") or ""
        cls = "pass" if verdict.endswith("GREEN") else "fail" if verdict.startswith("FAIL") else "warn"
        hit = f"{row.get('chrome_hit_passed')}/{row.get('chrome_hit_total')}"
        actions = f"{row.get('chrome_actions')} / {row.get('saccade_actions')} hit {hit}"
        lines.append(
            "<tr>"
            f"<td>{row['width']}</td>"
            f"<td>{esc(row['fixture'])}</td>"
            f"<td class='{cls}'>{esc(verdict)}</td>"
            f"<td>{pct(row.get('diff_ratio'))}</td>"
            f"<td>{esc(actions)}</td>"
            f"<td>{fmt(row.get('action_rect_delta'))} px<br>escape {fmt(row.get('click_escape_delta'))} px</td>"
            f"<td>{fmt(row.get('layout_rect_delta'))} px<br>display {row.get('layout_display_mismatches')} grid {row.get('layout_grid_template_mismatches')} style {row.get('layout_style_mismatches')}</td>"
            f"<td>{capture_summary(row)}</td>"
            "<td><div class='thumbs'>"
            f"<a href='{html_path(run_dir, row['chrome_screenshot'])}'><img src='{html_path(run_dir, row['chrome_screenshot'])}' alt='Chrome'></a>"
            f"<a href='{html_path(run_dir, row['saccade_screenshot'])}'><img src='{html_path(run_dir, row['saccade_screenshot'])}' alt='Saccade'></a>"
            f"<a href='{html_path(run_dir, row['diff_image'])}'><img src='{html_path(run_dir, row['diff_image'])}' alt='Diff'></a>"
            "</div></td>"
            "</tr>"
        )
    lines.extend(["</table>", "</body></html>"])
    (run_dir / "index.html").write_text("\n".join(lines) + "\n")


def pct(value):
    return "" if value is None else f"{value * 100:.2f}%"


def fmt(value):
    if value is None:
        return ""
    if isinstance(value, float):
        return f"{value:.1f}"
    return str(value)


def esc(value):
    return html.escape(str(value), quote=True)


def capture_summary(row):
    normalized = row.get("saccade_normalized") or {}
    raw = f"{normalized.get('raw_width', '?')}x{normalized.get('raw_height', '?')}"
    target = f"{normalized.get('target_width', '?')}x{normalized.get('target_height', '?')}"
    if normalized.get("applied"):
        status = f"normalized @{normalized.get('scale')}"
    elif normalized.get("reason"):
        status = normalized.get("reason")
    else:
        status = "native"
    return f"<code>{esc(raw)} -> {esc(target)}</code><br>{esc(status)}"


def html_path(run_dir, workspace_relative_path):
    target = WORKSPACE / workspace_relative_path
    return html.escape(os.path.relpath(target, run_dir), quote=True)


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    main()
