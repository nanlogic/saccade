#!/usr/bin/env python3
"""Measure media codec support in the pinned CEF binary without a live site."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
from typing import Any

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = ROOT / "test_pages" / "media_capabilities" / "index.html"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def parse_capabilities(text: str) -> dict[str, bool | str]:
    result: dict[str, bool | str] = {}
    for line in text.splitlines():
        if not line.startswith("MEDIA_CAPABILITY ") or "=" not in line:
            continue
        key, value = line.removeprefix("MEDIA_CAPABILITY ").split("=", 1)
        if value == "true":
            result[key] = True
        elif value == "false":
            result[key] = False
        else:
            result[key] = value
    return result


def main() -> int:
    args = parse_args()
    app = args.app.resolve()
    fixture = args.fixture.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    executable = app / "Contents" / "MacOS" / "Saccade"
    work = pathlib.Path(tempfile.mkdtemp(prefix="saccade-media-capability-"))
    profile = work / "profile"
    profile.mkdir(mode=0o700)
    socket_path = work / "control.sock"
    grant_path = work / "grant.json"
    log_path = output / "browser.log"
    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    report: dict[str, Any] = {
        "schema": "saccade-cef-media-capability-v1",
        "app": str(app),
        "fixture": str(fixture),
        "ign_observed_manifest": {
            "container": "HLS",
            "video_codec": "avc1.64001f",
            "audio_codec": "mp4a.40.2",
            "evidence": "runs/chrome_reference/ign_1781476351/chrome_network.json",
        },
    }
    try:
        env = os.environ.copy()
        env.update(
            {
                "SACCADE_ENGINE_SOCKET": str(socket_path),
                "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
                "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            }
        )
        with log_path.open("wb") as log:
            process = subprocess.Popen(
                [
                    str(executable),
                    f"--url={fixture.as_uri()}",
                    f"--user-data-dir={profile}",
                    "--incognito",
                    "--use-mock-keychain",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--disable-background-networking",
                    "--window-size=960,700",
                ],
                cwd=ROOT,
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
        grant = wait_for_grant(grant_path, process, args.timeout_sec)
        control = EngineControl(
            pathlib.Path(grant["control_endpoint"]["path"]),
            grant["control_capability"]["token"],
        )
        truth = wait_for_collector(control, args.timeout_sec)
        article = control.call(
            "article_text", {"basis_page_revision": int(truth["page_revision"])}
        )
        capabilities = parse_capabilities(str(article.get("text") or ""))
        codec_gap = (
            capabilities.get("mse_h264") is False
            and capabilities.get("mse_aac") is False
            and capabilities.get("mse_vp9") is True
            and capabilities.get("mse_opus") is True
        )
        checks = {
            "fixture_loaded": truth.get("collector_ready") is True,
            "capabilities_collected": len(capabilities) >= 8,
            "ign_codec_gap_identified": codec_gap,
        }
        report.update(
            {
                "capabilities": capabilities,
                "diagnosis": (
                    "pinned_cef_proprietary_codec_gap"
                    if codec_gap
                    else "not_explained_by_basic_codec_capabilities"
                ),
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

    report_path = output / "report.json"
    report_path.write_text(json.dumps(report, indent=2) + "\n")
    print(f"CEF_MEDIA_CAPABILITY verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
