#!/usr/bin/env python3
"""Verify company metadata and the native Saccade Help menu route."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import plistlib
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

from probe_cef_layout_epoch import wait_for_grant, wait_until
from probe_cef_truth_reflex import EngineControl


ROOT = pathlib.Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def apple_script(lines: list[str]) -> str:
    command = ["osascript"]
    for line in lines:
        command.extend(["-e", line])
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip())
    return result.stdout.strip()


def click_help() -> None:
    apple_script(
        [
            'tell application "System Events"',
            'set targetProcess to first application process whose bundle identifier is "ai.saccade.browser"',
            'set frontmost of targetProcess to true',
            'click menu item "Saccade Help — nanlogic.com" of menu "Help" of menu bar 1 of targetProcess',
            'end tell',
        ]
    )


def resolve_help_url(url: str) -> tuple[int, str]:
    current = url
    for _ in range(4):
        request = urllib.request.Request(
            current,
            headers={"User-Agent": "Saccade release help gate"},
            method="GET",
        )
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return int(response.status), str(response.url)
        except urllib.error.HTTPError as error:
            location = error.headers.get("Location")
            if 300 <= error.code < 400 and location:
                current = urllib.parse.urljoin(current, location)
                continue
            raise
    raise RuntimeError(f"too many Help URL redirects: {current}")


def main() -> int:
    args = parse_args()
    package = args.package.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    app = package / "Saccade.app"
    executable = app / "Contents" / "MacOS" / "Saccade"
    version = json.loads((package / "VERSION.json").read_text())
    with (app / "Contents" / "Info.plist").open("rb") as source:
        plist = plistlib.load(source)
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-company-help-"))
    session = work / "session"
    profile = work / "profile"
    session.mkdir(mode=0o700)
    profile.mkdir(mode=0o700)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    pointer_path = work / "current-grant-path"
    log_path = output / "cef.log"
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    report: dict[str, Any] = {
        "schema": "saccade-company-help-gate-v1",
        "package": str(package),
    }
    try:
        env = os.environ.copy()
        env.update(
            {
                "SACCADE_ENGINE_SOCKET": str(socket_path),
                "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
                "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
                "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
                "SACCADE_ENGINE_INITIAL_TAB_GRANT": "1",
                "SACCADE_ENGINE_BROKER": "1",
                "SACCADE_PROFILE_MODE": "incognito",
                "SACCADE_PROFILE_NAME": "company-help-gate",
            }
        )
        with log_path.open("wb") as log:
            process = subprocess.Popen(
                [
                    str(executable),
                    "--url=https://example.com/",
                    f"--user-data-dir={profile}",
                    "--incognito",
                    "--use-mock-keychain",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--window-size=1200,820",
                ],
                cwd=ROOT,
                env=env,
                stdout=log,
                stderr=log,
            )
        grant = wait_for_grant(grant_path, args.timeout_sec)
        control = EngineControl(
            socket_path, str(grant["control_capability"]["token"])
        )
        wait_until(
            lambda: control.call("shell_status").get("collector_ready") is True,
            args.timeout_sec,
            "waiting for initial Saccade tab",
        )
        before = control.call("tab_registry")
        click_help()

        after = wait_until(
            lambda: (
                current
                if int((current := control.call("tab_registry")).get("browser_count", 0))
                == int(before.get("browser_count", 0)) + 1
                else None
            ),
            args.timeout_sec,
            "waiting for Help tab",
        )
        help_status, resolved_help_url = resolve_help_url(str(version["help_url"]))
        checks = {
            "publisher_name": version.get("publisher_name") == "NaN Logic LLC"
            and plist.get("SaccadePublisherName") == "NaN Logic LLC",
            "publisher_url": version.get("publisher_url")
            == "https://nanlogic.com/"
            and plist.get("SaccadePublisherURL") == "https://nanlogic.com/",
            "help_url": version.get("help_url") == "https://nanlogic.com/"
            and plist.get("SaccadeHelpURL") == "https://nanlogic.com/",
            "help_opened_one_saccade_tab": int(after.get("browser_count", 0))
            == int(before.get("browser_count", 0)) + 1,
            "help_tab_remained_agent_off": int(after.get("eligible_count", 0))
            == int(before.get("eligible_count", 0))
            and after.get("agent_off_tabs_omitted") is True,
            "nanlogic_help_endpoint_reachable": 200 <= help_status < 400
            and "nanlogic.com" in resolved_help_url,
        }
        report.update(
            {
                "app_build": version.get("app_build"),
                "help_http_status": help_status,
                "resolved_help_url": resolved_help_url,
                "browser_count_before": before.get("browser_count"),
                "browser_count_after": after.get("browser_count"),
                "eligible_count_before": before.get("eligible_count"),
                "eligible_count_after": after.get("eligible_count"),
                "checks": checks,
                "verdict": "PASS" if all(checks.values()) else "FAIL",
            }
        )
    except Exception as error:
        report.update({"verdict": "FAIL", "error": str(error)})
    finally:
        if control is not None:
            try:
                control.call("close")
            except Exception:
                pass
        if process is not None:
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
        shutil.rmtree(work, ignore_errors=True)

    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"CEF_COMPANY_HELP verdict={report['verdict']} "
        f"report={output / 'report.json'}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
