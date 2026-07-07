#!/usr/bin/env python3
"""AI-028 gate for the original mouseaccuracy.com pages.

This is a diagnostic/public-site probe, not a benchmark harness. It verifies
that current source-release ServoShell can load the original Mouse Accuracy
pages, reach gameplay state, observe targets, and dispatch browser input.
"""

from __future__ import annotations

import argparse
import base64
import json
import math
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path
from typing import Any

try:
    from PIL import Image
except ImportError as error:  # pragma: no cover - environment problem.
    raise SystemExit(f"Pillow is required for this probe: {error}") from error


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SERVOSHELL = Path("/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell")


def unix_ms() -> int:
    return int(time.time() * 1000)


def choose_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def request(port: int, method: str, path: str, payload: Any | None = None, timeout: float = 10.0):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"} if payload is not None else {}
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(req, timeout=timeout) as response:
        text = response.read().decode("utf-8", "replace")
        return response.status, json.loads(text) if text else None


def wait_for_status(port: int, proc: subprocess.Popen[str], timeout_sec: float) -> None:
    deadline = time.monotonic() + timeout_sec
    last_error = ""
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"servoshell exited before WebDriver ready: {proc.returncode}")
        try:
            request(port, "GET", "/status", timeout=0.5)
            return
        except Exception as error:  # noqa: BLE001 - report transport details.
            last_error = repr(error)
            time.sleep(0.2)
    raise TimeoutError(f"WebDriver did not become ready: {last_error}")


def session_id_from(body: dict[str, Any]) -> str:
    value = body.get("value") if isinstance(body, dict) else None
    if isinstance(value, dict) and isinstance(value.get("sessionId"), str):
        return value["sessionId"]
    if isinstance(body.get("sessionId"), str):
        return body["sessionId"]
    raise RuntimeError(f"new session response missing session id: {body}")


def execute(port: int, session_id: str, script: str, timeout: float = 10.0) -> Any:
    _, body = request(
        port,
        "POST",
        f"/session/{session_id}/execute/sync",
        {"script": script, "args": []},
        timeout=timeout,
    )
    return body.get("value") if isinstance(body, dict) else None


def navigate(port: int, session_id: str, url: str) -> None:
    request(port, "POST", f"/session/{session_id}/url", {"url": url}, timeout=30)


def wait_page_ready(port: int, session_id: str, expected_url: str, timeout_sec: float) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_sec
    last: dict[str, Any] = {}
    while time.monotonic() < deadline:
        value = execute(
            port,
            session_id,
            "return {url: location.href, title: document.title, readyState: document.readyState, bodyTextLength: document.body ? (document.body.innerText || '').length : 0};",
            timeout=3,
        )
        if isinstance(value, dict):
            last = value
            if value.get("url") == expected_url and value.get("readyState") in {"interactive", "complete"}:
                return value
        time.sleep(0.2)
    raise TimeoutError(f"page did not become ready for {expected_url}: last={last}")


def screenshot(port: int, session_id: str, path: Path) -> Path:
    _, body = request(port, "GET", f"/session/{session_id}/screenshot", timeout=10)
    raw = body.get("value") if isinstance(body, dict) else None
    if not isinstance(raw, str):
        raise RuntimeError(f"screenshot response missing base64 value: {body}")
    path.write_bytes(base64.b64decode(raw))
    return path


def try_screenshot(port: int, session_id: str, path: Path, report: dict[str, Any]) -> bool:
    try:
        screenshot(port, session_id, path)
        return True
    except Exception as error:  # noqa: BLE001 - diagnostic should keep the report.
        report.setdefault("screenshot_errors", []).append({"path": str(path), "error": repr(error)})
        return False


