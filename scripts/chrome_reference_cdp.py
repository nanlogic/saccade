#!/usr/bin/env python3
import argparse
import base64
import hashlib
import json
import os
import pathlib
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request


DEFAULT_BLOCK_PATTERNS = [
    "*://*.2mdn.net/*",
    "*://*.adform.net/*",
    "*://*.adnxs.com/*",
    "*://*.adsrvr.org/*",
    "*://*.amazon-adsystem.com/*",
    "*://*.analytics.google.com/*",
    "*://*.criteo.com/*",
    "*://*.doubleclick.net/*",
    "*://*.facebook.net/*",
    "*://*.google-analytics.com/*",
    "*://*.googleadservices.com/*",
    "*://*.googlesyndication.com/*",
    "*://*.googletagservices.com/*",
    "*://*.moatads.com/*",
    "*://*.outbrain.com/*",
    "*://*.pubmatic.com/*",
    "*://*.quantserve.com/*",
    "*://*.rubiconproject.com/*",
    "*://*.scorecardresearch.com/*",
    "*://*.taboola.com/*",
]

AGGRESSIVE_BLOCK_PATTERNS = [
    "*://*/*.avi*",
    "*://*/*.flv*",
    "*://*/*.m3u8*",
    "*://*/*.mov*",
    "*://*/*.mp4*",
    "*://*/*.m4v*",
    "*://*/*.webm*",
    "*://*.googletagmanager.com/*",
]

