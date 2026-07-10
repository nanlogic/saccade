#!/usr/bin/env python3
"""Run a visible, persistent Chrome compatibility gate with redacted truth.

This is an explicit fallback for pages that the primary Servo engine cannot
load. It does not hide automation, solve challenges, or export profile data.
"""

import argparse
import base64
import json
import pathlib
import subprocess
import sys
import time

import chrome_reference_cdp as reference


STATUS_JS = r"""
JSON.stringify((() => {
  const body = document.body;
  const text = body ? String(body.innerText || body.textContent || "") : "";
  const normalized = text.toLowerCase();
  const title = String(document.title || "");
  const challenge =
    title.toLowerCase().includes("just a moment") ||
    normalized.includes("verify you are human") ||
    normalized.includes("checking your browser") ||
    normalized.includes("performing security verification") ||
    !!document.querySelector("script[src*='challenges.cloudflare.com'], #challenge-running, .cf-challenge");
  return {
    url: location.href,
    title,
    readyState: document.readyState,
    bodyTextLength: text.trim().length,
    challenge,
    navigatorWebdriver: navigator.webdriver === true
  };
})())
"""


def parse_args():
    parser = argparse.ArgumentParser(
        description="Open a visible persistent Chrome compatibility session and emit redacted truth."
    )
    parser.add_argument("url")
    parser.add_argument("output_dir")
    parser.add_argument("--profile-dir", default="runs/chrome_compat_profile/default")
    parser.add_argument("--width", type=int, default=1440)
    parser.add_argument("--height", type=int, default=1000)
    parser.add_argument("--timeout-sec", type=float, default=30.0)
    parser.add_argument(
        "--human-wait-sec",
        type=float,
        default=60.0,
        help="Visible wait for a user-owned challenge step; Saccade never clicks it.",
    )
    parser.add_argument("--settle-ms", type=int, default=1500)
    parser.add_argument(
        "--keep-open",
        action="store_true",
        help="Keep the visible browser open and refresh redacted truth as the user navigates.",
    )
    parser.add_argument("--poll-ms", type=int, default=1000)
    parser.add_argument(
        "--grant-current-tab",
        action="store_true",
        help="Write an explicit Human current-tab grant and start the loopback compatibility bridge.",
    )
    parser.add_argument(
        "--grant-path",
        help="Path for the explicit current-tab grant artifact; required with --grant-current-tab.",
    )
    parser.add_argument(
        "--screenshot",
        action="store_true",
        help="Capture visible pixels. Use only for public, non-sensitive pages.",
    )
    args = parser.parse_args()
    if args.grant_current_tab and not args.keep_open:
        parser.error("--grant-current-tab requires --keep-open")
    if args.grant_current_tab and not args.grant_path:
        parser.error("--grant-current-tab requires --grant-path")
    return args


def evaluate_json(client, expression):
    result = client.call(
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": True},
        timeout=10,
    )
    value = result.get("result", {}).get("value", "{}")
    return json.loads(value)


def sanitize_truth(truth):
    if not isinstance(truth, dict):
        return truth
    if isinstance(truth.get("url"), str):
        truth["url"] = reference.safe_url(truth["url"])
    for action in truth.get("actions", []):
        if not isinstance(action, dict):
            continue
        label = action.get("label")
        if isinstance(label, str) and label.startswith(("http://", "https://")):
            action["label"] = reference.safe_url(label)
    return truth


