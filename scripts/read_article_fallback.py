#!/usr/bin/env python3
"""Run Saccade article extraction with a hard timeout and HTTP fallback."""

from __future__ import annotations

import argparse
import html
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import time
from typing import Any
from urllib.parse import urldefrag, urlparse
from urllib.request import Request, urlopen


class TextExtractor(HTMLParser):
    SKIP_TAGS = {"script", "style", "noscript", "svg", "canvas", "template"}
    BLOCK_TAGS = {
        "address",
        "article",
        "aside",
        "blockquote",
        "br",
        "dd",
        "div",
        "dl",
        "dt",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hr",
        "li",
        "main",
        "nav",
        "ol",
        "p",
        "pre",
        "section",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "tr",
        "ul",
    }

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._skip_depth = 0
        self._in_title = False
        self.title_parts: list[str] = []
        self.body_parts: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        tag = tag.lower()
        if tag in self.SKIP_TAGS:
            self._skip_depth += 1
            return
        if tag == "title":
            self._in_title = True
        if tag in self.BLOCK_TAGS:
            self.body_parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        tag = tag.lower()
        if tag in self.SKIP_TAGS and self._skip_depth:
            self._skip_depth -= 1
            return
        if tag == "title":
            self._in_title = False
        if tag in self.BLOCK_TAGS:
            self.body_parts.append("\n")

    def handle_data(self, data: str) -> None:
        if self._skip_depth:
            return
        text = html.unescape(data)
        if self._in_title:
            self.title_parts.append(text)
            return
        self.body_parts.append(text)

    @staticmethod
    def normalize(text: str) -> str:
        lines = []
        for raw_line in text.replace("\r", "\n").split("\n"):
            line = " ".join(raw_line.split())
            if line:
                lines.append(line)
        return "\n".join(lines)

    @property
    def title(self) -> str:
        return " ".join(" ".join(self.title_parts).split())

    @property
    def text(self) -> str:
        return self.normalize("".join(self.body_parts))


def head(text: str, limit: int = 4000) -> str:
    return text[:limit]


def safe_url(url: str) -> str:
    without_fragment, _fragment = urldefrag(url)
    return without_fragment


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def fetch_public_html(args: argparse.Namespace, public_url: str) -> tuple[str, str, int | None, str]:
    curl = shutil.which("curl")
    if curl:
        marker = b"\n__SACCADE_CURL_META__\n"
        cmd = [
            curl,
            "-L",
            "--silent",
            "--show-error",
            "--max-time",
            str(args.http_timeout_sec),
            "-A",
            "Mozilla/5.0",
            "-H",
            "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "-H",
            "Accept-Language: en-US,en;q=0.9",
            "-w",
            "\n__SACCADE_CURL_META__\n%{http_code}\n%{url_effective}\n%{content_type}\n",
            public_url,
        ]
        completed = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=args.http_timeout_sec + 5,
            check=False,
        )
        if completed.returncode == 0 and marker in completed.stdout:
            body, meta = completed.stdout.split(marker, 1)
            meta_lines = meta.decode("utf-8", errors="replace").splitlines()
            status = int(meta_lines[0]) if meta_lines and meta_lines[0].isdigit() else None
            final_url = meta_lines[1] if len(meta_lines) > 1 else public_url
            content_type = meta_lines[2] if len(meta_lines) > 2 else ""
            body = body[: args.http_max_bytes]
            charset = "utf-8"
            if "charset=" in content_type:
                charset = content_type.split("charset=", 1)[1].split(";", 1)[0].strip() or "utf-8"
            return body.decode(charset, errors="replace"), content_type, status, final_url
        if completed.returncode != 0:
            raise RuntimeError(
                "curl fallback failed: "
                + completed.stderr.decode("utf-8", errors="replace")[:1000]
            )

    request = Request(
        public_url,
        headers={
            "User-Agent": (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                "AppleWebKit/537.36 (KHTML, like Gecko) "
                "Chrome/126.0.0.0 Safari/537.36"
            ),
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "Accept-Language": "en-US,en;q=0.9",
        },
    )
    with urlopen(request, timeout=args.http_timeout_sec) as response:
        raw = response.read(args.http_max_bytes + 1)
        content_type = response.headers.get("content-type", "")
        status = getattr(response, "status", None)
        final_url = response.geturl()
    raw = raw[: args.http_max_bytes]
    charset = "utf-8"
    if "charset=" in content_type:
        charset = content_type.split("charset=", 1)[1].split(";", 1)[0].strip() or "utf-8"
    return raw.decode(charset, errors="replace"), content_type, status, final_url