PROBE_JS = r"""
JSON.stringify((() => {
  const viewport = {
    width: window.innerWidth || 0,
    height: window.innerHeight || 0,
    devicePixelRatio: window.devicePixelRatio || 1
  };
  const body = document.body;
  const bodyTextLength = body ? ((body.innerText || body.textContent || "").trim().length) : 0;

  function rectOf(el) {
    const rect = el.getBoundingClientRect();
    return {
      left: rect.left,
      top: rect.top,
      right: rect.right,
      bottom: rect.bottom,
      width: rect.width,
      height: rect.height
    };
  }

  function centerOf(rect) {
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  }

  function visibleRect(rect) {
    return rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.bottom > 0 &&
      rect.left < viewport.width && rect.top < viewport.height;
  }

  function offscreen(rect) {
    return rect.width > 0 && rect.height > 0 &&
      (rect.right <= 0 || rect.bottom <= 0 || rect.left >= viewport.width || rect.top >= viewport.height);
  }

  function label(el) {
    if (!el) return "";
    if (el.id) return "#" + el.id;
    if (el.className && typeof el.className === "string") return "." + el.className.trim().split(/\s+/).join(".");
    const text = (el.innerText || el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 40);
    return el.tagName.toLowerCase() + (text ? ":" + text : "");
  }

  function fieldToken(el) {
    return [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
  }

  function sensitivityOf(el) {
    const token = fieldToken(el);
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(passport|driver|license|national id|government)\b/.test(token)) return "government_or_tax_id";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  function safeLabel(el, sensitivity) {
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    const role = el.getAttribute("role") || "";
    const isCommandInput = tag === "input" && ["button", "submit", "reset"].includes(type);
    if (tag === "button" || tag === "a" || role === "button" || isCommandInput) {
      return (el.innerText || el.textContent || el.getAttribute("aria-label") || el.value || el.getAttribute("href") || el.tagName).trim();
    }
    const descriptor = (el.getAttribute("aria-label") || el.getAttribute("placeholder") || el.getAttribute("name") || el.id || el.tagName).trim();
    return sensitivity === "none" ? descriptor : `${descriptor || tag} (${sensitivity})`;
  }

  function completionState(el, sensitivity) {
    if (sensitivity === "none") return "not_sensitive";
    const hasEntry = el.type === "checkbox" || el.type === "radio" ? !!el.checked : !!String(el.value || "");
    return hasEntry ? "completed_without_value" : "requires_user_input";
  }

  function layoutProbeOf(el) {
    const style = getComputedStyle(el);
    return {
      name: el.getAttribute("data-saccade-probe") || "",
      tag: el.tagName.toLowerCase(),
      rect: rectOf(el),
      display: style.display || "",
      position: style.position || "",
      gridTemplateColumns: style.gridTemplateColumns || "",
      gridTemplateRows: style.gridTemplateRows || "",
      columnGap: style.columnGap || "",
      rowGap: style.rowGap || "",
      flexDirection: style.flexDirection || "",
      width: style.width || "",
      height: style.height || "",
      maxWidth760: window.matchMedia ? window.matchMedia("(max-width: 760px)").matches : null
    };
  }

  const elements = Array.from(document.querySelectorAll("body *"));
  const blockers = elements.map((el, index) => {
    const style = getComputedStyle(el);
    const rect = rectOf(el);
    return { el, index, style, rect };
  }).filter(item => {
    const pos = item.style.position;
    const pointer = item.style.pointerEvents;
    const visible = item.style.display !== "none" && item.style.visibility !== "hidden" && item.style.opacity !== "0";
    const area = item.rect.width * item.rect.height;
    return visible && pointer !== "none" && area > 1000 && visibleRect(item.rect) &&
      (pos === "fixed" || pos === "absolute");
  });

  const actions = Array.from(document.querySelectorAll("button, a, input, select, textarea, [role='button'], [onclick]")).map((el, index) => {
    const rect = rectOf(el);
    const center = centerOf(rect);
    const style = getComputedStyle(el);
    const sensitivity = sensitivityOf(el);
    const action = {
      action_id: safeLabel(el, sensitivity).toLowerCase() === "submit" ? "act_submit" : `act_${index}`,
      label: safeLabel(el, sensitivity),
      tag: el.tagName.toLowerCase(),
      kind: "click",
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      enabled: false,
      rect,
      offscreen: offscreen(rect),
      visible: visibleRect(rect) && style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0",
      blocked_by: null,
      sensitivity: {
        kind: sensitivity,
        completion_state: completionState(el, sensitivity)
      }
    };
    for (const blocker of blockers) {
      if (blocker.el === el || el.contains(blocker.el) || blocker.el.contains(el)) continue;
      const b = blocker.rect;
      if (center.x >= b.left && center.x <= b.right && center.y >= b.top && center.y <= b.bottom) {
        action.blocked_by = label(blocker.el);
        break;
      }
    }
    action.enabled = !action.disabled && action.visible && !action.offscreen && !action.blocked_by;
    return action;
  });

  return {
    engine: "chrome-cdp-reference-v1",
    title: document.title || "",
    url: location.href,
    viewport,
    bodyTextLength,
    bodyChildCount: body ? body.children.length : 0,
    scroll: {
      x: window.scrollX || 0,
      y: window.scrollY || 0,
      width: Math.max(document.documentElement.scrollWidth || 0, body ? body.scrollWidth || 0 : 0),
      height: Math.max(document.documentElement.scrollHeight || 0, body ? body.scrollHeight || 0 : 0)
    },
    layoutProbes: Array.from(document.querySelectorAll("[data-saccade-probe]")).map(layoutProbeOf),
    actions
  };
})())
"""


class CdpError(Exception):
    pass


