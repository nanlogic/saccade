#!/usr/bin/env python3
"""Build a sanitized, web-ready public evidence pack for a Saccade reflex run.

This script does not operate a browser and is never part of the reflex hot loop.
It packages an existing ``saccade.web.reflex_run`` result, its JSONL replay, and
a human-facing screen recording. Structured same-WebView truth and matching
native-input receipts remain authoritative; the video is illustrative.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
PACK_SCHEMA = "saccade-public-reflex-evidence-v1"
REDACTED = "[REDACTED]"
SENSITIVE_KEYS = {
    "authorization",
    "capability",
    "capability_token",
    "control_capability",
    "cookie",
    "cookies",
    "cvv",
    "otp",
    "passport",
    "password",
    "secret",
    "session_token",
    "ssn",
    "storage_value",
    "token",
}


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create a sanitized public report/replay and WebM, MP4, GIF, poster, "
            "manifest, checksums, and README from one reflex run."
        )
    )
    parser.add_argument(
        "--run-dir",
        type=pathlib.Path,
        required=True,
        help="Directory containing report.json and replay.jsonl",
    )
    parser.add_argument(
        "--master-video",
        type=pathlib.Path,
        required=True,
        help="Uncut human-facing recording of the run",
    )
    parser.add_argument(
        "--output-dir",
        type=pathlib.Path,
        required=True,
        help="New evidence-pack directory; must not already contain files",
    )
    parser.add_argument("--build", required=True, help="Installed Saccade build number")
    parser.add_argument(
        "--title", default="Saccade 15-second reflex run", help="Public report title"
    )
    parser.add_argument(
        "--expected-game-duration-sec",
        type=float,
        default=15.0,
        help="Minimum expected game duration represented by the master recording",
    )
    parser.add_argument(
        "--preview-start-sec",
        type=float,
        default=0.0,
        help="Preview-loop start offset in the master recording",
    )
    parser.add_argument(
        "--preview-duration-sec",
        type=float,
        default=6.0,
        help="Preview-loop duration",
    )
    parser.add_argument(
        "--commit",
        default=None,
        help="Git commit; defaults to the current repository HEAD",
    )
    parser.add_argument(
        "--platform",
        dest="platform_label",
        default=None,
        help="Public platform label; defaults to OS and CPU architecture",
    )
    parser.add_argument(
        "--allow-fail",
        action="store_true",
        help="Package a failed run for debugging; PASS remains the default requirement",
    )
    parser.add_argument(
        "--max-gif-mib",
        type=float,
        default=8.0,
        help="Reject an oversized optional README GIF",
    )
    parser.add_argument("--ffmpeg", default=None, help="ffmpeg executable override")
    parser.add_argument("--ffprobe", default=None, help="ffprobe executable override")
    return parser.parse_args(argv)


def run_command(command: list[str], *, cwd: pathlib.Path | None = None) -> str:
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        detail = (error.stderr or error.stdout or "command failed").strip()
        raise RuntimeError(f"{pathlib.Path(command[0]).name}: {detail}") from error
    return completed.stdout.strip()


def resolve_tool(explicit: str | None, name: str) -> str:
    resolved = explicit or shutil.which(name)
    if not resolved:
        raise RuntimeError(f"missing required media tool: {name}")
    return resolved


def current_commit() -> str:
    return run_command(["git", "rev-parse", "HEAD"], cwd=ROOT)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sanitize_url(value: str) -> str:
    try:
        parsed = urllib.parse.urlsplit(value)
    except ValueError:
        return value
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return value
    return urllib.parse.urlunsplit(
        (parsed.scheme, parsed.netloc, parsed.path, "", "")
    )


def sanitize_string(value: str) -> str:
    sanitized = value.replace(str(ROOT), "$REPO")
    sanitized = sanitized.replace(str(pathlib.Path.home()), "$HOME")
    temp_roots = {tempfile.gettempdir(), str(pathlib.Path(tempfile.gettempdir()).resolve())}
    for temp_root in temp_roots:
        sanitized = sanitized.replace(temp_root, "$TMP")
    if sanitized.startswith(("http://", "https://")):
        sanitized = sanitize_url(sanitized)
    if sanitized.lower().startswith("bearer "):
        return "Bearer [REDACTED]"
    return sanitized


def sensitive_key(key: str) -> bool:
    normalized = key.strip().lower()
    return normalized in SENSITIVE_KEYS or normalized.endswith("_secret") or normalized.endswith(
        "_token"
    )


def sanitize(value: Any, *, key: str = "") -> Any:
    if key and sensitive_key(key):
        return REDACTED
    if isinstance(value, dict):
        return {str(child_key): sanitize(child, key=str(child_key)) for child_key, child in value.items()}
    if isinstance(value, list):
        return [sanitize(child) for child in value]
    if isinstance(value, str):
        return sanitize_string(value)
    return value


def load_json(path: pathlib.Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError as error:
        raise RuntimeError(f"missing required evidence file: {path}") from error
    except json.JSONDecodeError as error:
        raise RuntimeError(f"invalid JSON in {path}: {error}") from error
    if not isinstance(value, dict):
        raise RuntimeError(f"expected a JSON object in {path}")
    return value


def verdict(report: dict[str, Any]) -> str:
    raw = report.get("verdict")
    if isinstance(raw, str) and raw.strip():
        return raw.strip().upper()
    return "PASS" if report.get("completed") is True else "UNKNOWN"


def require_agent_layer_proof(report: dict[str, Any]) -> None:
    agent_layer = report.get("agent_layer")
    if not isinstance(agent_layer, dict):
        raise RuntimeError("report is missing agent_layer proof")
    required = {
        "required": True,
        "bound": True,
        "route": "same_webview_control_v1",
        "input_route": "native_cef_input",
        "receipt_verification": "matching_action_id_applied_v1",
        "llm_calls_in_hot_loop": 0,
        "screenshot_fallback_used": False,
        "external_input_fallback_used": False,
        "fail_closed": True,
    }
    mismatches = [
        f"{name}={agent_layer.get(name)!r}"
        for name, expected in required.items()
        if agent_layer.get(name) != expected
    ]
    if mismatches:
        raise RuntimeError(
            "report does not satisfy installed Agent Layer proof: " + ", ".join(mismatches)
        )


def validate_pass_report(report: dict[str, Any]) -> None:
    if verdict(report) != "PASS":
        raise RuntimeError(f"public PASS pack requires verdict=PASS, got {verdict(report)}")
    if report.get("completed") is not True:
        raise RuntimeError("public PASS pack requires completed=true")
    require_agent_layer_proof(report)

    receipts = report.get("verified_target_receipts")
    if not isinstance(receipts, int) or receipts <= 0:
        raise RuntimeError("public PASS pack requires verified_target_receipts > 0")
    if report.get("final_misses") not in {0, 0.0}:
        raise RuntimeError("public PASS pack requires final_misses=0")

    if report.get("completion_policy") == "mouseaccuracy_results_truth_v1":
        truth = report.get("benchmark_truth")
        if not isinstance(truth, dict):
            raise RuntimeError("MouseAccuracy PASS is missing benchmark_truth")
        expected = {
            "target_efficiency_pct": 100,
            "click_accuracy_pct": 100,
            "verified_receipt_count_matches_hits": True,
        }
        mismatches = [
            f"{name}={truth.get(name)!r}"
            for name, expected_value in expected.items()
            if truth.get(name) != expected_value
        ]
        if truth.get("targets_hit") != receipts:
            mismatches.append(
                f"targets_hit={truth.get('targets_hit')!r} receipts={receipts!r}"
            )
        if mismatches:
            raise RuntimeError(
                "MouseAccuracy result truth is not a full-score proof: "
                + ", ".join(mismatches)
            )


def sanitize_replay(source: pathlib.Path, destination: pathlib.Path) -> int:
    if not source.is_file():
        raise RuntimeError(f"missing required evidence file: {source}")
    count = 0
    with source.open() as input_handle, destination.open("w") as output_handle:
        for line_number, line in enumerate(input_handle, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            try:
                value = json.loads(stripped)
            except json.JSONDecodeError as error:
                raise RuntimeError(
                    f"invalid replay JSON at {source}:{line_number}: {error}"
                ) from error
            output_handle.write(json.dumps(sanitize(value), sort_keys=True) + "\n")
            count += 1
    if count == 0:
        raise RuntimeError("public evidence replay must contain at least one event")
    return count


def probe_video(ffprobe: str, path: pathlib.Path) -> dict[str, Any]:
    output = run_command(
        [
            ffprobe,
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "format=duration:stream=codec_name,width,height,r_frame_rate",
            "-of",
            "json",
            str(path),
        ]
    )
    value = json.loads(output)
    streams = value.get("streams") or []
    if not streams:
        raise RuntimeError(f"master video has no video stream: {path}")
    duration = float((value.get("format") or {}).get("duration") or 0.0)
    stream = streams[0]
    return {
        "duration_sec": round(duration, 3),
        "codec": stream.get("codec_name"),
        "width": stream.get("width"),
        "height": stream.get("height"),
        "frame_rate": stream.get("r_frame_rate"),
    }


def ffmpeg_base(ffmpeg: str) -> list[str]:
    return [ffmpeg, "-nostdin", "-hide_banner", "-loglevel", "error", "-y"]


def export_media(
    *,
    ffmpeg: str,
    master: pathlib.Path,
    output: pathlib.Path,
    preview_start: float,
    preview_duration: float,
) -> list[pathlib.Path]:
    scale_full = "scale=min(1280\\,iw):-2:flags=lanczos"
    scale_preview = "scale=min(960\\,iw):-2:flags=lanczos"
    scale_gif = "scale=min(720\\,iw):-2:flags=lanczos"
    full_mp4 = output / "reflex-full.mp4"
    loop_webm = output / "reflex-loop.webm"
    loop_mp4 = output / "reflex-loop.mp4"
    poster = output / "reflex-poster.jpg"
    gif = output / "reflex-readme.gif"

    run_command(
        ffmpeg_base(ffmpeg)
        + [
            "-i",
            str(master),
            "-vf",
            scale_full,
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            str(full_mp4),
        ]
    )
    preview_input = [
        "-ss",
        f"{preview_start:.3f}",
        "-t",
        f"{preview_duration:.3f}",
        "-i",
        str(master),
    ]
    run_command(
        ffmpeg_base(ffmpeg)
        + preview_input
        + [
            "-vf",
            scale_preview,
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-crf",
            "34",
            "-b:v",
            "0",
            "-row-mt",
            "1",
            str(loop_webm),
        ]
    )
    run_command(
        ffmpeg_base(ffmpeg)
        + preview_input
        + [
            "-vf",
            scale_preview,
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "24",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            str(loop_mp4),
        ]
    )
    poster_at = preview_start + min(0.5, preview_duration / 2.0)
    run_command(
        ffmpeg_base(ffmpeg)
        + [
            "-ss",
            f"{poster_at:.3f}",
            "-i",
            str(master),
            "-frames:v",
            "1",
            "-vf",
            scale_preview,
            "-c:v",
            "mjpeg",
            "-q:v",
            "3",
            str(poster),
        ]
    )
    gif_filter = (
        f"fps=10,{scale_gif},split[s0][s1];"
        "[s0]palettegen=max_colors=96[p];"
        "[s1][p]paletteuse=dither=bayer:bayer_scale=3"
    )
    run_command(
        ffmpeg_base(ffmpeg)
        + preview_input
        + ["-filter_complex", gif_filter, "-loop", "0", str(gif)]
    )
    return [full_mp4, loop_webm, loop_mp4, poster, gif]


def nested(report: dict[str, Any], *path: str) -> Any:
    current: Any = report
    for part in path:
        if not isinstance(current, dict):
            return None
        current = current.get(part)
    return current


def display(value: Any) -> str:
    if value is None:
        return "not reported"
    if isinstance(value, bool):
        return "yes" if value else "no"
    return str(value)


def report_summary(report: dict[str, Any]) -> dict[str, Any]:
    return {
        "verdict": verdict(report),
        "completed": report.get("completed"),
        "completion_policy": report.get("completion_policy"),
        "final_hits": report.get("final_hits"),
        "final_misses": report.get("final_misses"),
        "verified_target_receipts": report.get("verified_target_receipts"),
        "total_score": nested(report, "benchmark_truth", "total_score"),
        "target_efficiency_pct": nested(
            report, "benchmark_truth", "target_efficiency_pct"
        ),
        "click_accuracy_pct": nested(
            report, "benchmark_truth", "click_accuracy_pct"
        ),
        "latency_p95_ms": nested(report, "latency_ms", "p95"),
        "duration_ms": report.get("duration_ms"),
        "llm_calls_in_hot_loop": nested(
            report, "agent_layer", "llm_calls_in_hot_loop"
        ),
        "screenshot_fallback_used": nested(
            report, "agent_layer", "screenshot_fallback_used"
        ),
        "external_input_fallback_used": nested(
            report, "agent_layer", "external_input_fallback_used"
        ),
    }


def write_readme(
    path: pathlib.Path,
    *,
    title: str,
    build: str,
    commit: str,
    platform_label: str,
    report: dict[str, Any],
    replay_events: int,
    video: dict[str, Any],
) -> None:
    summary = report_summary(report)
    path.write_text(
        f"""# {title}