def launch_chrome(chrome, port, profile_dir, width, height, url):
    command = [
        chrome,
        f"--user-data-dir={profile_dir}",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={port}",
        f"--window-size={width},{height}",
        "--no-first-run",
        "--no-default-browser-check",
        url,
    ]
    return command, subprocess.Popen(
        command,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def wait_for_page(client, timeout_sec, human_wait_sec):
    deadline = time.monotonic() + timeout_sec
    challenge_seen = False
    status = None
    ready_since = None
    while time.monotonic() < deadline:
        status = evaluate_json(client, STATUS_JS)
        challenge_seen = challenge_seen or bool(status.get("challenge"))
        if status.get("readyState") == "complete" and not status.get("challenge"):
            ready_since = ready_since or time.monotonic()
            if time.monotonic() - ready_since >= 0.5:
                return status, challenge_seen
        else:
            ready_since = None
        time.sleep(0.25)

    if not status or not status.get("challenge") or human_wait_sec <= 0:
        return status, challenge_seen

    human_deadline = time.monotonic() + human_wait_sec
    while time.monotonic() < human_deadline:
        status = evaluate_json(client, STATUS_JS)
        challenge_seen = True
        if status.get("readyState") == "complete" and not status.get("challenge"):
            return status, challenge_seen
        time.sleep(0.5)
    return status, challenge_seen


def terminate(process):
    if not process:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def start_control_bridge(args, cdp_port):
    script = pathlib.Path(__file__).with_name("chrome_compat_control.py")
    return subprocess.Popen(
        [
            sys.executable,
            str(script),
            "--cdp-port",
            str(cdp_port),
            "--output-dir",
            str(pathlib.Path(args.output_dir).resolve()),
            "--grant-path",
            str(pathlib.Path(args.grant_path).resolve()),
            "--initial-url",
            reference.safe_url(args.url),
            "--timeout-sec",
            str(args.timeout_sec),
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def wait_for_file(path, process, timeout_sec):
    deadline = time.monotonic() + timeout_sec
    while time.monotonic() < deadline:
        if path.exists():
            return True
        if process.poll() is not None:
            return False
        time.sleep(0.05)
    return path.exists()


def attach_control_metadata(report, grant_path):
    if grant_path is None:
        return report
    report["grant_path"] = str(grant_path)
    if grant_path.exists():
        try:
            grant = json.loads(grant_path.read_text())
            report["control_endpoint"] = grant.get("control_endpoint")
            report["grant_status"] = grant.get("status")
        except (OSError, json.JSONDecodeError):
            report["grant_status"] = "unreadable"
    else:
        report["grant_status"] = "not_ready"
    return report


def write_json_atomic(path, payload):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def build_report(args, status, challenge_seen, truth, started, profile_dir, paths, command):
    ok = bool(status) and status.get("readyState") == "complete" and not status.get("challenge")
    safe_command = [*command[:-1], reference.safe_url(command[-1])] if command else []
    return {
        "ok": ok,
        "engine": "chrome-compat-cdp-v0",
        "route": "compatibility" if ok else "provider_challenge" if status and status.get("challenge") else "not_ready",
        "url": reference.safe_url((status or {}).get("url", args.url)),
        "title": (status or {}).get("title", ""),
        "ready_state": (status or {}).get("readyState"),
        "body_text_length": (status or {}).get("bodyTextLength", 0),
        "challenge_seen": challenge_seen,
        "navigator_webdriver": (status or {}).get("navigatorWebdriver"),
        "elapsed_ms": round((time.monotonic() - started) * 1000),
        "profile_dir": str(profile_dir),
        "profile_persistent": True,
        "live_follow": args.keep_open,
        "truth_stale": truth is None,
        "cookies_exported": False,
        "storage_exported": False,
        "sensitive_values_exported": False,
        "actions": len((truth or {}).get("actions", [])),
        "artifacts": {
            "truth": str(paths["truth"]) if truth is not None else None,
            "screenshot": (
                str(paths["screenshot"])
                if args.screenshot and paths["screenshot"].exists()
                else None
            ),
            "stderr": None,
        },
        "command": safe_command,
        "updated_at_unix_ms": round(time.time() * 1000),
    }


def main():
    args = parse_args()
    output_dir = pathlib.Path(args.output_dir).resolve()
    profile_dir = pathlib.Path(args.profile_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    profile_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "report.json"
    truth_path = output_dir / "truth.json"
    screenshot_path = output_dir / "page.png"
    truth_path.unlink(missing_ok=True)
    if not args.screenshot:
        screenshot_path.unlink(missing_ok=True)
    chrome = reference.find_chrome()
    port = reference.free_port()
    process = None
    client = None
    started = time.monotonic()
    command = []
    control_process = None
    grant_path = pathlib.Path(args.grant_path).resolve() if args.grant_current_tab else None
    exit_code = 1
    try:
        command, process = launch_chrome(
            chrome,
            port,
            profile_dir,
            args.width,
            args.height,
            args.url,
        )
        _, client = reference.wait_for_cdp_client(port, args.timeout_sec)
        client.call("Page.enable")
        client.call("Runtime.enable")
        status, challenge_seen = wait_for_page(client, args.timeout_sec, args.human_wait_sec)
        if status and not status.get("challenge") and args.settle_ms > 0:
            client.drain(args.settle_ms / 1000)
            status = evaluate_json(client, STATUS_JS)
            if status.get("readyState") != "complete":
                status, challenge_seen_after_settle = wait_for_page(
                    client, args.timeout_sec, args.human_wait_sec
                )
                challenge_seen = challenge_seen or challenge_seen_after_settle

        ok = bool(status) and status.get("readyState") == "complete" and not status.get("challenge")
        truth = sanitize_truth(evaluate_json(client, reference.PROBE_JS)) if ok else None
        if truth is not None:
            write_json_atomic(truth_path, truth)
        if ok and args.screenshot:
            screenshot_path.write_bytes(base64.b64decode(reference.capture_screenshot(client)))

        paths = {"truth": truth_path, "screenshot": screenshot_path}
        report = build_report(args, status, challenge_seen, truth, started, profile_dir, paths, command)
        if ok and args.grant_current_tab:
            control_process = start_control_bridge(args, port)
            if not wait_for_file(grant_path, control_process, min(args.timeout_sec, 5)):
                raise RuntimeError("Chrome compatibility control bridge did not write its grant")
        attach_control_metadata(report, grant_path)
        write_json_atomic(report_path, report)
        print(json.dumps(report, indent=2, sort_keys=True))
        exit_code = 0 if ok else 1
        if not ok or not args.keep_open:
            return exit_code

        print(f"SACCADE COMPATIBILITY LIVE report={report_path} truth={truth_path}", flush=True)
        poll_seconds = max(0.25, args.poll_ms / 1000)
        while process.poll() is None:
            time.sleep(poll_seconds)
            try:
                status = evaluate_json(client, STATUS_JS)
                challenge_seen = challenge_seen or bool(status.get("challenge"))
                ready = status.get("readyState") == "complete" and not status.get("challenge")
                truth = (
                    sanitize_truth(evaluate_json(client, reference.PROBE_JS)) if ready else None
                )
                if truth is None:
                    truth_path.unlink(missing_ok=True)
                else:
                    write_json_atomic(truth_path, truth)
                report = build_report(
                    args, status, challenge_seen, truth, started, profile_dir, paths, command
                )
                attach_control_metadata(report, grant_path)
                write_json_atomic(report_path, report)
            except Exception as error:
                report["ok"] = False
                report["route"] = "browser_closed_or_unavailable"
                report["truth_stale"] = True
                report["last_error"] = str(error)
                report["artifacts"]["truth"] = None
                attach_control_metadata(report, grant_path)
                truth_path.unlink(missing_ok=True)
                write_json_atomic(report_path, report)
                break
        if process.poll() is not None:
            report["ok"] = False
            report["route"] = "browser_closed"
            report["truth_stale"] = True
            report["artifacts"]["truth"] = None
            attach_control_metadata(report, grant_path)
            truth_path.unlink(missing_ok=True)
            write_json_atomic(report_path, report)
        return exit_code
    except KeyboardInterrupt:
        truth_path.unlink(missing_ok=True)
        if "report" in locals() and args.keep_open:
            report["ok"] = False
            report["route"] = "browser_closed"
            report["truth_stale"] = True
            report["artifacts"]["truth"] = None
            attach_control_metadata(report, grant_path)
            write_json_atomic(report_path, report)
        return 0
    finally:
        if client:
            try:
                client.close()
            except Exception:
                pass
        terminate(control_process)
        terminate(process)


if __name__ == "__main__":
    raise SystemExit(main())