def pointer_click(port: int, session_id: str, x: float, y: float) -> dict[str, Any]:
    payload = {
        "actions": [
            {
                "type": "pointer",
                "id": "mouse",
                "parameters": {"pointerType": "mouse"},
                "actions": [
                    {"type": "pointerMove", "duration": 0, "origin": "viewport", "x": int(round(x)), "y": int(round(y))},
                    {"type": "pointerDown", "button": 0},
                    {"type": "pointerUp", "button": 0},
                ],
            }
        ]
    }
    try:
        code, body = request(port, "POST", f"/session/{session_id}/actions", payload, timeout=5)
        return {"ok": 200 <= code < 300, "route": "webdriver_actions", "code": code, "body": body}
    except Exception as error:  # noqa: BLE001 - fallback is part of the probe.
        script = f"""
        const x = {float(x)!r};
        const y = {float(y)!r};
        const el = document.elementFromPoint(x, y) || document.body;
        for (const type of ['mousemove', 'mousedown', 'mouseup', 'click']) {{
          el.dispatchEvent(new MouseEvent(type, {{
            bubbles: true, cancelable: true, view: window,
            clientX: x, clientY: y, button: 0, buttons: type === 'mousedown' ? 1 : 0
          }}));
        }}
        return {{tag: el && el.tagName, cls: el && el.className, x, y}};
        """
        value = execute(port, session_id, script)
        return {"ok": True, "route": "js_mouse_event_fallback", "error": repr(error), "body": value}


PAGE_PROBE_JS = r"""
return (() => {
  const rectOf = (el) => {
    const r = el.getBoundingClientRect();
    return {x:r.x, y:r.y, w:r.width, h:r.height, top:r.top, left:r.left, right:r.right, bottom:r.bottom};
  };
  const text = (el) => (el.innerText || el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 160);
  const actions = Array.from(document.querySelectorAll('button, a, input, [role=button], .go, .target'))
    .map((el, index) => ({
      index,
      tag: el.tagName,
      type: el.getAttribute('type') || '',
      id: el.id || '',
      cls: typeof el.className === 'string' ? el.className : '',
      text: text(el),
      checked: !!el.checked,
      rect: rectOf(el),
      visible: !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length),
    }))
    .filter((item) => item.visible);
  const canvases = Array.from(document.querySelectorAll('canvas')).map((el, index) => ({
    index,
    id: el.id || '',
    cls: typeof el.className === 'string' ? el.className : '',
    width: el.width,
    height: el.height,
    rect: rectOf(el),
  }));
  const targets = Array.from(document.querySelectorAll('.target')).map((el, index) => ({
    index,
    cls: typeof el.className === 'string' ? el.className : '',
    rect: rectOf(el),
    display: getComputedStyle(el).display,
    visibility: getComputedStyle(el).visibility,
  }));
  return {
    title: document.title,
    url: location.href,
    readyState: document.readyState,
    bodyTextLength: document.body ? (document.body.innerText || '').length : 0,
    viewport: {width: innerWidth, height: innerHeight, dpr: devicePixelRatio},
    actions,
    canvases,
    targets,
    scoreText: document.body ? (document.body.innerText || '').match(/SCORE\s*\n?\s*(\d+)/i)?.[1] || null : null,
    timeText: document.body ? (document.body.innerText || '').match(/TIME\s*\n?\s*(\d+)/i)?.[1] || null : null,
    classicScore: document.querySelector('#score')?.textContent || null,
    classicMissed: document.querySelector('#missed')?.textContent || null,
    classicGlobals: {
      score: typeof score !== 'undefined' ? score : null,
      clicks: typeof clicks !== 'undefined' ? clicks : null,
      missedTargets: typeof missedTargets !== 'undefined' ? missedTargets : null,
      modeSetting: typeof modeSetting !== 'undefined' ? modeSetting : null,
      targetSize: typeof tSize !== 'undefined' ? tSize : null,
    },
  };
})();
"""


def click_classic_options(port: int, session_id: str) -> list[dict[str, Any]]:
    script = """
    const click = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return {selector, ok:false};
      el.click();
      return {selector, ok:true};
    };
    return [click('#epic'), click('#tiny'), click('.go')];
    """
    return execute(port, session_id, script) or []


