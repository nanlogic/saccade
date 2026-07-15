#!/usr/bin/env python3
"""Verify CEF renders and controls the local Canvas game without CDP."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from typing import Any

from PIL import Image, ImageChops, ImageStat

from probe_cef_truth_reflex import EngineControl, wait_for_collector, wait_for_grant


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_APP = ROOT / "target" / "cef-release" / "Saccade.app"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    parser.add_argument("--drags", type=int, default=8)
    parser.add_argument("--surface-label")
    parser.add_argument("--expect-text", action="append", default=[])
    parser.add_argument("--allow-static-render", action="store_true")
    visibility = parser.add_mutually_exclusive_group()
    visibility.add_argument("--headed", dest="headed", action="store_true")
    visibility.add_argument("--hidden", dest="headed", action="store_false")
    parser.set_defaults(headed=True)
    return parser.parse_args()


def percentile_95(values: list[float]) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    return round(ordered[max(0, int(len(ordered) * 0.95 + 0.999) - 1)], 3)


def wait_for_surface(
    control: EngineControl,
    timeout: float,
    expected_label: str | None,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last = control.call("actions")
        for action in last.get("actions", []):
            if (
                action.get("role") == "surface"
                and action.get("kind") == "pointer_drag"
                and (expected_label is None or action.get("label") == expected_label)
            ):
                return action
        time.sleep(0.03)
    raise TimeoutError(f"visible Canvas surface was not discovered: {last}")


def capture(
    control: EngineControl,
    revision: int,
    destination: pathlib.Path,
) -> dict[str, Any]:
    result = control.call(
        "screenshot_audit",
        {"basis_page_revision": revision, "audit_requested": True},
    )
    source = pathlib.Path(result["screenshot_path"])
    shutil.copy2(source, destination)
    return result


def wait_for_expected_text(
    control: EngineControl,
    revision: int,
    expected: list[str],
    timeout: float,
) -> None:
    if not expected:
        return
    deadline = time.monotonic() + timeout
    missing = list(expected)
    while time.monotonic() < deadline:
        article = control.call("article_text", {"basis_page_revision": revision})
        value = str(article.get("text") or "")
        missing = [marker for marker in expected if marker not in value]
        if not missing:
            return
        time.sleep(0.05)
    raise AssertionError(f"expected local runtime markers were missing: {missing}")


def image_metrics(before: pathlib.Path, after: pathlib.Path) -> dict[str, Any]:
    first = Image.open(before).convert("RGB")
    second = Image.open(after).convert("RGB")
    if first.size != second.size:
        raise AssertionError(f"screenshot dimensions changed: {first.size} -> {second.size}")
    extrema = first.getextrema()
    channel_range = max(high - low for low, high in extrema)
    difference = ImageChops.difference(first, second)
    stat = ImageStat.Stat(difference)
    histogram = difference.convert("L").histogram()
    changed = sum(histogram[9:])
    total = max(1, first.width * first.height)
    return {
        "width": first.width,
        "height": first.height,
        "max_channel_range": channel_range,
        "mean_absolute_difference": round(sum(stat.mean) / 3.0, 3),
        "changed_pixel_fraction": round(changed / total, 6),
        "before_sha256": hashlib.sha256(before.read_bytes()).hexdigest(),
        "after_sha256": hashlib.sha256(after.read_bytes()).hexdigest(),
    }


def main() -> int:
    args = parse_args()
    if args.drags <= 0:
        raise SystemExit("--drags must be positive")
    executable = args.app / "Contents" / "MacOS" / "cefsimple"
    if not executable.is_file():
        raise SystemExit(f"missing CEF release app: {executable}")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    args.output_dir = args.output_dir.resolve()
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-cef-local-game-"))
    os.chmod(session, 0o700)
    socket_path = session / "control.sock"
    grant_path = session / "grant.json"
    replay_path = args.output_dir / "replay.jsonl"
    before_path = args.output_dir / "before.png"
    after_path = args.output_dir / "after.png"
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(socket_path),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_REFLEX_GATE": "1",
        }
    )
    command = [
        str(executable),
        f"--url={args.url}",
        f"--user-data-dir={session / 'profile'}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--use-mock-keychain",
        "--saccade-reflex-gate",
        "--window-size=1280,900",
    ]
    if not args.headed:
        command.append("--initial-show-state=hidden")

    process: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    browser_log = (args.output_dir / "browser.log").open("wb")
    started = time.monotonic()
    report: dict[str, Any]
    try:
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=env,
            stdout=browser_log,
            stderr=subprocess.STDOUT,
        )
        grant = wait_for_grant(grant_path, process, args.timeout_sec)
        control = EngineControl(
            pathlib.Path(grant["control_endpoint"]["path"]),
            grant["control_capability"]["token"],
        )
        truth = wait_for_collector(control, args.timeout_sec)
        surface = wait_for_surface(control, args.timeout_sec, args.surface_label)
        revision = int(surface["basis_page_revision"])
        wait_for_expected_text(control, revision, args.expect_text, args.timeout_sec)
        rejected: list[str] = []
        for name, mutation in (
            ("stale_revision", {"basis_page_revision": revision + 1, "direction": "east"}),
            ("invalid_direction", {"basis_page_revision": revision, "direction": "diagonal"}),
        ):
            try:
                control.call(
                    "act_drag",
                    {"action_id": surface["action_id"], **mutation},
                )
            except RuntimeError:
                rejected.append(name)
            else:
                raise AssertionError(f"unsafe drag request was accepted: {name}")
        capture(control, revision, before_path)

        acceptance_ms: list[float] = []
        receipt_ms: list[float] = []
        directions = ["east", "south", "west", "north"]
        receipts: list[dict[str, Any]] = []
        for index in range(args.drags):
            direction = directions[index % len(directions)]
            issued = time.monotonic_ns()
            accepted = control.call(
                "act_drag",
                {
                    "action_id": surface["action_id"],
                    "basis_page_revision": revision,
                    "direction": direction,
                },
            )
            accepted_at = time.monotonic_ns()
            if accepted.get("status") != "accepted":
                raise AssertionError(f"drag was not accepted: {accepted}")
            receipt = control.call("next_receipt", {"timeout_ms": 3000})
            receipted_at = time.monotonic_ns()
            if (
                receipt.get("verified") is not True
                or receipt.get("action_id") != surface["action_id"]
                or receipt.get("values_logged") is not False
            ):
                raise AssertionError(f"drag receipt did not verify: {receipt}")
            acceptance_ms.append((accepted_at - issued) / 1_000_000)
            receipt_ms.append((receipted_at - issued) / 1_000_000)
            receipts.append(receipt)

        time.sleep(0.15)
        capture(control, revision, after_path)
        visual = image_metrics(before_path, after_path)
        replay = [
            json.loads(line)
            for line in replay_path.read_text().splitlines()
            if line.strip()
        ]
        pointer_replay = [item for item in replay if item.get("event") == "pointer_applied"]
        status = control.call("shell_status")
        render_changed = visual["changed_pixel_fraction"] >= 0.005
        passed = (
            len(receipts) == args.drags
            and len(pointer_replay) == args.drags
            and all(item.get("values_logged") is False for item in replay)
            and visual["max_channel_range"] >= 80
            and (args.allow_static_render or render_changed)
            and status.get("collector_ready") is True
            and status.get("popup_count") == 0
        )
        report = {
            "schema": "saccade-cef-local-game-v1",
            "verdict": "PASS" if passed else "FAIL",
            "engine": "cef",
            "url": args.url,
            "surface": {
                "role": surface.get("role"),
                "kind": surface.get("kind"),
                "fact_bound": True,
                "raw_host_coordinates_supplied": False,
                "page_revision": revision,
                "label": surface.get("label"),
            },
            "motor": {
                "requested_drags": args.drags,
                "verified_receipts": len(receipts),
                "acceptance_p95_ms": percentile_95(acceptance_ms),
                "intentional_hold_ms": 250,
                "receipt_p95_ms": percentile_95(receipt_ms),
                "unsafe_requests_rejected": rejected,
            },
            "render": {
                **visual,
                "dynamic_change_required": not args.allow_static_render,
                "dynamic_change_observed": render_changed,
                "expected_text_markers": args.expect_text,
                "expected_text_markers_observed": True,
            },
            "replay": {
                "pointer_events": len(pointer_replay),
                "values_logged": False,
            },
            "route": {
                "truth": "renderer_surface_fact",
                "motor": "cef_native_pointer_drag",
                "receipt": "renderer_pointer_receipt",
                "cdp_used": False,
                "webdriver_used": False,
                "screenshot_used_as_truth": False,
                "screenshot_used_for_guarded_validation": True,
            },
            "final_status": status,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-cef-local-game-v1",
            "verdict": "FAIL",
            "error": str(error),
            "duration_sec": round(time.monotonic() - started, 3),
        }
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
        browser_log.close()
        shutil.rmtree(session, ignore_errors=True)

    report_path = args.output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(
        "CEF_LOCAL_GAME "
        f"verdict={report['verdict']} "
        f"receipts={report.get('motor', {}).get('verified_receipts', 0)} "
        f"report={report_path}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