def run_browser(args: argparse.Namespace) -> tuple[dict[str, Any] | None, dict[str, Any]]:
    cmd = [
        args.bin,
        "bridge",
        "--servoshell",
        args.servoshell,
        "--url",
        args.url,
        "--profile-dir",
        args.profile_dir,
        "--read-article",
        "--article-max-chars",
        str(args.article_max_chars),
        "--exit",
        "--json",
        "--grant-path",
        str(args.grant_path),
        "--output-dir",
        str(args.output_dir),
        "--timeout-sec",
        str(args.timeout_sec),
    ]
    if args.userscripts_dir:
        cmd.extend(["--userscripts-dir", args.userscripts_dir])

    started = time.monotonic()
    proc = subprocess.Popen(
        cmd,
        cwd=args.cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    timed_out = False
    try:
        stdout, stderr = proc.communicate(timeout=args.hard_timeout_sec)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            stdout, stderr = proc.communicate(timeout=5)

    elapsed_ms = round((time.monotonic() - started) * 1000)
    meta: dict[str, Any] = {
        "command": cmd,
        "elapsed_ms": elapsed_ms,
        "returncode": proc.returncode,
        "timed_out": timed_out,
        "timeout_sec": args.timeout_sec,
        "hard_timeout_sec": args.hard_timeout_sec,
        "stdout_head": head(stdout),
        "stderr_head": head(stderr),
    }
    if stdout.strip():
        try:
            parsed = json.loads(stdout)
            meta["json_parse_ok"] = True
            return parsed, meta
        except json.JSONDecodeError as exc:
            meta["json_parse_ok"] = False
            meta["json_parse_error"] = str(exc)
    return None, meta


def http_article_fallback(args: argparse.Namespace, browser_meta: dict[str, Any]) -> dict[str, Any]:
    parsed = urlparse(args.url)
    if parsed.scheme not in {"http", "https"}:
        raise ValueError(f"HTTP fallback only supports http/https URLs, got {parsed.scheme!r}")
    public_url = safe_url(args.url)
    html_text, content_type, status, final_url = fetch_public_html(args, public_url)
    truncated_bytes = len(html_text.encode("utf-8", errors="replace")) >= args.http_max_bytes
    extractor = TextExtractor()
    extractor.feed(html_text)
    text = extractor.text
    returned = text[: args.article_max_chars]
    return {
        "ok": True,
        "engine": "saccade-read-article-fallback-v0",
        "runtime": "official_http_fallback_no_browser_cookies",
        "route": "http_article_fallback",
        "url": public_url,
        "final_url": safe_url(final_url),
        "title": extractor.title,
        "article_text": {
            "status": "fallback_ok",
            "runtime": "official_http_fallback_no_browser_cookies",
            "engine": "saccade-http-article-text-v0",
            "summary": "public page text extracted through HTTP fallback after Saccade browser article extraction failed or timed out",
            "same_webview_control": False,
            "url": safe_url(final_url),
            "title": extractor.title,
            "body_text_length": len(text),
            "article_text_length": len(text),
            "text_chars_returned": len(returned),
            "text_truncated": len(returned) < len(text) or truncated_bytes,
            "text": returned,
            "extraction": {
                "mode": "http_html_text_fallback",
                "selector": None,
                "content_type": content_type,
                "http_status": status,
                "bytes_truncated": truncated_bytes,
            },
            "policy": {
                "browser_profile_used": False,
                "cookies_sent": False,
                "login_required": False,
                "public_reference_only": True,
            },
        },
        "saccade_browser": {
            "ok": False,
            "route": "browser_article_unavailable",
            **browser_meta,
        },
        "fallback_reason": fallback_reason(browser_meta),
        "artifacts": {
            "report": str(args.output_dir / "report.json"),
        },
    }


def fallback_reason(meta: dict[str, Any]) -> str:
    if meta.get("timed_out"):
        return "saccade_browser_article_hard_timeout"
    if meta.get("returncode") not in (0, None):
        return "saccade_browser_article_nonzero_exit"
    if meta.get("json_parse_ok") is False:
        return "saccade_browser_article_invalid_json"
    return "saccade_browser_article_missing_json"


def run_selftest() -> None:
    sample = """
    <html><head><title>Example Title</title><script>secret()</script></head>
    <body><main><h1>Hello</h1><p>One&nbsp;two</p><style>.x{}</style></main></body></html>
    """
    extractor = TextExtractor()
    extractor.feed(sample)
    assert extractor.title == "Example Title"
    assert "Hello" in extractor.text
    assert "One two" in extractor.text
    assert "secret" not in extractor.text
    print("READ_ARTICLE_FALLBACK_SELFTEST ok=true")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--bin")
    parser.add_argument("--servoshell")
    parser.add_argument("--url")
    parser.add_argument("--profile-dir")
    parser.add_argument("--userscripts-dir")
    parser.add_argument("--cwd", default=os.getcwd())
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--grant-path", type=Path)
    parser.add_argument("--article-max-chars", type=int, default=30_000)
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    parser.add_argument("--hard-timeout-sec", type=float, default=50.0)
    parser.add_argument("--http-timeout-sec", type=float, default=20.0)
    parser.add_argument("--http-max-bytes", type=int, default=2_000_000)
    parser.add_argument("--fallback", choices=["auto", "off"], default="auto")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.selftest:
        run_selftest()
        return 0

    required = ["bin", "servoshell", "url", "profile_dir", "output_dir", "grant_path"]
    missing = [name for name in required if getattr(args, name) in (None, "")]
    if missing:
        raise SystemExit(f"missing required arguments: {', '.join(missing)}")

    args.output_dir.mkdir(parents=True, exist_ok=True)
    browser_json, browser_meta = run_browser(args)
    if browser_json and browser_meta.get("returncode") == 0:
        browser_json.setdefault("fallback_reason", None)
        browser_json.setdefault("saccade_browser", {"ok": True, **browser_meta})
        write_json(args.output_dir / "report.json", browser_json)
        print(json.dumps(browser_json, indent=2, ensure_ascii=False))
        return 0

    if args.fallback == "off":
        report = {
            "ok": False,
            "engine": "saccade-read-article-fallback-v0",
            "runtime": "browser_only_no_fallback",
            "url": safe_url(args.url),
            "saccade_browser": {"ok": False, **browser_meta},
            "fallback_reason": fallback_reason(browser_meta),
            "artifacts": {"report": str(args.output_dir / "report.json")},
        }
        write_json(args.output_dir / "report.json", report)
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 1

    try:
        report = http_article_fallback(args, browser_meta)
        write_json(args.output_dir / "report.json", report)
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 0
    except Exception as exc:  # noqa: BLE001 - report fallback failures as data.
        report = {
            "ok": False,
            "engine": "saccade-read-article-fallback-v0",
            "runtime": "official_http_fallback_no_browser_cookies",
            "route": "http_article_fallback",
            "url": safe_url(args.url),
            "saccade_browser": {"ok": False, **browser_meta},
            "fallback_reason": fallback_reason(browser_meta),
            "fallback_error": str(exc),
            "artifacts": {"report": str(args.output_dir / "report.json")},
        }
        write_json(args.output_dir / "report.json", report)
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