def click_modern_start(port: int, session_id: str, probe: dict[str, Any]) -> dict[str, Any]:
    candidates = [
        item for item in probe.get("actions", [])
        if "start" in str(item.get("text", "")).lower() or "start" in str(item.get("cls", "")).lower()
    ]
    if candidates:
        rect = candidates[0]["rect"]
        return pointer_click(port, session_id, rect["x"] + rect["w"] / 2, rect["y"] + rect["h"] / 2)
    value = execute(
        port,
        session_id,
        """
        const buttons = Array.from(document.querySelectorAll('button, [role=button], a'));
        const start = buttons.find((el) => /start/i.test(el.innerText || el.textContent || ''));
        if (start) { start.click(); return {ok:true, route:'dom_start_click', text:start.innerText || start.textContent}; }
        if (location.pathname !== '/game') { location.href = '/game'; return {ok:true, route:'navigate_game'}; }
        return {ok:false, route:'already_game_or_no_start'};
        """,
    )
    return {"ok": bool(value and value.get("ok")), "route": "execute_start_fallback", "body": value}


def colored_blob_centers(image_path: Path, max_centers: int = 8) -> list[dict[str, Any]]:
    image = Image.open(image_path).convert("RGB")
    width, height = image.size
    pixels = image.load()
    visited: set[tuple[int, int]] = set()
    centers: list[dict[str, Any]] = []

    def is_targetish(x: int, y: int) -> bool:
        r, g, b = pixels[x, y]
        if y < 55:
            return False
        return (
            (r > 180 and g < 120 and b < 140)
            or (b > 170 and r > 80 and g < 150)
            or (g > 160 and r < 180 and b < 180)
            or (r > 200 and g > 110 and b < 80)
        )

    for y in range(55, height, 2):
        for x in range(0, width, 2):
            if (x, y) in visited or not is_targetish(x, y):
                continue
            stack = [(x, y)]
            visited.add((x, y))
            xs: list[int] = []
            ys: list[int] = []
            while stack:
                cx, cy = stack.pop()
                xs.append(cx)
                ys.append(cy)
                for nx, ny in ((cx + 2, cy), (cx - 2, cy), (cx, cy + 2), (cx, cy - 2)):
                    if nx < 0 or ny < 55 or nx >= width or ny >= height or (nx, ny) in visited:
                        continue
                    if is_targetish(nx, ny):
                        visited.add((nx, ny))
                        stack.append((nx, ny))
            area = len(xs) * 4
            if area < 40:
                continue
            min_x, max_x = min(xs), max(xs)
            min_y, max_y = min(ys), max(ys)
            if max_x - min_x > width * 0.4 or max_y - min_y > height * 0.4:
                continue
            centers.append({
                "x": (min_x + max_x) / 2,
                "y": (min_y + max_y) / 2,
                "area_px": area,
                "bbox": {"x": min_x, "y": min_y, "w": max_x - min_x + 2, "h": max_y - min_y + 2},
            })
    centers.sort(key=lambda item: item["area_px"], reverse=True)
    return centers[:max_centers]


