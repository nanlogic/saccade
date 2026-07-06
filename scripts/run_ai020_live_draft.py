#!/usr/bin/env python3
"""Run an AI-020 human-in-loop live draft measurement.

This is a thin harness around the existing ServoShell bridge. It does not add a
new browser capability. It launches the visible/human browser, optionally waits
for the human to log in or navigate, then calls inspect_editors and
draft_editor_fill through the same bridge and writes a redacted measurement
report.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import queue
import re
import signal
import socket
import subprocess
import sys
import threading
import time
from typing import Any


ROOT = Path(os.environ.get("SACCADE_ROOT", Path(__file__).resolve().parents[1])).resolve()
DEFAULT_KIT = ROOT / "dist" / "saccade-dogfood-current"
DEFAULT_BIN = DEFAULT_KIT / "bin" / "saccade-servoshell"
DEFAULT_SERVOSHELL = Path(
    "/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell"
)
DEFAULT_PROFILE = ROOT / "runs" / "dogfood_profile" / "default"

CANONICAL_DRAFT_FIELDS = {"description", "filename", "body"}
DRAFT_PROFILES: dict[str, dict[str, str]] = {
    "raw": {
        "description": "description",
        "filename": "filename",
        "body": "body",
    },
    "gist": {
        "description": "description",
        "filename": "filename",
        "body": "body",
    },
    "generic_body": {
        "body": "body",
        "comment": "body",
        "message": "body",
        "reply": "body",
        "text": "body",
    },
    "hn_comment": {
        "body": "body",
        "comment": "body",
        "reply": "body",
        "text": "body",
    },
    "discourse_reply": {
        "body": "body",
        "comment": "body",
        "reply": "body",
        "text": "body",
    },
    "reddit_comment": {
        "body": "body",
        "comment": "body",
        "reply": "body",
        "text": "body",
    },
    "github_issue": {
        "title": "description",
        "description": "description",
        "body": "body",
        "comment": "body",
    },
    "github_discussion": {
        "title": "description",
        "description": "description",
        "body": "body",
        "comment": "body",
    },
}
PROFILE_BY_SITE = {
    "gist": "gist",
    "gist_draft": "gist",
    "hn_comment": "hn_comment",
    "local_forum": "generic_body",
    "local_forum_fixture": "generic_body",
    "github_issue": "github_issue",
    "github_discussion": "github_discussion",
    "discourse_reply": "discourse_reply",
    "reddit_comment": "reddit_comment",
}


def now_stamp() -> str:
    return time.strftime("%Y%m%d-%H%M%S")


def safe_slug(text: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9_.-]+", "_", text.strip()).strip("_")
    return slug[:80] or "ai020_live_draft"


def read_text(path: str | None) -> str | None:
    if not path:
        return None
    return Path(path).read_text(encoding="utf-8")


def resolve_draft_profile(args: argparse.Namespace) -> str:
    profile = args.draft_profile or PROFILE_BY_SITE.get(args.site, "raw")
    if profile not in DRAFT_PROFILES:
        known = ", ".join(sorted(DRAFT_PROFILES))
        raise SystemExit(f"unknown --draft-profile {profile!r}; known profiles: {known}")
    return profile


def normalize_fields(raw_fields: dict[str, str], profile: str) -> dict[str, str]:
    field_map = DRAFT_PROFILES[profile]
    normalized: dict[str, str] = {}
    source_for_slot: dict[str, str] = {}
    unsupported = sorted(set(raw_fields) - set(field_map))
    if unsupported:
        raise SystemExit(
            f"unsupported draft field(s) for profile {profile}: {', '.join(unsupported)}"
        )

    for source, value in raw_fields.items():
        slot = field_map[source]
        if slot not in CANONICAL_DRAFT_FIELDS:
            raise SystemExit(f"draft profile {profile} maps {source} to unsupported slot {slot}")
        if slot in normalized and normalized[slot] != value:
            first = source_for_slot[slot]
            raise SystemExit(
                f"fields {first!r} and {source!r} both map to draft slot {slot!r}; "
                "provide only one"
            )
        normalized[slot] = value
        source_for_slot[slot] = source
    return normalized


def load_fields(args: argparse.Namespace) -> tuple[dict[str, str], dict[str, Any]]:
    raw_fields: dict[str, str] = {}
    if args.fields_json:
        data = json.loads(args.fields_json)
        if not isinstance(data, dict):
            raise SystemExit("--fields-json must decode to an object")
        raw_fields.update({str(k): str(v) for k, v in data.items()})
    if args.fields_file:
        data = json.loads(Path(args.fields_file).read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            raise SystemExit("--fields-file must decode to an object")
        raw_fields.update({str(k): str(v) for k, v in data.items()})
    for key, value in {
        "title": read_text(args.title_file),
        "description": read_text(args.description_file),
        "filename": read_text(args.filename_file),
        "body": read_text(args.body_file),
        "comment": read_text(args.comment_file),
    }.items():
        if value is not None:
            raw_fields[key] = value
    profile = resolve_draft_profile(args)
    fields = normalize_fields(raw_fields, profile)
    if not fields:
        raise SystemExit("provide at least one draft field")
    return fields, redacted_field_summary(fields, raw_fields, profile)


def redacted_field_summary(
    fields: dict[str, str],
    raw_fields: dict[str, str],
    profile: str,
) -> dict[str, Any]:
    return {
        "draft_profile": profile,
        "source_names": sorted(raw_fields),
        "names": sorted(fields),
        "lengths": {key: len(value) for key, value in fields.items()},
        "source_lengths": {key: len(value) for key, value in raw_fields.items()},
    }


def reader_thread(stream: Any, out: "queue.Queue[tuple[str, str]]", name: str) -> None:
    try:
        for line in iter(stream.readline, ""):
            out.put((name, line.rstrip("\n")))
    finally:
        out.put((name, "__EOF__"))


def wait_for_ready(
    proc: subprocess.Popen[str],
    lines: "queue.Queue[tuple[str, str]]",
    timeout_sec: float,
) -> tuple[str, str | None, str | None, list[tuple[str, str]]]:
    deadline = time.monotonic() + timeout_sec
    seen: list[tuple[str, str]] = []
    pattern = re.compile(r"SACCADE_SERVOSHELL_BRIDGE READY endpoint=(\S+) grant=(\S+) report=(\S+)")
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"bridge exited before ready: returncode={proc.returncode}")
        try:
            source, line = lines.get(timeout=0.2)
        except queue.Empty:
            continue
        seen.append((source, line))
        match = pattern.search(line)
        if match:
            return match.group(1), match.group(2), match.group(3), seen
    raise TimeoutError(f"bridge did not become ready within {timeout_sec}s")


def call_bridge(endpoint: str, method: str, params: dict[str, Any], timeout_sec: float) -> dict[str, Any]:
    host, port_text = endpoint.rsplit(":", 1)
    request = json.dumps({"id": 1, "method": method, "params": params}) + "\n"
    with socket.create_connection((host, int(port_text)), timeout=timeout_sec) as sock:
        sock.settimeout(timeout_sec)
        sock.sendall(request.encode("utf-8"))
        chunks = []
        while True:
            chunk = sock.recv(65536)
            if not chunk:
                break
            chunks.append(chunk)
            if b"\n" in chunk:
                break
    raw = b"".join(chunks).decode("utf-8", errors="replace").strip()
    response = json.loads(raw)
    if response.get("ok") is not True:
        raise RuntimeError(response.get("error") or f"{method} failed")
    result = response.get("result")
    return result if isinstance(result, dict) else {"value": result}


def collect_remaining_lines(
    lines: "queue.Queue[tuple[str, str]]",
    limit: int = 120,
) -> list[tuple[str, str]]:
    out: list[tuple[str, str]] = []
    while len(out) < limit:
        try:
            item = lines.get_nowait()
        except queue.Empty:
            break
        out.append(item)
    return out


def assert_no_value_leak(
    paths: list[Path],
    fields: dict[str, str],
    virtual_texts: dict[str, str] | None = None,
) -> dict[str, Any]:
    leaks: list[dict[str, str]] = []
    needles = {key: value for key, value in fields.items() if value}
    checked_virtual: list[str] = []
    for path in paths:
        if not path.exists() or path.is_dir():
            continue
        text = path.read_text(errors="replace")
        for key, value in needles.items():
            if value and value in text:
                leaks.append({"field": key, "path": str(path)})
    for name, text in (virtual_texts or {}).items():
        checked_virtual.append(name)
        for key, value in needles.items():
            if value and value in text:
                leaks.append({"field": key, "path": name})
    return {
        "ok": not leaks,
        "checked_paths": [str(path) for path in paths if path.exists() and not path.is_dir()],
        "checked_virtual": checked_virtual,
        "leaks": leaks,
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")


def under_root(path_text: str | Path) -> Path:
    path = Path(path_text)
    return path if path.is_absolute() else ROOT / path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True)
    parser.add_argument("--site", default="")
    parser.add_argument("--run-name", default="")
    parser.add_argument("--bin", default=str(DEFAULT_BIN))
    parser.add_argument("--servoshell", default=str(DEFAULT_SERVOSHELL))
    parser.add_argument("--profile-dir", default=str(DEFAULT_PROFILE))
    parser.add_argument("--userscripts-dir", default="")
    parser.add_argument("--output-dir", default="")
    parser.add_argument(
        "--draft-profile",
        default="",
        help=(
            "Field profile: raw, gist, generic_body, hn_comment, discourse_reply, "
            "reddit_comment, github_issue, or github_discussion. Defaults from --site when known."
        ),
    )
    parser.add_argument("--fields-json", default="")
    parser.add_argument("--fields-file", default="")
    parser.add_argument("--title-file", default="")
    parser.add_argument("--description-file", default="")
    parser.add_argument("--filename-file", default="")
    parser.add_argument("--body-file", default="")
    parser.add_argument("--comment-file", default="")
    parser.add_argument("--overwrite", action="store_true")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--manual-gate", action="store_true")
    parser.add_argument("--human-wait-sec", type=float, default=0)
    parser.add_argument("--ready-timeout-sec", type=float, default=45)
    parser.add_argument("--control-timeout-sec", type=float, default=35)
    parser.add_argument("--review-gate", action=argparse.BooleanOptionalAction, default=None)
    parser.add_argument("--shutdown", action=argparse.BooleanOptionalAction, default=True)
    args = parser.parse_args()
    review_gate = args.review_gate if args.review_gate is not None else (args.manual_gate and not args.headless)

    fields, field_summary = load_fields(args)
    run_name = safe_slug(args.run_name or args.site or "ai020_live_draft")
    output_dir = under_root(args.output_dir) if args.output_dir else ROOT / "runs" / "ai020_live" / f"{run_name}_{now_stamp()}"
    output_dir.mkdir(parents=True, exist_ok=True)
    grant_path = output_dir / "current_tab_grant.json"
    bridge_output = output_dir / "bridge"

    cmd = [
        args.bin,
        "bridge",
        "--servoshell",
        args.servoshell,
        "--url",
        args.url,
        "--profile-dir",
        args.profile_dir,
        "--grant-path",
        str(grant_path),
        "--output-dir",
        str(bridge_output),
        "--timeout-sec",
        str(int(args.control_timeout_sec)),
    ]
    if args.userscripts_dir:
        cmd.extend(["--userscripts-dir", args.userscripts_dir])
    if not args.headless:
        cmd.append("--no-headless")

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "error")
    env.setdefault("SACCADE_OWNED_DOMAINS", "nanmesh.ai,mythcastera.com,mysterypartynow.com")

    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )

    line_queue: "queue.Queue[tuple[str, str]]" = queue.Queue()
    assert proc.stdout is not None
    assert proc.stderr is not None
    threading.Thread(target=reader_thread, args=(proc.stdout, line_queue, "stdout"), daemon=True).start()
    threading.Thread(target=reader_thread, args=(proc.stderr, line_queue, "stderr"), daemon=True).start()

    report: dict[str, Any] = {
        "ok": False,
        "kind": "ai020_live_draft_measurement",
        "site": args.site or None,
        "url": args.url,
        "run_name": run_name,
        "command_redacted": [part if part not in fields.values() else "[redacted-draft-value]" for part in cmd],
        "output_dir": str(output_dir),
        "bridge_output_dir": str(bridge_output),
        "grant_path": str(grant_path),
        "profile_dir": args.profile_dir,
        "field_summary": field_summary,
        "policy": {
            "manual_login_human_only": True,
            "review_gate": review_gate,
            "block_sensitive": True,
            "no_submit": True,
            "values_logged": False,
            "publish_attempted": False,
            "screenshots_used": False,
        },
    }

    try:
        endpoint, ready_grant, ready_report, seen = wait_for_ready(proc, line_queue, args.ready_timeout_sec)
        report["bridge_ready"] = {
            "endpoint": endpoint,
            "grant": ready_grant,
            "report": ready_report,
            "lines": seen[-20:],
        }
        print(f"Saccade bridge ready: {endpoint}", file=sys.stderr)
        print(f"Visible browser URL: {args.url}", file=sys.stderr)

        if args.human_wait_sec > 0:
            print(f"Waiting {args.human_wait_sec:.0f}s for human login/navigation...", file=sys.stderr)
            time.sleep(args.human_wait_sec)
        if args.manual_gate:
            print("Human step: log in/navigate/review visible page, then press Enter here.", file=sys.stderr)
            sys.stdin.readline()

        ping = call_bridge(endpoint, "ping", {}, args.control_timeout_sec)
        inspect_before = call_bridge(endpoint, "inspect_editors", {}, args.control_timeout_sec)
        fill = call_bridge(
            endpoint,
            "draft_editor_fill",
            {
                "block_sensitive": True,
                "no_submit": True,
                "overwrite": args.overwrite,
                "fields": fields,
            },
            args.control_timeout_sec,
        )
        inspect_after = call_bridge(endpoint, "inspect_editors", {}, args.control_timeout_sec)

        control_report = under_root(
            fill.get("artifacts", {}).get("report")
            or inspect_after.get("artifacts", {}).get("report")
            or bridge_output / "control" / "report.json"
        )
        replay = under_root(
            fill.get("artifacts", {}).get("replay")
            or inspect_after.get("artifacts", {}).get("replay")
            or bridge_output / "control" / "replay.jsonl"
        )

        report.update(
            {
                "ok": True,
                "read_status": "pass" if ping.get("status") == "ok" else "fail",
                "draft_status": "pass" if fill.get("draft_fields_filled", 0) > 0 else "no_field_filled",
                "handoff_status": "pending_human_review_submit",
                "replay_status": "pass",
                "page": {
                    "title": ping.get("title"),
                    "url": ping.get("url"),
                    "site_policy": ping.get("site_policy"),
                },
                "inspect_before": {
                    "route": inspect_before.get("route"),
                    "editor_count": inspect_before.get("editor_count"),
                    "visible_writable_count": inspect_before.get("visible_writable_count"),
                    "visible_authoring_count": inspect_before.get("visible_authoring_count"),
                    "sensitive_count": inspect_before.get("sensitive_count"),
                    "source_url": inspect_before.get("source_url"),
                    "source_title": inspect_before.get("source_title"),
                },
                "fill": {
                    "status": fill.get("status"),
                    "draft_fields_requested": fill.get("draft_fields_requested"),
                    "draft_fields_filled": fill.get("draft_fields_filled"),
                    "draft_fields_rejected": fill.get("draft_fields_rejected"),
                    "chars_written": fill.get("chars_written"),
                    "filled": fill.get("filled", []),
                    "rejected": fill.get("rejected", []),
                    "verification": fill.get("verification"),
                    "policy": fill.get("policy"),
                },
                "inspect_after": {
                    "route": inspect_after.get("route"),
                    "editor_count": inspect_after.get("editor_count"),
                    "visible_writable_count": inspect_after.get("visible_writable_count"),
                    "visible_authoring_count": inspect_after.get("visible_authoring_count"),
                    "sensitive_count": inspect_after.get("sensitive_count"),
                    "source_url": inspect_after.get("source_url"),
                    "source_title": inspect_after.get("source_title"),
                },
                "artifacts": {
                    "report": str(output_dir / "report.json"),
                    "bridge_report": ready_report,
                    "control_report": str(control_report),
                    "control_replay": str(replay),
                    "grant": str(grant_path),
                },
            }
        )
        leak_paths = [control_report, replay]
        leak_check = assert_no_value_leak(
            leak_paths,
            fields,
            {"final_report_candidate": json.dumps(report, ensure_ascii=False)},
        )
        artifact_check = {
            "control_report_exists": control_report.exists(),
            "control_replay_exists": replay.exists(),
        }
        report["artifact_check"] = artifact_check
        report["value_leak_check"] = leak_check
        if not artifact_check["control_report_exists"] or not artifact_check["control_replay_exists"]:
            report["ok"] = False
            report["replay_status"] = "failed_missing_control_artifacts"
        if not leak_check["ok"]:
            report["ok"] = False
            report["draft_status"] = "failed_value_leak_check"
        if review_gate and report.get("ok"):
            write_json(output_dir / "report.json", report)
            print(
                "Draft filled. Review the visible Saccade window now; press Enter here to close it.",
                file=sys.stderr,
            )
            sys.stdin.readline()
            report["handoff_status"] = "human_review_gate_acknowledged"
    except Exception as error:
        report["ok"] = False
        report["error"] = str(error)
        report["tail_lines"] = collect_remaining_lines(line_queue)
    finally:
        if args.shutdown:
            try:
                if "bridge_ready" in report:
                    call_bridge(report["bridge_ready"]["endpoint"], "shutdown", {}, 5)
            except Exception as error:
                report["shutdown_error"] = str(error)
            try:
                proc.wait(timeout=8)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(proc.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
        else:
            report["process_left_running"] = proc.poll() is None
            report["process_pid"] = proc.pid
            try:
                if proc.stdout:
                    proc.stdout.close()
                if proc.stderr:
                    proc.stderr.close()
            except Exception:
                pass
        report["process"] = {"returncode": proc.poll()}
        report["tail_lines"] = collect_remaining_lines(line_queue)
        write_json(output_dir / "report.json", report)

    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