This is sanitized experimental dogfood evidence, not a stable-release claim.

![Short reflex preview](reflex-readme.gif)

## Result

| Field | Recorded value |
| --- | --- |
| Verdict | {display(summary['verdict'])} |
| Saccade build | {build} |
| Commit | `{commit}` |
| Platform | {platform_label} |
| Completion policy | `{display(summary['completion_policy'])}` |
| Final hits | {display(summary['final_hits'])} |
| Final misses | {display(summary['final_misses'])} |
| Verified target receipts | {display(summary['verified_target_receipts'])} |
| Page-reported total score | {display(summary['total_score'])} |
| Target efficiency | {display(summary['target_efficiency_pct'])}% |
| Click accuracy | {display(summary['click_accuracy_pct'])}% |
| Fact-to-receipt p95 | {display(summary['latency_p95_ms'])} ms |
| LLM calls in hot loop | {display(summary['llm_calls_in_hot_loop'])} |
| Screenshot fallback used | {display(summary['screenshot_fallback_used'])} |
| External-input fallback used | {display(summary['external_input_fallback_used'])} |
| Sanitized replay events | {replay_events} |
| Master recording duration | {video['duration_sec']} s |

## What is proof

`report.json` contains the same-WebView result truth returned by
`saccade.web.reflex_run`. `replay.jsonl` records the sanitized execution trail.
A PASS requires matching native-input receipts and, for MouseAccuracy, 100%
target efficiency, 100% click accuracy, and exact equality between page hits
and verified target receipts.