def run_probe(args: argparse.Namespace) -> dict[str, Any]:
    output_dir = args.output_dir or ROOT / "runs" / "ai028_mouseaccuracy_original" / f"gate_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)
    port = args.port or choose_port()
    cmd = [
        str(args.servoshell),
        f"--webdriver={port}",
        "--temporary-storage",
        f"--window-size={args.window_size}",
        args.url,
    ]
    if args.headless:
        cmd.insert(1, "-z")

    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    session_id = None
    report: dict[str, Any] = {
        "engine": "saccade-ai028-mouseaccuracy-original-gate-v0",
        "url": args.url,
        "cmd": cmd,
        "port": port,
        "headless": args.headless,
        "output_dir": str(output_dir),
        "checks": {},
        "artifacts": {},
    }
    try:
        wait_for_status(port, proc, args.timeout_sec)
        _, new_session = request(port, "POST", "/session", {"capabilities": {"alwaysMatch": {"browserName": "servo"}}})
        session_id = session_id_from(new_session)
        navigate(port, session_id, args.url)
        report["page_ready"] = wait_page_ready(port, session_id, args.url, args.timeout_sec)
        time.sleep(args.initial_wait_sec)

        before = execute(port, session_id, PAGE_PROBE_JS)
        report["before"] = before
        before_screenshot = output_dir / "before.png"
        if try_screenshot(port, session_id, before_screenshot, report):
            report["artifacts"]["before_screenshot"] = str(before_screenshot)

        if args.mode == "classic":
            report["start_action"] = {"route": "classic_dom_options", "items": click_classic_options(port, session_id)}
        else:
            report["start_action"] = click_modern_start(port, session_id, before)
        time.sleep(args.after_start_wait_sec)

        after_start = execute(port, session_id, PAGE_PROBE_JS)
        report["after_start"] = after_start
        after_start_screenshot = output_dir / "after_start.png"
        if try_screenshot(port, session_id, after_start_screenshot, report):
            report["artifacts"]["after_start_screenshot"] = str(after_start_screenshot)

        click_results = []
        if args.mode == "classic":
            for _ in range(args.max_clicks):
                probe = execute(port, session_id, PAGE_PROBE_JS)
                targets = [
                    target for target in probe.get("targets", [])
                    if target.get("display") != "none"
                    and target.get("visibility") != "hidden"
                    and float(target.get("rect", {}).get("w") or 0) >= 2
                    and float(target.get("rect", {}).get("h") or 0) >= 2
                ]
                targets.sort(
                    key=lambda target: (
                        float(target.get("rect", {}).get("w") or 0)
                        * float(target.get("rect", {}).get("h") or 0)
                    ),
                    reverse=True,
                )
                if not targets:
                    time.sleep(args.click_wait_sec)
                    continue
                rect = targets[0]["rect"]
                result = pointer_click(port, session_id, rect["x"] + rect["w"] / 2, rect["y"] + rect["h"] / 2)
                post_click = execute(port, session_id, PAGE_PROBE_JS)
                click_results.append({"target": targets[0], "click": result, "post_click": post_click})
                time.sleep(args.click_wait_sec)
        else:
            targets = [
                target for target in after_start.get("targets", [])
                if target.get("display") != "none"
                and target.get("visibility") != "hidden"
                and float(target.get("rect", {}).get("w") or 0) >= 2
                and float(target.get("rect", {}).get("h") or 0) >= 2
            ]
            targets.sort(
                key=lambda target: (
                    float(target.get("rect", {}).get("w") or 0)
                    * float(target.get("rect", {}).get("h") or 0)
                ),
                reverse=True,
            )
            if targets:
                candidates = [
                    {
                        "source": "dom_target_rect",
                        "target": target,
                        "x": target["rect"]["x"] + target["rect"]["w"] / 2,
                        "y": target["rect"]["y"] + target["rect"]["h"] / 2,
                    }
                    for target in targets[: args.max_clicks]
                ]
            else:
                image_width = image_height = None
                if after_start_screenshot.exists():
                    with Image.open(after_start_screenshot) as image:
                        image_width, image_height = image.size
                viewport = after_start.get("viewport") or {}
                scale_x = (image_width or viewport.get("width") or 1) / max(1, float(viewport.get("width") or 1))
                scale_y = (image_height or viewport.get("height") or 1) / max(1, float(viewport.get("height") or 1))
                candidates = [
                    {
                        "source": "screenshot_blob_scaled_to_css",
                        "center": center,
                        "x": center["x"] / scale_x,
                        "y": center["y"] / scale_y,
                        "screenshot_scale": {"x": scale_x, "y": scale_y},
                    }
                    for center in (
                        colored_blob_centers(after_start_screenshot)
                        if after_start_screenshot.exists()
                        else []
                    )
                    if center["y"] / scale_y >= 80
                ][: args.max_clicks]

            for candidate in candidates:
                result = pointer_click(port, session_id, candidate["x"], candidate["y"])
                post_click = execute(port, session_id, PAGE_PROBE_JS)
                click_results.append({
                    "candidate": candidate,
                    "click": result,
                    "screenshot": str(after_start_screenshot),
                    "post_click": post_click,
                })
                time.sleep(args.click_wait_sec)
        report["click_results"] = click_results

        final = execute(port, session_id, PAGE_PROBE_JS)
        report["final"] = final
        final_screenshot = output_dir / "final.png"
        if try_screenshot(port, session_id, final_screenshot, report):
            report["artifacts"]["final_screenshot"] = str(final_screenshot)

        report["checks"] = summarize_checks(args, report)
        report["ok"] = bool(report["checks"].get("ok"))
    finally:
        if session_id:
            try:
                request(port, "DELETE", f"/session/{session_id}", timeout=3)
            except Exception:
                pass
        proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
        report["process"] = {
            "returncode": proc.returncode,
            "stdout_head": stdout.splitlines()[:40],
            "stderr_head": stderr.splitlines()[:80],
            "gl_warning": "GLD_TEXTURE" in stderr or "texture unloadable" in stderr,
        }
        report_path = output_dir / "report.json"
        report["artifacts"]["report"] = str(report_path)
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def summarize_checks(args: argparse.Namespace, report: dict[str, Any]) -> dict[str, Any]:
    before = report.get("before") if isinstance(report.get("before"), dict) else {}
    after = report.get("after_start") if isinstance(report.get("after_start"), dict) else {}
    final = report.get("final") if isinstance(report.get("final"), dict) else {}
    click_results = report.get("click_results") if isinstance(report.get("click_results"), list) else []
    canvas_count = len(after.get("canvases") or [])
    targets_after = len(after.get("targets") or [])
    click_ok = sum(1 for item in click_results if item.get("click", {}).get("ok") is True)
    modern_candidates = sum(1 for item in click_results if item.get("candidate"))
    score_before = parse_int(
        after.get("scoreText")
        or after.get("classicScore")
        or (after.get("classicGlobals") or {}).get("score")
    )
    score_after = parse_int(
        final.get("scoreText")
        or final.get("classicScore")
        or (final.get("classicGlobals") or {}).get("score")
    )
    click_count_after = parse_int((final.get("classicGlobals") or {}).get("clicks"))
    missed_after = parse_int((final.get("classicGlobals") or {}).get("missedTargets"))
    score_delta = (
        score_after - score_before
        if score_after is not None and score_before is not None
        else None
    )
    started = before.get("url") != after.get("url") or bool(after.get("targets")) or canvas_count > 0
    observed = (targets_after > 0) if args.mode == "classic" else (canvas_count > 0 and modern_candidates > 0)
    modern_score_hits = score_delta is not None and score_delta > 0
    if args.mode == "classic":
        ok = bool(started and observed and click_ok > 0 and (score_delta or 0) > 0)
    else:
        ok = bool(started and observed and click_ok > 0 and modern_score_hits)
    return {
        "ok": ok,
        "mode": args.mode,
        "loaded": bool(before.get("title")),
        "started_or_in_game": started,
        "canvas_count_after_start": canvas_count,
        "targets_after_start": targets_after,
        "modern_click_candidates": modern_candidates,
        "click_attempts": len(click_results),
        "click_ok": click_ok,
        "score_before_clicks": score_before,
        "score_after_clicks": score_after,
        "score_delta": score_delta,
        "modern_score_hits": modern_score_hits,
        "classic_clicks_after": click_count_after,
        "classic_missed_after": missed_after,
        "route": "green" if ok else "needs_review",
    }


