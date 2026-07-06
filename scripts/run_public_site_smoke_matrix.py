#!/usr/bin/env python3
"""Run a small, sequential public-site Saccade smoke matrix.

This is an evidence helper, not a new browser capability. It uses the dogfood
ServoShell bridge wrapper to open low-risk public URLs, collect read-only smoke
truth, optionally collect article text, and write one aggregate report.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from typing import Any


ROOT = Path(os.environ.get("SACCADE_ROOT", Path(__file__).resolve().parents[1])).resolve()
DEFAULT_KIT = ROOT / "dist" / "saccade-dogfood-current"

DEFAULT_SITES = [
    {
        "name": "example",
        "url": "https://example.com/",
        "kind": "public_simple",
        "read_article": True,
    },
    {
        "name": "hacker_news",
        "url": "https://news.ycombinator.com/",
        "kind": "public_forum_read",
        "read_article": False,
    },
    {
        "name": "wikipedia_servo",
        "url": "https://en.wikipedia.org/wiki/Servo_(software)",
        "kind": "public_reference",
        "read_article": True,
    },
    {
        "name": "rookies_modular_environment",
        "url": "https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art",
        "kind": "public_tutorial",
        "read_article": True,
    },
]


def now_stamp() -> str:
    return time.strftime("%Y%m%d-%H%M%S")


def safe_slug(text: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9_.-]+", "_", text.strip()).strip("_")
    return slug[:80] or "site"


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_sites(args: argparse.Namespace) -> list[dict[str, Any]]:
    if args.sites_json:
        data = json.loads(Path(args.sites_json).read_text(encoding="utf-8"))
        if not isinstance(data, list):
            raise SystemExit("--sites-json must contain a JSON array")
        sites = []
        for item in data:
            if not isinstance(item, dict) or not item.get("url"):
                raise SystemExit("each site must be an object with at least url")
            sites.append(
                {
                    "name": str(item.get("name") or safe_slug(str(item["url"]))),
                    "url": str(item["url"]),
                    "kind": str(item.get("kind") or "public"),
                    "read_article": bool(item.get("read_article", args.read_article)),
                }
            )
        return sites
    return [dict(site) for site in DEFAULT_SITES]


def bridge_bin(args: argparse.Namespace) -> Path:
    if args.bridge_bin:
        return Path(args.bridge_bin).expanduser().resolve()
    return (Path(args.kit).expanduser().resolve() / "servoshell-bridge")


def run_site(
    args: argparse.Namespace,
    bridge: Path,
    site: dict[str, Any],
    site_dir: Path,
) -> dict[str, Any]:
    site_dir.mkdir(parents=True, exist_ok=True)
    stdout_path = site_dir / "stdout.json"
    stderr_path = site_dir / "stderr.txt"
    output_dir = site_dir / "bridge"
    grant_path = site_dir / "current_tab_grant.json"

    cmd = [
        str(bridge),
        "--url",
        str(site["url"]),
        "--output-dir",
        str(output_dir),
        "--grant-path",
        str(grant_path),
        "--smoke",
        "--json",
        "--timeout-sec",
        str(args.timeout_sec),
    ]
    if site.get("read_article"):
        cmd.append("--read-article")

    started = time.monotonic()
    completed = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=args.timeout_sec + 20,
        check=False,
    )
    elapsed_sec = time.monotonic() - started
    stdout_path.write_text(completed.stdout, encoding="utf-8")
    stderr_path.write_text(completed.stderr, encoding="utf-8")

    parsed: dict[str, Any] | None = None
    parse_error = None
    if completed.stdout.strip():
        try:
            parsed_value = json.loads(completed.stdout)
            if isinstance(parsed_value, dict):
                parsed = parsed_value
            else:
                parse_error = "stdout JSON was not an object"
        except json.JSONDecodeError as error:
            parse_error = str(error)
    else:
        parse_error = "empty stdout"

    smoke = parsed.get("smoke", {}) if parsed else {}
    ping = smoke.get("ping", {}) if isinstance(smoke, dict) else {}
    article = parsed.get("article_text", {}) if parsed else {}
    process = parsed.get("process", {}) if parsed else {}
    page_ready = parsed.get("page", {}).get("ready", {}) if parsed else {}
    site_policy = ping.get("site_policy") if isinstance(ping, dict) else None

    article_status = "not_requested"
    article_length = 0
    if site.get("read_article"):
        article_length = int(article.get("article_text_length") or 0) if isinstance(article, dict) else 0
        article_status = "pass" if article_length > 0 else "empty_or_failed"

    ok = (
        completed.returncode == 0
        and parsed is not None
        and parsed.get("ok") is True
        and (not site.get("read_article") or article_length > 0)
    )
    result = {
        "name": site["name"],
        "url": site["url"],
        "kind": site.get("kind"),
        "ok": ok,
        "returncode": completed.returncode,
        "elapsed_sec": round(elapsed_sec, 3),
        "read_article_requested": bool(site.get("read_article")),
        "article_status": article_status,
        "article_text_length": article_length,
        "title": (
            ping.get("title")
            or page_ready.get("title")
            or article.get("title")
            if isinstance(article, dict)
            else None
        ),
        "ready_url": page_ready.get("url") or ping.get("url") if isinstance(ping, dict) else None,
        "actions_count": smoke.get("actions_count") if isinstance(smoke, dict) else None,
        "same_webview_control": smoke.get("same_webview_control") if isinstance(smoke, dict) else None,
        "termination": process.get("termination") if isinstance(process, dict) else None,
        "graceful_shutdown_ok": process.get("graceful_shutdown", {}).get("ok")
        if isinstance(process, dict)
        else None,
        "site_policy": site_policy,
        "artifacts": {
            "stdout": str(stdout_path),
            "stderr": str(stderr_path),
            "grant": str(grant_path),
            "bridge_output_dir": str(output_dir),
            "control_report": str(output_dir / "control" / "report.json"),
            "control_replay": str(output_dir / "control" / "replay.jsonl"),
        },
        "stderr_head": completed.stderr.splitlines()[:8],
    }
    if parse_error:
        result["parse_error"] = parse_error
    write_json(site_dir / "summary.json", result)
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kit", default=str(DEFAULT_KIT))
    parser.add_argument("--bridge-bin")
    parser.add_argument("--sites-json")
    parser.add_argument("--output-dir")
    parser.add_argument("--timeout-sec", type=int, default=45)
    parser.add_argument(
        "--read-article",
        action="store_true",
        help="Default read_article=true for sites supplied by --sites-json.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    sites = load_sites(args)
    if len(sites) > 8:
        raise SystemExit("refusing to run more than 8 public-site smoke checks at once")

    output_dir = (
        Path(args.output_dir).expanduser().resolve()
        if args.output_dir
        else ROOT / "runs" / "ai023_public_site_matrix" / f"matrix_{now_stamp()}"
    )
    output_dir.mkdir(parents=True, exist_ok=True)
    bridge = bridge_bin(args)

    results = []
    for index, site in enumerate(sites, start=1):
        name = safe_slug(str(site["name"]))
        site_dir = output_dir / f"{index:02d}_{name}"
        print(f"[{index}/{len(sites)}] {site['name']} {site['url']}", file=sys.stderr)
        try:
            result = run_site(args, bridge, site, site_dir)
        except Exception as error:
            result = {
                "name": site.get("name"),
                "url": site.get("url"),
                "kind": site.get("kind"),
                "ok": False,
                "error": str(error),
                "artifacts": {"site_dir": str(site_dir)},
            }
            write_json(site_dir / "summary.json", result)
        results.append(result)

    report = {
        "ok": all(result.get("ok") for result in results),
        "route": "saccade_public_site_smoke_matrix_v0",
        "kit": str(Path(args.kit).expanduser().resolve()),
        "bridge": str(bridge),
        "site_count": len(results),
        "pass_count": sum(1 for result in results if result.get("ok")),
        "fail_count": sum(1 for result in results if not result.get("ok")),
        "results": results,
    }
    write_json(output_dir / "report.json", report)
    print(json.dumps({"ok": report["ok"], "report": str(output_dir / "report.json")}, indent=2))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