class MiniWebSocket:
    def __init__(self, ws_url, timeout):
        parsed = urllib.parse.urlparse(ws_url)
        if parsed.scheme != "ws":
            raise CdpError(f"only ws:// CDP endpoints are supported: {ws_url}")
        port = parsed.port or 80
        self.sock = socket.create_connection((parsed.hostname, port), timeout=timeout)
        self.sock.settimeout(0.5)
        path = parsed.path or "/"
        if parsed.query:
            path += "?" + parsed.query
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        request = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {parsed.netloc}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            "\r\n"
        ).encode("ascii")
        self.sock.sendall(request)
        response = self._read_http_response()
        if b" 101 " not in response.split(b"\r\n", 1)[0]:
            raise CdpError(f"CDP websocket handshake failed: {response[:120]!r}")

    def close(self):
        try:
            self._send_frame(0x8, b"")
        except OSError:
            pass
        self.sock.close()

    def send_json(self, value):
        payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
        self._send_frame(0x1, payload)

    def recv_json(self):
        while True:
            opcode, payload = self._recv_frame()
            if opcode == 0x1:
                return json.loads(payload.decode("utf-8"))
            if opcode == 0x8:
                raise CdpError("CDP websocket closed")
            if opcode == 0x9:
                self._send_frame(0xA, payload)

    def _read_http_response(self):
        chunks = []
        data = b""
        while b"\r\n\r\n" not in data:
            chunk = self.sock.recv(4096)
            if not chunk:
                break
            chunks.append(chunk)
            data = b"".join(chunks)
        return data

    def _send_frame(self, opcode, payload):
        header = bytearray([0x80 | opcode])
        length = len(payload)
        if length < 126:
            header.append(0x80 | length)
        elif length < 65536:
            header.append(0x80 | 126)
            header.extend(struct.pack("!H", length))
        else:
            header.append(0x80 | 127)
            header.extend(struct.pack("!Q", length))
        mask = os.urandom(4)
        header.extend(mask)
        masked = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        self.sock.sendall(bytes(header) + masked)

    def _recv_frame(self):
        first = self._read_exact(2)
        b1, b2 = first[0], first[1]
        opcode = b1 & 0x0F
        length = b2 & 0x7F
        if length == 126:
            length = struct.unpack("!H", self._read_exact(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self._read_exact(8))[0]
        mask = self._read_exact(4) if b2 & 0x80 else None
        payload = self._read_exact(length)
        if mask:
            payload = bytes(byte ^ mask[index % 4] for index, byte in enumerate(payload))
        return opcode, payload

    def _read_exact(self, length):
        data = bytearray()
        while len(data) < length:
            chunk = self.sock.recv(length - len(data))
            if not chunk:
                raise CdpError("CDP websocket closed while reading frame")
            data.extend(chunk)
        return bytes(data)


class CdpClient:
    def __init__(self, ws_url, timeout):
        self.ws = MiniWebSocket(ws_url, timeout)
        self.next_id = 0
        self.events = []
        self.requests = {}

    def close(self):
        self.ws.close()

    def call(self, method, params=None, timeout=10):
        self.next_id += 1
        request_id = self.next_id
        payload = {"id": request_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.ws.send_json(payload)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                message = self.ws.recv_json()
            except socket.timeout:
                continue
            if message.get("id") == request_id:
                if "error" in message:
                    raise CdpError(f"{method} failed: {message['error']}")
                return message.get("result", {})
            self._handle_event(message)
        raise TimeoutError(f"timed out waiting for {method}")

    def wait_for_event(self, method, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                message = self.ws.recv_json()
            except socket.timeout:
                continue
            if message.get("method") == method:
                self._handle_event(message)
                return message
            self._handle_event(message)
        return None

    def drain(self, duration):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            try:
                self._handle_event(self.ws.recv_json())
            except socket.timeout:
                continue

    def _handle_event(self, message):
        method = message.get("method")
        params = message.get("params", {})
        if method:
            self.events.append({"method": method, "params": params})
        if method == "Network.requestWillBeSent":
            request = params.get("request", {})
            self.requests[params.get("requestId")] = {
                "url": safe_url(request.get("url", "")),
                "host": safe_host(request.get("url", "")),
                "method": request.get("method"),
                "type": params.get("type", "Other"),
                "status": None,
                "failed": False,
                "blocked": False,
                "blocked_reason": None,
                "encoded_bytes": 0,
            }
        elif method == "Network.responseReceived":
            entry = self.requests.setdefault(params.get("requestId"), {})
            response = params.get("response", {})
            entry["type"] = params.get("type", entry.get("type", "Other"))
            entry["status"] = response.get("status")
            entry["mime_type"] = response.get("mimeType")
            entry["url"] = safe_url(response.get("url", entry.get("url", "")))
            entry["host"] = safe_host(response.get("url", entry.get("url", "")))
        elif method == "Network.loadingFailed":
            entry = self.requests.setdefault(params.get("requestId"), {})
            entry["type"] = params.get("type", entry.get("type", "Other"))
            entry["failed"] = True
            entry["error_text"] = params.get("errorText")
            entry["blocked_reason"] = params.get("blockedReason")
            entry["blocked"] = bool(params.get("blockedReason")) or "blocked" in str(params.get("errorText", "")).lower()
        elif method == "Network.loadingFinished":
            entry = self.requests.setdefault(params.get("requestId"), {})
            entry["encoded_bytes"] = params.get("encodedDataLength", 0)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Capture a Chrome-rendered screenshot plus redacted page truth through CDP."
    )
    parser.add_argument("url")
    parser.add_argument("output_dir")
    parser.add_argument("width", nargs="?", type=int, default=1920)
    parser.add_argument("height", nargs="?", type=int, default=1080)
    parser.add_argument("--timeout-sec", type=float, default=float(os.environ.get("SACCADE_CHROME_TIMEOUT_SEC", "30")))
    parser.add_argument("--settle-ms", type=int, default=int(os.environ.get("SACCADE_CHROME_SETTLE_MS", "1000")))
    parser.add_argument(
        "--block-mode",
        choices=["none", "balanced", "aggressive"],
        default=os.environ.get("SACCADE_CHROME_BLOCK", "balanced"),
    )
    parser.add_argument("--block-file", default=os.environ.get("SACCADE_CHROME_BLOCK_FILE"))
    return parser.parse_args()


def main():
    args = parse_args()
    output_dir = pathlib.Path(args.output_dir).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    chrome = find_chrome()
    user_data_dir = tempfile.mkdtemp(prefix="saccade-chrome-")
    stderr_log = output_dir / "chrome_stderr.log"
    port = free_port()
    process = None
    client = None
    started_ms = unix_ms()
    try:
        process = launch_chrome(chrome, port, user_data_dir, args.width, args.height, stderr_log)
        target, client = wait_for_cdp_client(port, args.timeout_sec)

        block_patterns = load_block_patterns(args.block_mode, args.block_file)
        client.call("Page.enable")
        client.call("Runtime.enable")
        client.call("Network.enable")
        client.call("Network.setCacheDisabled", {"cacheDisabled": True})
        if block_patterns:
            client.call("Network.setBlockedURLs", {"urls": block_patterns})
        client.call(
            "Emulation.setDeviceMetricsOverride",
            {
                "width": args.width,
                "height": args.height,
                "deviceScaleFactor": 1,
                "mobile": False,
            },
        )
        client.call("Page.navigate", {"url": args.url})
        load_event = client.wait_for_event("Page.loadEventFired", args.timeout_sec)
        load_status = "load_event" if load_event else "timeout_continue"
        if not load_event:
            try:
                client.call("Page.stopLoading", timeout=2)
            except Exception:
                pass
        client.drain(max(0, args.settle_ms) / 1000)

        truth_result = client.call(
            "Runtime.evaluate",
            {"expression": PROBE_JS, "returnByValue": True, "awaitPromise": True},
        )
        truth_json = truth_result.get("result", {}).get("value", "{}")
        truth = json.loads(truth_json)
        screenshot_data = capture_screenshot(client)
        screenshot_path = output_dir / "chrome_page.png"
        screenshot_path.write_bytes(base64.b64decode(screenshot_data))

        truth_path = output_dir / "chrome_truth.json"
        truth_path.write_text(json.dumps(truth, indent=2, sort_keys=True) + "\n")
        network = network_summary(client.requests)
        network_path = output_dir / "chrome_network.json"
        network_path.write_text(json.dumps(network, indent=2, sort_keys=True) + "\n")
        manifest = build_manifest(
            args,
            chrome,
            target,
            block_patterns,
            truth,
            network,
            load_status,
            started_ms,
        )
        manifest_path = output_dir / "chrome_reference_manifest.json"
        manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        print(
            "CHROME REFERENCE READY "
            f"screenshot={screenshot_path} manifest={manifest_path} "
            f"actions={len(truth.get('actions', []))} blocked={network['blocked_requests']}"
        )
        print("Use only with local fixtures or non-sensitive pages; screenshots capture visible page values.")
    finally:
        if client:
            try:
                client.close()
            except Exception:
                pass
        if process:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
        shutil.rmtree(user_data_dir, ignore_errors=True)


def find_chrome():
    env = os.environ.get("CHROME")
    if env:
        if pathlib.Path(env).is_file() and os.access(env, os.X_OK):
            return env
        raise SystemExit(f"CHROME is set but not executable: {env}")
    candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        shutil.which("google-chrome"),
        shutil.which("chromium"),
        shutil.which("chromium-browser"),
        shutil.which("microsoft-edge"),
        shutil.which("brave-browser"),
    ]
    for candidate in candidates:
        if candidate and pathlib.Path(candidate).is_file() and os.access(candidate, os.X_OK):
            return candidate
    raise SystemExit("could not find Chrome/Chromium; set CHROME=/path/to/browser")


def launch_chrome(chrome, port, user_data_dir, width, height, stderr_log):
    args = [
        chrome,
        "--headless=new",
        "--disable-gpu",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-crash-reporter",
        "--disable-default-apps",
        "--disable-features=OptimizationHints,MediaRouter",
        "--disable-popup-blocking",
        "--disable-sync",
        "--metrics-recording-only",
        "--no-first-run",
        "--no-default-browser-check",
        "--password-store=basic",
        "--use-mock-keychain",
        f"--user-data-dir={user_data_dir}",
        "--force-device-scale-factor=1",
        f"--window-size={width},{height}",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={port}",
        "about:blank",
    ]
    return subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=stderr_log.open("wb"))


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_page_target(port, timeout):
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            targets = http_json(port, "/json/list")
            for target in targets:
                if target.get("type") == "page" and target.get("webSocketDebuggerUrl"):
                    return target
        except Exception as error:
            last_error = error
        time.sleep(0.1)
    raise TimeoutError(f"Chrome did not expose a page target: {last_error}")