def parse_int(value: Any) -> int | None:
    if value is None:
        return None
    try:
        if isinstance(value, float) and math.isnan(value):
            return None
        return int(str(value).strip())
    except ValueError:
        return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="https://mouseaccuracy.com/game")
    parser.add_argument("--mode", choices=["modern", "classic"], default="modern")
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--window-size", default="1280x900")
    parser.add_argument("--timeout-sec", type=float, default=45)
    parser.add_argument("--initial-wait-sec", type=float, default=1.0)
    parser.add_argument("--after-start-wait-sec", type=float, default=4.2)
    parser.add_argument("--click-wait-sec", type=float, default=0.25)
    parser.add_argument("--max-clicks", type=int, default=8)
    parser.add_argument("--headed", dest="headless", action="store_false")
    parser.set_defaults(headless=True)
    args = parser.parse_args()

    report = run_probe(args)
    print(
        "AI028_MOUSEACCURACY_ORIGINAL "
        f"mode={args.mode} ok={str(report.get('ok')).lower()} "
        f"route={report.get('checks', {}).get('route')} "
        f"click_ok={report.get('checks', {}).get('click_ok')} "
        f"score_delta={report.get('checks', {}).get('score_delta')} "
        f"gl_warning={report.get('process', {}).get('gl_warning')} "
        f"report={report.get('artifacts', {}).get('report')}"
    )
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
