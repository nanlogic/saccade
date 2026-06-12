#!/usr/bin/env python3
import argparse
import html
import json
import os
import pathlib
import socket
import subprocess
import sys
import time
import urllib.request


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = WORKSPACE / "test_pages" / "visual_parity" / "dashboard" / "index.html"


def main():
    args = parse_args()
    run_dir = pathlib.Path(args.output_dir).resolve() if args.output_dir else default_run_dir()
    run_dir.mkdir(parents=True, exist_ok=True)

    local_server = None
    try:
        if args.url:
            target_url = args.url
            native_url_source = "user_url"
        else:
            local_server, target_url = start_local_http_server(DEFAULT_FIXTURE)
            native_url_source = "local_http_fixture"
        visual = run_visual_parity(args, run_dir)
        native = run_native_captures(args, run_dir, target_url)
        manifest = {
            "engine": "saccade-demo-comparison-pack-v0",
            "created_at_unix_ms": unix_ms(),
            "target_url": target_url,
            "target_url_source": native_url_source,
            "fixtures": args.fixtures,
            "rendering_profile": args.rendering_profile,
            "visual_parity": visual,
            "native_browser_ui": native,
            "artifacts": {
                "demo_review": "demo_review.html",
                "manifest": "demo_comparison_manifest.json",
            },
            "note": "Native browser UI screenshots are public-demo artifacts. Browser truth, safety, and replay evidence remain separate Saccade artifacts.",
        }
        manifest_path = run_dir / "demo_comparison_manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        write_demo_html(run_dir, manifest)
        print(f"DEMO COMPARISON PACK READY report={run_dir / 'demo_review.html'}")
    finally:
        if local_server:
            local_server.terminate()
            try:
                local_server.wait(timeout=3)
            except subprocess.TimeoutExpired:
                local_server.kill()


def parse_args():
    parser = argparse.ArgumentParser(
        description="Build a public demo comparison pack with native browser UI attempts and Saccade evidence."
    )
    parser.add_argument("--url", help="URL to open for native browser UI capture. Defaults to dashboard fixture.")
    parser.add_argument(
        "--fixtures",
        nargs="+",
        default=["dashboard"],
        help="Visual parity fixtures to run, or all.",
    )
    parser.add_argument("--rendering-profile", default="servo-modern")
    parser.add_argument("--timeout-sec", type=float, default=60)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument(
        "--native-browsers",
        nargs="+",
        default=["chrome", "safari"],
        choices=["chrome", "safari"],
    )
    parser.add_argument("--output-dir")
    parser.add_argument(
        "--skip-native",
        action="store_true",
        help="Skip macOS native browser UI capture attempts.",
    )
    return parser.parse_args()


def default_run_dir():
    return WORKSPACE / "runs" / "demo_pack" / f"demo_{unix_ms()}"


def start_local_http_server(target_file):
    port = free_port()
    process = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "http.server",
            str(port),
            "--bind",
            "127.0.0.1",
            "--directory",
            str(WORKSPACE),
        ],
        cwd=WORKSPACE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    rel_path = target_file.resolve().relative_to(WORKSPACE.resolve()).as_posix()
    url = f"http://127.0.0.1:{port}/{rel_path}"
    wait_for_http_server(process, url)
    return process, url


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_http_server(process, url):
    deadline = time.monotonic() + 8
    last_error = None
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("local HTTP server exited before demo capture")
        try:
            with urllib.request.urlopen(url, timeout=1) as response:
                if response.status < 500:
                    return
        except Exception as error:
            last_error = error
            time.sleep(0.1)
    raise RuntimeError(f"local HTTP server did not become ready: {last_error}")


def run_visual_parity(args, run_dir):
    fixture_count = 7 if args.fixtures == ["all"] else len(args.fixtures)
    cmd = [
        str(WORKSPACE / "scripts" / "visual_parity_compare.py"),
        "--timeout-sec",
        str(args.timeout_sec),
        "--rendering-profile",
        args.rendering_profile,
        "--width",
        str(args.width),
        "--height",
        str(args.height),
        *args.fixtures,
    ]
    result = subprocess.run(
        cmd,
        cwd=WORKSPACE,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=max(30, args.timeout_sec * max(2, fixture_count + 1)),
    )
    (run_dir / "visual_parity_stdout.log").write_text(result.stdout)
    if result.returncode != 0:
        raise RuntimeError(f"visual parity failed:\n{result.stdout}")
    report_path = parse_report_path(result.stdout)
    manifest_path = report_path.parent / "visual_parity_manifest.json"
    manifest = json.loads(manifest_path.read_text())
    return {
        "command": cmd,
        "stdout_log": "visual_parity_stdout.log",
        "report": str(report_path),
        "manifest": str(manifest_path),
        "summary": summarize_visual_manifest(manifest),
    }


def parse_report_path(stdout):
    for line in stdout.splitlines():
        if "VISUAL PARITY PASS" in line and "report=" in line:
            return pathlib.Path(line.split("report=", 1)[1].strip()).resolve()
    raise RuntimeError(f"could not find visual parity report path in output:\n{stdout}")


def summarize_visual_manifest(manifest):
    rows = []
    for fixture in manifest.get("fixtures", []):
        clicks = fixture.get("chrome_click_verification", {})
        policy = clicks.get("candidate_policy", {})
        rows.append(
            {
                "fixture": fixture.get("fixture", ""),
                "verdict": fixture.get("diff_classification", {}).get("verdict", ""),
                "hit_passed": clicks.get("passed", 0),
                "hit_total": clicks.get("total", 0),
                "hit_skipped": policy.get("saccade_actions_skipped", 0),
                "diff_ratio": fixture.get("metrics", {}).get("diff_ratio", 0),
            }
        )
    return rows