def wait_for_cdp_client(port, timeout):
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            target = wait_for_page_target(port, min(1.0, max(0.1, deadline - time.monotonic())))
            return target, CdpClient(target["webSocketDebuggerUrl"], max(1.0, deadline - time.monotonic()))
        except Exception as error:
            last_error = error
            time.sleep(0.2)
    raise TimeoutError(f"Chrome CDP websocket did not become ready: {last_error}")


def http_json(port, path):
    with urllib.request.urlopen(f"http://127.0.0.1:{port}{path}", timeout=1) as response:
        return json.loads(response.read().decode("utf-8"))


def capture_screenshot(client):
    params = {
        "format": "png",
        "fromSurface": True,
        "captureBeyondViewport": False,
        "optimizeForSpeed": True,
    }
    try:
        return client.call("Page.captureScreenshot", params, timeout=15)["data"]
    except CdpError:
        params.pop("optimizeForSpeed", None)
        return client.call("Page.captureScreenshot", params, timeout=15)["data"]


def load_block_patterns(mode, block_file):
    patterns = []
    if mode in ("balanced", "aggressive"):
        patterns.extend(DEFAULT_BLOCK_PATTERNS)
    if mode == "aggressive":
        patterns.extend(AGGRESSIVE_BLOCK_PATTERNS)
    if block_file:
        with open(block_file, "r", encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line and not line.startswith("#"):
                    patterns.append(line)
    seen = set()
    unique = []
    for pattern in patterns:
        if pattern not in seen:
            seen.add(pattern)
            unique.append(pattern)
    return unique


def network_summary(requests):
    rows = [value for key, value in sorted(requests.items(), key=lambda item: str(item[0])) if value.get("url")]
    by_type = {}
    by_host = {}
    blocked_hosts = {}
    encoded_bytes = 0
    failed = 0
    blocked = 0
    for row in rows:
        by_type[row.get("type", "Other")] = by_type.get(row.get("type", "Other"), 0) + 1
        host = row.get("host") or ""
        if host:
            by_host[host] = by_host.get(host, 0) + 1
        if row.get("failed"):
            failed += 1
        if row.get("blocked"):
            blocked += 1
            if host:
                blocked_hosts[host] = blocked_hosts.get(host, 0) + 1
        encoded_bytes += int(row.get("encoded_bytes") or 0)
    return {
        "requests_total": len(rows),
        "failed_requests": failed,
        "blocked_requests": blocked,
        "encoded_bytes": encoded_bytes,
        "resource_types": dict(sorted(by_type.items())),
        "top_hosts": top_counts(by_host, 20),
        "blocked_hosts": top_counts(blocked_hosts, 20),
        "requests": rows[:300],
        "truncated": len(rows) > 300,
    }


def top_counts(values, limit):
    return [
        {"host": host, "count": count}
        for host, count in sorted(values.items(), key=lambda item: (-item[1], item[0]))[:limit]
    ]


def safe_url(value):
    if not value:
        return ""
    try:
        parsed = urllib.parse.urlparse(value)
        netloc = parsed.hostname or ""
        if parsed.port:
            netloc = f"{netloc}:{parsed.port}"
        path = parsed.path or "/"
        return urllib.parse.urlunparse((parsed.scheme, netloc, path, "", "", ""))
    except Exception:
        return value.split("?", 1)[0].split("#", 1)[0]


def safe_host(value):
    try:
        return urllib.parse.urlparse(value).hostname or ""
    except Exception:
        return ""


def build_manifest(args, chrome, target, block_patterns, truth, network, load_status, started_ms):
    sensitive_fields = sum(
        1
        for action in truth.get("actions", [])
        if action.get("sensitivity", {}).get("kind", "none") != "none"
    )
    return {
        "engine": "chrome-cdp-reference-v1",
        "url": args.url,
        "captured_at_unix_ms": started_ms,
        "browser_binary": chrome,
        "browser_product": target.get("browser") or target.get("description") or "unknown",
        "viewport": {
            "width": args.width,
            "height": args.height,
            "device_scale_factor": 1,
        },
        "navigation": {
            "load_status": load_status,
            "timeout_sec": args.timeout_sec,
            "settle_ms": args.settle_ms,
        },
        "block_policy": {
            "mode": args.block_mode,
            "patterns_count": len(block_patterns),
            "patterns_sha256": sha256_text("\n".join(block_patterns)),
            "blocked_requests": network["blocked_requests"],
            "patterns": block_patterns,
        },
        "page": {
            "title": truth.get("title", ""),
            "body_text_length": truth.get("bodyTextLength", 0),
            "body_child_count": truth.get("bodyChildCount", 0),
            "actions": len(truth.get("actions", [])),
            "sensitive_fields": sensitive_fields,
        },
        "network": {
            "requests_total": network["requests_total"],
            "failed_requests": network["failed_requests"],
            "blocked_requests": network["blocked_requests"],
            "encoded_bytes": network["encoded_bytes"],
        },
        "artifacts": {
            "page_screenshot": "chrome_page.png",
            "truth": "chrome_truth.json",
            "network": "chrome_network.json",
            "stderr_log": "chrome_stderr.log",
        },
        "sources": {
            "cdp_network": "https://chromedevtools.github.io/devtools-protocol/tot/Network/",
            "cdp_page": "https://chromedevtools.github.io/devtools-protocol/tot/Page/",
            "cdp_runtime": "https://chromedevtools.github.io/devtools-protocol/tot/Runtime/",
        },
        "note": "Chrome-rendered page-content screenshot plus redacted page truth. This is not a browser-UI screenshot with URL bar.",
    }


def sha256_text(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def unix_ms():
    return int(time.time() * 1000)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"Chrome reference capture failed: {error}", file=sys.stderr)
        sys.exit(1)
