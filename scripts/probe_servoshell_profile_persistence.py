#!/usr/bin/env python3
"""Probe ServoShell profile cookie persistence with a local HTTP fixture."""

from __future__ import annotations

import argparse
import http.server
import json
import shutil
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE_SERVOSHELL = Path(
    "/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell"
)
OFFICIAL_SERVOSHELL = Path("/Applications/Servo.app/Contents/MacOS/servoshell")
SACCADE_SERVOSHELL = ROOT / "target/debug/saccade-servoshell"


HTML = b"""<!doctype html>
<html>
<head><meta charset="utf-8"><title>Saccade Profile Probe</title></head>
<body>
  <h1>Saccade Profile Probe</h1>
  <script>
    if (new URL(location.href).searchParams.get("set") === "1") {
      document.cookie = "saccade_persist=present; Max-Age=3600; SameSite=Lax; Path=/";
      document.cookie = "saccade_session=present; SameSite=Lax; Path=/";
    }
    window.__saccadeProfileProbe = {
      cookieNames() {
        return document.cookie.split(";").map((item) => item.trim().split("=")[0]).filter(Boolean).sort();
      }
    };
  </script>
</body>
</html>
"""


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802 - stdlib API name.
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        if "set=1" in self.path:
            self.send_header("Set-Cookie", "saccade_header_persist=present; Max-Age=3600; SameSite=Lax; Path=/")
            self.send_header("Set-Cookie", "saccade_header_session=present; SameSite=Lax; Path=/")
        self.end_headers()
        self.wfile.write(HTML)

    def log_message(self, fmt, *args):  # noqa: A003 - stdlib API name.
        return


def unix_ms() -> int:
    return int(time.time() * 1000)


def webdriver_request(port: int, method: str, path: str, payload=None, timeout: float = 10.0):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"} if payload is not None else {}
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        text = response.read().decode("utf-8", "replace")
        return response.status, json.loads(text) if text else None


def wait_for_status(port: int, proc: subprocess.Popen, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last_error = None
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"servoshell exited before WebDriver was ready: {proc.returncode}")
        try:
            return webdriver_request(port, "GET", "/status", timeout=0.5)
        except Exception as error:  # noqa: BLE001
            last_error = repr(error)
            time.sleep(0.25)
    raise TimeoutError(f"WebDriver status was not ready; last_error={last_error}")


def session_id(response) -> str:
    value = response.get("value") if isinstance(response, dict) else None
    if isinstance(value, dict) and isinstance(value.get("sessionId"), str):
        return value["sessionId"]
    if isinstance(response.get("sessionId"), str):
        return response["sessionId"]
    raise RuntimeError(f"missing session id: {response}")


def execute(port: int, sid: str, script: str):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{sid}/execute/sync",
        {"script": script, "args": []},
    )
    return body.get("value") if isinstance(body, dict) else body


def run_servoshell_once(
    *,
    servoshell: Path,
    profile_dir: Path,
    webdriver_port: int,
    url: str,
    timeout_sec: float,
) -> dict:
    cmd = [
        str(servoshell),
        f"--webdriver={webdriver_port}",
        f"--config-dir={profile_dir.resolve()}",
        url,
    ]
    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    sid = None
    report = {"cmd": cmd, "url": url, "webdriver_port": webdriver_port}
    try:
        wait_for_status(webdriver_port, proc, timeout_sec)
        _, body = webdriver_request(
            webdriver_port,
            "POST",
            "/session",
            {"capabilities": {"alwaysMatch": {"browserName": "servo"}}},
        )
        sid = session_id(body)
        deadline = time.monotonic() + timeout_sec
        ready = None
        while time.monotonic() < deadline:
            ready = execute(
                webdriver_port,
                sid,
                "return {readyState: document.readyState, hasProbe: !!window.__saccadeProfileProbe};",
            )
            if ready.get("readyState") in {"interactive", "complete"} and ready.get("hasProbe"):
                break
            time.sleep(0.25)
        names = execute(
            webdriver_port,
            sid,
            "return window.__saccadeProfileProbe.cookieNames();",
        )
        report.update({"ready": ready, "cookie_names": names, "ok": True})
    except Exception as error:  # noqa: BLE001
        report.update({"error": repr(error), "ok": False})
    finally:
        if sid:
            try:
                webdriver_request(webdriver_port, "DELETE", f"/session/{sid}/servo/shutdown", timeout=3)
            except Exception:
                pass
        try:
            stdout, stderr = proc.communicate(timeout=8)
        except subprocess.TimeoutExpired:
            if sid:
                try:
                    webdriver_request(webdriver_port, "DELETE", f"/session/{sid}", timeout=3)
                except Exception:
                    pass
            if proc.poll() is None:
                proc.terminate()
            try:
                stdout, stderr = proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                stdout, stderr = proc.communicate()
        except Exception:
            proc.kill()
            stdout, stderr = proc.communicate()
        report["returncode"] = proc.returncode
        report["stdout_head"] = stdout.splitlines()[:30]
        report["stderr_head"] = stderr.splitlines()[:60]
    return report