def run_native_captures(args, run_dir, target_url):
    if args.skip_native:
        return []
    captures = []
    for browser in args.native_browsers:
        output_dir = run_dir / "native" / browser
        cmd = [
            str(WORKSPACE / "scripts" / "capture_native_browser_ui.py"),
            "--browser",
            browser,
            "--url",
            target_url,
            "--output-dir",
            str(output_dir),
            "--width",
            str(args.width),
            "--height",
            str(args.height + 80),
            "--allow-failure",
        ]
        result = subprocess.run(
            cmd,
            cwd=WORKSPACE,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=45,
        )
        output_dir.mkdir(parents=True, exist_ok=True)
        (output_dir / "capture_stdout.log").write_text(result.stdout)
        manifest_path = output_dir / "native_browser_ui_manifest.json"
        if manifest_path.exists():
            manifest = json.loads(manifest_path.read_text())
        else:
            manifest = {
                "engine": "saccade-native-browser-ui-capture-v0",
                "browser": browser,
                "status": "capture_failed",
                "error": {"message": result.stdout.strip() or "native capture produced no manifest"},
            }
            manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        captures.append(
            {
                "browser": browser,
                "command": cmd,
                "stdout_log": str(output_dir / "capture_stdout.log"),
                "manifest": str(manifest_path),
                "status": manifest.get("status", "unknown"),
                "screenshot": manifest.get("screenshot"),
                "error": manifest.get("error"),
            }
        )
    return captures


def write_demo_html(run_dir, manifest):
    native_cards = "\n".join(native_card(run_dir, item) for item in manifest["native_browser_ui"])
    visual_rows = "\n".join(visual_row(row) for row in manifest["visual_parity"]["summary"])
    visual_report = rel(run_dir, pathlib.Path(manifest["visual_parity"]["report"]))
    page = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Saccade Demo Comparison Pack</title>
  <style>
    body {{ margin: 0; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #f6f8fb; color: #162033; }}
    header {{ padding: 28px 34px; background: #ffffff; border-bottom: 1px solid #d8dee8; }}
    main {{ padding: 24px 34px; }}
    h1 {{ margin: 0 0 8px; }}
    .muted {{ color: #64748b; }}
    .grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }}
    .card {{ background: #ffffff; border: 1px solid #d8dee8; border-radius: 8px; overflow: hidden; }}
    .card h3 {{ margin: 0; padding: 12px 14px; border-bottom: 1px solid #d8dee8; }}
    .card p {{ padding: 0 14px; }}
    .card img {{ display: block; width: 100%; height: auto; border-top: 1px solid #d8dee8; }}
    table {{ width: 100%; border-collapse: collapse; background: #ffffff; border: 1px solid #d8dee8; }}
    th, td {{ text-align: left; padding: 10px 12px; border-bottom: 1px solid #d8dee8; }}
    code {{ background: #eef2f7; padding: 2px 5px; border-radius: 4px; }}
    .warning {{ color: #92400e; }}
    @media (max-width: 900px) {{ .grid {{ grid-template-columns: 1fr; }} }}
  </style>
</head>
<body>
  <header>
    <h1>Saccade Demo Comparison Pack</h1>
    <p class="muted">Target: <code>{esc(manifest['target_url'])}</code></p>
    <p class="muted">Native browser UI screenshots are public-demo artifacts. Saccade truth, safety, and verified actions remain separate evidence.</p>
  </header>
  <main>
    <h2>Native Browser UI</h2>
    <div class="grid">{native_cards}</div>

    <h2>Verified Saccade Evidence</h2>
    <p>Visual parity report: <a href="{esc(visual_report)}">{esc(visual_report)}</a></p>
    <table>
      <thead><tr><th>Fixture</th><th>Verdict</th><th>Chrome Hit-Test</th><th>Diff Ratio</th></tr></thead>
      <tbody>{visual_rows}</tbody>
    </table>
  </main>
</body>
</html>
"""
    (run_dir / "demo_review.html").write_text(page)


def native_card(run_dir, item):
    status = item.get("status", "unknown")
    title = item.get("browser", "browser")
    screenshot = item.get("screenshot")
    if status == "captured" and screenshot and pathlib.Path(screenshot).exists():
        image = f'<img src="{esc(rel(run_dir, pathlib.Path(screenshot)))}" alt="{esc(title)} native browser UI screenshot">'
        body = f"<p>Status: <strong>{esc(status)}</strong></p>{image}"
    else:
        error = item.get("error") or {}
        body = (
            f'<p>Status: <strong class="warning">{esc(status)}</strong></p>'
            f'<p class="muted">{esc(error.get("message", "native capture unavailable"))}</p>'
            "<p class=\"muted\">Grant macOS Screen Recording permission to the terminal/Codex app, then rerun the pack.</p>"
        )
    return f'<section class="card"><h3>{esc(title)}</h3>{body}</section>'


def visual_row(row):
    return (
        "<tr>"
        f"<td>{esc(row['fixture'])}</td>"
        f"<td>{esc(row['verdict'])}</td>"
        f"<td>{row['hit_passed']}/{row['hit_total']} skipped={row['hit_skipped']}</td>"
        f"<td>{row['diff_ratio']}</td>"
        "</tr>"
    )


def rel(base, path):
    return os.path.relpath(path.resolve(), base.resolve())


def esc(value):
    return html.escape(str(value), quote=True)


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"demo comparison pack failed: {error}", file=sys.stderr)
        sys.exit(1)