The video and GIF are human-facing illustrations. They are not used as browser
truth and cannot make a failed or unverifiable run pass.

## Media

- [Uncut master recording](reflex-master{pathlib.Path(video['master_name']).suffix})
- [Full MP4](reflex-full.mp4)
- [Website WebM loop](reflex-loop.webm)
- [Website MP4 loop](reflex-loop.mp4)
- [Poster](reflex-poster.jpg)
- [README GIF](reflex-readme.gif)

Use `embed.html` for the recommended WebM-first website markup. Verify every
download against `SHA256SUMS` or `manifest.json`.
"""
    )


def write_embed(path: pathlib.Path, *, title: str) -> None:
    escaped = (
        title.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )
    path.write_text(
        f"""<!-- Human-facing illustration; report.json and replay.jsonl are authoritative. -->
<video autoplay loop muted playsinline preload="metadata" poster="reflex-poster.jpg"
       aria-label="{escaped}">
  <source src="reflex-loop.webm" type="video/webm">
  <source src="reflex-loop.mp4" type="video/mp4">
  <a href="reflex-full.mp4">Watch the full reflex run</a>
</video>
"""
    )


def write_manifest(output: pathlib.Path, roles: dict[str, str]) -> list[pathlib.Path]:
    files = sorted(path for path in output.iterdir() if path.is_file())
    entries = [
        {
            "path": path.name,
            "bytes": path.stat().st_size,
            "sha256": sha256(path),
            "role": roles.get(path.name, "supporting evidence"),
        }
        for path in files
        if path.name not in {"manifest.json", "SHA256SUMS"}
    ]
    manifest_path = output / "manifest.json"
    manifest_path.write_text(
        json.dumps({"schema": PACK_SCHEMA, "files": entries}, indent=2, sort_keys=True)
        + "\n"
    )
    checksum_paths = sorted([*files, manifest_path], key=lambda path: path.name)
    checksum_path = output / "SHA256SUMS"
    checksum_path.write_text(
        "".join(f"{sha256(path)}  {path.name}\n" for path in checksum_paths)
    )
    return [*checksum_paths, checksum_path]


def ensure_clean_output(output: pathlib.Path) -> None:
    if output.exists() and not output.is_dir():
        raise RuntimeError(f"output path is not a directory: {output}")
    if output.exists() and any(output.iterdir()):
        raise RuntimeError(f"output directory is not empty: {output}")


def package(args: argparse.Namespace) -> pathlib.Path:
    if args.expected_game_duration_sec <= 0:
        raise RuntimeError("--expected-game-duration-sec must be positive")
    if args.preview_start_sec < 0 or args.preview_duration_sec <= 0:
        raise RuntimeError("preview offsets and duration must be non-negative/positive")
    if args.max_gif_mib <= 0:
        raise RuntimeError("--max-gif-mib must be positive")

    run_dir = args.run_dir.expanduser().resolve()
    master = args.master_video.expanduser().resolve()
    output = args.output_dir.expanduser().resolve()
    report_path = run_dir / "report.json"
    replay_path = run_dir / "replay.jsonl"
    if not master.is_file():
        raise RuntimeError(f"missing master video: {master}")
    ensure_clean_output(output)

    report = load_json(report_path)
    if not args.allow_fail:
        validate_pass_report(report)
    else:
        require_agent_layer_proof(report)

    ffmpeg = resolve_tool(args.ffmpeg, "ffmpeg")
    ffprobe = resolve_tool(args.ffprobe, "ffprobe")
    video = probe_video(ffprobe, master)
    if video["duration_sec"] + 0.5 < args.expected_game_duration_sec:
        raise RuntimeError(
            "master recording is shorter than the expected game duration: "
            f"{video['duration_sec']}s < {args.expected_game_duration_sec}s"
        )
    if args.preview_start_sec + args.preview_duration_sec > video["duration_sec"] + 0.05:
        raise RuntimeError("preview window extends beyond the master recording")

    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = pathlib.Path(
        tempfile.mkdtemp(prefix=f".{output.name}.", dir=output.parent)
    )
    try:
        sanitized_report = sanitize(report)
        (temporary / "report.json").write_text(
            json.dumps(sanitized_report, indent=2, sort_keys=True) + "\n"
        )
        replay_events = sanitize_replay(replay_path, temporary / "replay.jsonl")

        master_name = f"reflex-master{master.suffix.lower() or '.mov'}"
        copied_master = temporary / master_name
        shutil.copy2(master, copied_master)
        export_media(
            ffmpeg=ffmpeg,
            master=copied_master,
            output=temporary,
            preview_start=args.preview_start_sec,
            preview_duration=args.preview_duration_sec,
        )
        gif = temporary / "reflex-readme.gif"
        gif_mib = gif.stat().st_size / (1024 * 1024)
        if gif_mib > args.max_gif_mib:
            raise RuntimeError(
                f"README GIF is {gif_mib:.2f} MiB, above --max-gif-mib={args.max_gif_mib}; "
                "shorten the preview or lower the export dimensions"
            )

        commit = args.commit or current_commit()
        platform_label = args.platform_label or f"{platform.system()}-{platform.machine()}"
        video["master_name"] = master_name
        environment = {
            "schema": PACK_SCHEMA,
            "generated_at_utc": dt.datetime.now(dt.timezone.utc)
            .replace(microsecond=0)
            .isoformat(),
            "saccade_build": str(args.build),
            "git_commit": commit,
            "platform": platform_label,
            "python": platform.python_version(),
            "ffmpeg": run_command([ffmpeg, "-version"]).splitlines()[0],
            "source_video": {
                **video,
                "sha256": sha256(copied_master),
            },
            "preview": {
                "start_sec": args.preview_start_sec,
                "duration_sec": args.preview_duration_sec,
            },
            "evidence_boundary": {
                "structured_report_and_replay_authoritative": True,
                "video_and_gif_are_illustrative": True,
                "screenshot_or_video_used_as_browser_truth": False,
            },
        }
        (temporary / "environment.json").write_text(
            json.dumps(environment, indent=2, sort_keys=True) + "\n"
        )
        write_readme(
            temporary / "README.md",
            title=args.title,
            build=str(args.build),
            commit=commit,
            platform_label=platform_label,
            report=sanitized_report,
            replay_events=replay_events,
            video=video,
        )
        write_embed(temporary / "embed.html", title=args.title)

        roles = {
            "README.md": "human-readable evidence summary",
            "report.json": "authoritative sanitized same-WebView result truth",
            "replay.jsonl": "sanitized execution replay",
            "environment.json": "build and media provenance",
            "embed.html": "website embed snippet",
            master_name: "uncut human-facing recording",
            "reflex-full.mp4": "full web-compatible recording",
            "reflex-loop.webm": "website preview loop",
            "reflex-loop.mp4": "website preview fallback",
            "reflex-poster.jpg": "preview poster",
            "reflex-readme.gif": "optional README animation",
        }
        write_manifest(temporary, roles)
        if output.exists():
            output.rmdir()
        os.replace(temporary, output)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise

    print(
        "REFLEX_EVIDENCE_PACK "
        f"verdict={verdict(report)} output={output} "
        f"receipts={report.get('verified_target_receipts')}"
    )
    return output


def main(argv: list[str] | None = None) -> int:
    try:
        package(parse_args(argv))
    except (RuntimeError, subprocess.CalledProcessError, OSError, ValueError) as error:
        print(f"REFLEX_EVIDENCE_PACK FAIL error={error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