def cookie_jar_names(profile_dir: Path) -> list[dict]:
    jar_path = profile_dir / "cookie_jar.json"
    if not jar_path.exists():
        return []
    try:
        data = json.loads(jar_path.read_text())
    except Exception:
        return []
    output = []
    cookies_map = data.get("cookies_map") if isinstance(data, dict) else None
    if not isinstance(cookies_map, dict):
        return output
    for domain, cookies in sorted(cookies_map.items()):
        for item in cookies or []:
            raw = str(item.get("cookie") or "")
            name = raw.split("=", 1)[0]
            if name.startswith("saccade_"):
                output.append(
                    {
                        "domain": domain,
                        "name": name,
                        "persistent": item.get("persistent"),
                        "host_only": item.get("host_only"),
                        "has_expiry": bool(item.get("expiry_time")),
                    }
                )
    return output


def wait_cookie_jar_names(profile_dir: Path, timeout_sec: float = 5.0) -> list[dict]:
    deadline = time.monotonic() + timeout_sec
    names = []
    while time.monotonic() < deadline:
        names = cookie_jar_names(profile_dir)
        if names:
            return names
        time.sleep(0.25)
    return names


def run_probe(label: str, servoshell: Path, fixture_port: int, output_dir: Path, timeout_sec: float):
    profile_dir = output_dir / f"profile_{label}"
    if profile_dir.exists():
        shutil.rmtree(profile_dir)
    profile_dir.mkdir(parents=True)
    first = run_servoshell_once(
        servoshell=servoshell,
        profile_dir=profile_dir,
        webdriver_port=fixture_port + 10,
        url=f"http://127.0.0.1:{fixture_port}/?set=1",
        timeout_sec=timeout_sec,
    )
    jar_after_first = wait_cookie_jar_names(profile_dir)
    second = run_servoshell_once(
        servoshell=servoshell,
        profile_dir=profile_dir,
        webdriver_port=fixture_port + 11,
        url=f"http://127.0.0.1:{fixture_port}/",
        timeout_sec=timeout_sec,
    )
    jar_after_second = wait_cookie_jar_names(profile_dir)
    second_names = set(second.get("cookie_names") or [])
    return {
        "label": label,
        "servoshell": str(servoshell),
        "profile_dir": str(profile_dir),
        "first": first,
        "jar_after_first": jar_after_first,
        "second": second,
        "jar_after_second": jar_after_second,
        "persistent_cookie_reused": "saccade_persist" in second_names,
        "session_cookie_reused": "saccade_session" in second_names,
        "header_persistent_cookie_reused": "saccade_header_persist" in second_names,
        "header_session_cookie_reused": "saccade_header_session" in second_names,
    }


