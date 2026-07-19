#!/usr/bin/env python3
"""Verify packaged Saccade profile status and value-free deletion controls."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import tempfile
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    return parser.parse_args()


def run(command: list[str], env: dict[str, str]) -> dict[str, Any]:
    result = subprocess.run(
        command,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=10,
    )
    return {
        "return_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def main() -> int:
    args = parse_args()
    release = args.release_dir.resolve()
    status_command = (
        release / "Saccade.app" / "Contents" / "MacOS" / "saccade-profile-status"
    )
    clear_command = (
        release / "Saccade.app" / "Contents" / "MacOS" / "saccade-clear-profile"
    )
    outer_status = release / "bin" / "profile-status"
    outer_clear = release / "bin" / "clear-profile"
    if not all(
        path.is_file()
        for path in (status_command, clear_command, outer_status, outer_clear)
    ):
        raise SystemExit("release is missing in-app or package profile controls")

    protected = "RAW-COOKIE-VALUE-MUST-NOT-APPEAR"
    checks: dict[str, bool] = {}
    evidence: dict[str, Any] = {}
    with tempfile.TemporaryDirectory(prefix="saccade-profile-controls-") as home:
        env = os.environ.copy()
        env["HOME"] = home
        profile = (
            pathlib.Path(home)
            / "Library"
            / "Application Support"
            / "Saccade"
            / "CEF"
            / "Profiles"
            / "default"
        )
        (profile / "Network").mkdir(parents=True)
        (profile / "Network" / "Cookies").write_text(protected)
        (profile / "Local Storage").mkdir()
        (profile / "Local Storage" / "state").write_text(protected)

        before = run([str(status_command)], env)
        dry_run = run([str(clear_command), "--dry-run"], env)
        checks["status_reports_persistent_profile"] = (
            before["return_code"] == 0
            and "exists=true" in before["stdout"]
            and "cookies_exposed_to_agent=false" in before["stdout"]
        )
        checks["package_and_in_app_controls_present"] = all(
            os.access(path, os.X_OK)
            for path in (status_command, clear_command, outer_status, outer_clear)
        )
        checks["dry_run_preserves_profile"] = (
            dry_run["return_code"] == 0
            and profile.is_dir()
            and "cleared=false" in dry_run["stdout"]
        )
        checks["outputs_are_value_free"] = protected not in json.dumps(
            [before, dry_run]
        )

        cleared = run([str(clear_command), "--yes"], env)
        after = run([str(status_command)], env)
        checks["confirmed_clear_removes_profile"] = (
            cleared["return_code"] == 0
            and not profile.exists()
            and "cleared=true" in cleared["stdout"]
            and "exists=false" in after["stdout"]
        )
        checks["clear_output_is_value_free"] = protected not in json.dumps(
            [cleared, after]
        )

        invalid_env = env.copy()
        invalid_env["SACCADE_PROFILE_NAME"] = "../outside"
        invalid = run([str(clear_command), "--yes"], invalid_env)
        checks["invalid_profile_name_rejected"] = invalid["return_code"] != 0

        outside = pathlib.Path(home) / "outside-profile"
        outside.mkdir()
        sentinel = outside / "keep"
        sentinel.write_text(protected)
        profile.parent.mkdir(parents=True, exist_ok=True)
        profile.symlink_to(outside, target_is_directory=True)
        symlinked = run([str(clear_command), "--yes"], env)
        checks["symlinked_profile_rejected"] = (
            symlinked["return_code"] != 0 and sentinel.read_text() == protected
        )

        evidence = {
            "before": before,
            "dry_run": dry_run,
            "cleared": cleared,
            "after": after,
            "invalid": invalid,
            "symlinked": symlinked,
        }

    sanitized = json.loads(json.dumps(evidence).replace(protected, "[REDACTED]"))
    verdict = "PASS" if checks and all(checks.values()) else "FAIL"
    report = {
        "schema": "saccade-cef-profile-controls-v1",
        "verdict": verdict,
        "release_dir": str(release),
        "checks": checks,
        "evidence": sanitized,
        "raw_cookie_or_storage_value_exposed": False,
    }
    args.output_dir.mkdir(parents=True, exist_ok=True)
    report_path = args.output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"CEF_PROFILE_CONTROLS verdict={verdict} report={report_path.resolve()}")
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