def run_bridge_once(
    *,
    label: str,
    servoshell: Path,
    profile_dir: Path,
    fixture_url: str,
    output_dir: Path,
    timeout_sec: float,
) -> dict:
    cmd = [
        str(SACCADE_SERVOSHELL),
        "bridge",
        "--servoshell",
        str(servoshell),
        "--url",
        fixture_url,
        "--profile-dir",
        str(profile_dir),
        "--output-dir",
        str(output_dir),
        "--exit",
        "--json",
        "--timeout-sec",
        str(timeout_sec),
    ]
    proc = subprocess.run(
        cmd,
        cwd=str(ROOT),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout_sec + 25,
        check=False,
    )
    report_path = output_dir / "report.json"
    report = None
    if report_path.exists():
        try:
            report = json.loads(report_path.read_text())
        except Exception:  # noqa: BLE001
            report = None
    process = report.get("process") if isinstance(report, dict) else None
    return {
        "label": label,
        "cmd": cmd,
        "returncode": proc.returncode,
        "stdout_head": proc.stdout.splitlines()[:40],
        "stderr_head": proc.stderr.splitlines()[:80],
        "report_path": str(report_path),
        "bridge_ok": bool(report.get("ok")) if isinstance(report, dict) else False,
        "process": process,
    }


def run_bridge_probe(label: str, servoshell: Path, fixture_port: int, output_dir: Path, timeout_sec: float):
    profile_dir = output_dir / f"profile_bridge_{label}"
    if profile_dir.exists():
        shutil.rmtree(profile_dir)
    profile_dir.mkdir(parents=True)
    first = run_bridge_once(
        label=f"bridge_{label}",
        servoshell=servoshell,
        profile_dir=profile_dir,
        fixture_url=f"http://127.0.0.1:{fixture_port}/?set=1",
        output_dir=output_dir / f"bridge_{label}_set",
        timeout_sec=timeout_sec,
    )
    jar_after_first = wait_cookie_jar_names(profile_dir)
    second = run_servoshell_once(
        servoshell=servoshell,
        profile_dir=profile_dir,
        webdriver_port=fixture_port + 30,
        url=f"http://127.0.0.1:{fixture_port}/",
        timeout_sec=timeout_sec,
    )
    second_names = set(second.get("cookie_names") or [])
    return {
        "label": f"bridge_{label}",
        "servoshell": str(servoshell),
        "profile_dir": str(profile_dir),
        "first": first,
        "jar_after_first": jar_after_first,
        "second": second,
        "persistent_cookie_reused": "saccade_persist" in second_names,
        "session_cookie_reused": "saccade_session" in second_names,
        "header_persistent_cookie_reused": "saccade_header_persist" in second_names,
        "header_session_cookie_reused": "saccade_header_session" in second_names,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timeout-sec", type=float, default=15.0)
    parser.add_argument("--fixture-port", type=int, default=7765)
    args = parser.parse_args()

    output_dir = args.output_dir or ROOT / "runs/profile_persistence" / f"servoshell_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)

    server = socketserver.TCPServer(("127.0.0.1", args.fixture_port), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    report = {
        "output_dir": str(output_dir),
        "fixture": f"http://127.0.0.1:{args.fixture_port}/",
        "probes": [],
    }
    try:
        report["probes"].append(
            run_probe("source_release", SOURCE_SERVOSHELL, args.fixture_port, output_dir, args.timeout_sec)
        )
        if OFFICIAL_SERVOSHELL.exists():
            report["probes"].append(
                run_probe("official_app", OFFICIAL_SERVOSHELL, args.fixture_port, output_dir, args.timeout_sec)
            )
        if SACCADE_SERVOSHELL.exists():
            report["probes"].append(
                run_bridge_probe(
                    "source_release",
                    SOURCE_SERVOSHELL,
                    args.fixture_port,
                    output_dir,
                    args.timeout_sec,
                )
            )
    finally:
        server.shutdown()
        server.server_close()

    report["ok"] = all(probe.get("header_persistent_cookie_reused") for probe in report["probes"])
    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        "SERVOSHELL_PROFILE_PERSISTENCE "
        f"ok={str(report['ok']).lower()} report={report_path}"
    )
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
