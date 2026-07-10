#!/usr/bin/env python3
"""Loopback current-tab bridge for a visible Chrome compatibility session.

The adapter deliberately exposes only redacted truth, action maps, browser-input
clicks for low-risk actions, and browser navigation. It never exports profile
state or field values, and it refuses sensitive or side-effecting actions.
"""

import argparse
import hashlib
import json
import pathlib
import secrets
import socket
import threading
import time
import urllib.parse

import chrome_compat_cdp as compat
import chrome_reference_cdp as reference


RUNTIME = "saccade-chrome-compat-cdp-v0"
PROTOCOL = "saccade-dogfood-control-v0"
SAFE_ACTION_BLOCK = (
    "submit",
    "publish",
    "post",
    "send",
    "delete",
    "remove",
    "pay",
    "checkout",
    "buy",
    "release",
    "sign",
    "confirm",
    "save changes",
    "sign out",
    "log out",
)

TEXT_REVISION_JS = r"""
(() => {
  const source = __SACCADE_REVISION_SALT__ + "\n" + String(document.body?.innerText || "");
  let hash = 2166136261;
  for (let index = 0; index < source.length; index += 1) {
    hash ^= source.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16);
})()
"""

FILL_FIELDS_JS = r"""
(() => {
  const requested = __SACCADE_FIELDS__;
  const controls = Array.from(document.querySelectorAll('input, textarea, select'));
  const visible = el => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden';
  };
  const sensitivityOf = el => {
    const token = [el.dataset.sensitive || '', el.autocomplete || '', el.name || '', el.id || '', el.type || ''].join(' ').toLowerCase();
    if ((el.type || '').toLowerCase() === 'password' || /\b(password|passcode)\b/.test(token)) return 'password';
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return 'otp';
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver|license|government)\b/.test(token)) return 'government_or_tax_id';
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return 'payment';
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return 'legal_attestation';
    return 'none';
  };
  const find = key => controls.find(el => visible(el) && (el.id === key || el.name === key));
  const filled = [];
  const rejected = [];
  const sensitiveFieldsSeen = [];
  for (const [key, value] of Object.entries(requested)) {
    const el = find(key);
    if (!el) { rejected.push({ field: key, reason: 'field_not_found' }); continue; }
    const sensitive = sensitivityOf(el);
    if (sensitive !== 'none') {
      sensitiveFieldsSeen.push({ field: key, kind: sensitive });
      rejected.push({ field: key, reason: 'sensitive_field' });
      continue;
    }
    const owner = String(el.dataset.owner || el.dataset.saccadeOwner || 'human').toLowerCase();
    if (owner !== 'agent') { rejected.push({ field: key, reason: 'not_agent_owned' }); continue; }
    if (String(el.value || '').length > 0) { rejected.push({ field: key, reason: 'already_has_user_value' }); continue; }
    if (el.type === 'file' || el.type === 'hidden') { rejected.push({ field: key, reason: 'unsupported_field_type' }); continue; }
    const text = String(value);
    if (el.tagName.toLowerCase() === 'select') {
      el.value = text;
    } else if (el.tagName.toLowerCase() === 'textarea') {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    } else {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, text);
    }
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    filled.push({ field: key, value_logged: false });
  }
  return { filled, rejected, sensitiveFieldsSeen };
})()
"""

INSPECT_FIELDS_JS = r"""
(() => {
  const requested = __SACCADE_FIELDS__;
  const controls = Array.from(document.querySelectorAll('input, textarea, select'));
  const visible = el => {
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden';
  };
  const sensitivityOf = el => {
    const token = [el.dataset.sensitive || '', el.autocomplete || '', el.name || '', el.id || '', el.type || ''].join(' ').toLowerCase();
    if ((el.type || '').toLowerCase() === 'password' || /\b(password|passcode)\b/.test(token)) return 'password';
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return 'otp';
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver|license|government)\b/.test(token)) return 'government_or_tax_id';
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return 'payment';
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return 'legal_attestation';
    return 'none';
  };
  const fields = requested.map(key => {
    const el = controls.find(item => visible(item) && (item.id === key || item.name === key));
    if (!el) return { field: key, found: false, value_returned: false, value_redacted: false };
    const sensitive = sensitivityOf(el);
    return {
      field: key,
      found: true,
      visible: true,
      owner: String(el.dataset.owner || el.dataset.saccadeOwner || 'human').toLowerCase(),
      sensitivity: sensitive,
      completion_state: String(el.value || '').length > 0 ? 'completed_without_value' : 'requires_user_input',
      value_returned: false,
      value_redacted: sensitive !== 'none',
    };
  });
  return { fields, sensitiveFieldsSeen: fields.filter(item => item.sensitivity && item.sensitivity !== 'none').map(item => ({ field: item.field, kind: item.sensitivity })) };
})()
"""


def parse_args():
    parser = argparse.ArgumentParser(
        description="Serve the Saccade current-tab control protocol for a Chrome compatibility window."
    )
    parser.add_argument("--cdp-port", type=int, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--grant-path", type=pathlib.Path, required=True)
    parser.add_argument("--initial-url", required=True)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def write_json_atomic(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


class ChromeCompatControl:
    def __init__(self, args, client):
        self.args = args
        self.client = client
        self.output_dir = args.output_dir.resolve() / "control"
        self.report_path = self.output_dir / "report.json"
        self.replay_path = self.output_dir / "replay.jsonl"
        self.page_revision = 1
        self.last_fingerprint = None
        self.revision_salt = secrets.token_hex(16)
        self.shutdown = threading.Event()
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen()
        self.listener.settimeout(0.25)
        self.port = self.listener.getsockname()[1]

    def endpoint(self):
        return {
            "protocol": PROTOCOL,
            "scheme": "tcp",
            "host": "127.0.0.1",
            "port": self.port,
        }

    def artifacts(self):
        return {
            "run_dir": str(self.output_dir.parent),
            "report": str(self.report_path),
            "replay": str(self.replay_path),
        }

    def copilot(self):
        return {
            "status": "granted",
            "badge": "Compatibility Granted",
            "owner": "Human",
            "read_grant": "FullTruth",
            "agent_input_grant": True,
            "user_confirmation_required_for_side_effects": True,
            "sensitive_values_visible_to_user": True,
            "sensitive_values_exposed_to_agent": False,
            "page_dom_injected": False,
            "visible_ui": "chrome_compatibility_external_window",
        }

    def write_grant(self):
        status = self.snapshot()
        payload = {
            "status": "granted",
            "runtime": RUNTIME,
            "grant_type": "current_tab_copilot",
            "selected_tab_seen": True,
            "grant_required": True,
            "grant_given": True,
            "owner": "Human",
            "read_grant": "FullTruth",
            "agent_input_grant": True,
            "copilot": self.copilot(),
            "url": status["url"],
            "title": status["title"],
            "rendering_profile": "chrome-compatibility",
            "mcp_tool": "saccade.tabs.grant_current",
            "control_endpoint": self.endpoint(),
            "transport_status": "chrome_compatibility_control_v0",
            "note": "Explicit Human grant for a visible Chrome compatibility tab. Supports redacted truth/actions, low-risk browser-input act, and navigation. Sensitive fields, provider challenges, and side-effecting actions remain user-owned.",
            "written_unix_ms": round(time.time() * 1000),
        }
        write_json_atomic(self.args.grant_path.resolve(), payload)

    def snapshot(self):
        status = compat.evaluate_json(self.client, compat.STATUS_JS)
        ready = status.get("readyState") == "complete" and not status.get("challenge")
        truth = compat.sanitize_truth(
            compat.evaluate_json(self.client, reference.PROBE_JS)
        ) if ready else None
        actions = (truth or {}).get("actions", [])
        text_revision = self.client.call(
            "Runtime.evaluate",
            {
                "expression": TEXT_REVISION_JS.replace(
                    "__SACCADE_REVISION_SALT__", json.dumps(self.revision_salt)
                ),
                "returnByValue": True,
            },
        ).get("result", {}).get("value", "") if ready else ""
        fingerprint_source = {
            "url": reference.safe_url(status.get("url", self.args.initial_url)),
            "title": status.get("title", ""),
            "ready": ready,
            "body": status.get("bodyTextLength", 0),
            "text_revision": text_revision,
            "actions": [
                (item.get("action_id"), item.get("enabled"), item.get("label"))
                for item in actions
                if isinstance(item, dict)
            ],
        }
        fingerprint = hashlib.sha256(
            json.dumps(fingerprint_source, sort_keys=True).encode("utf-8")
        ).hexdigest()
        if self.last_fingerprint is not None and fingerprint != self.last_fingerprint:
            self.page_revision += 1
        self.last_fingerprint = fingerprint
        return {
            "ready": ready,
            "status": status,
            "truth": truth,
            "actions": actions,
            "url": reference.safe_url(status.get("url", self.args.initial_url)),
            "title": status.get("title", ""),
            "page_revision": self.page_revision,
            "fingerprint": fingerprint,
        }

    def status_response(self, engine):
        snapshot = self.snapshot()
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": engine,
            "summary": "Chrome compatibility bridge is attached to the visible Human-owned browser session",
            "same_webview_control": True,
            "url": snapshot["url"],
            "title": snapshot["title"],
            "load_state": snapshot["status"].get("readyState"),
            "page_revision": snapshot["page_revision"],
            "site_challenge": bool(snapshot["status"].get("challenge")),
            "copilot": self.copilot(),
            "capabilities": [
                "ping",
                "shell_status",
                "truth",
                "actions",
                "act",
                "fill_agent_fields",
                "inspect_fields",
                "navigate",
                "reload",
                "back",
                "forward",
                "shutdown",
            ],
            "artifacts": self.artifacts(),
        }

    def truth_response(self, engine):
        snapshot = self.snapshot()
        if not snapshot["ready"]:
            raise ValueError("current compatibility page is loading or has a provider challenge")
        truth = snapshot["truth"] or {}
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": engine,
            "summary": "redacted truth/actions collected from the visible Chrome compatibility tab",
            "same_webview_control": True,
            "url": snapshot["url"],
            "title": snapshot["title"],
            "page_revision": snapshot["page_revision"],
            "actions": snapshot["actions"],
            "findings": [],
            "truth": {
                "page": {
                    "url": snapshot["url"],
                    "title": snapshot["title"],
                    "body_text_length": truth.get("bodyTextLength", 0),
                    "child_count": truth.get("bodyChildCount", 0),
                },
                "viewport": truth.get("viewport"),
                "scroll": truth.get("scroll"),
                "safety": {
                    "sensitive_values_exposed_to_agent": False,
                    "cookies_exported": False,
                    "storage_exported": False,
                },
                "action_count": len(snapshot["actions"]),
            },
            "artifacts": self.artifacts(),
        }

    def append_replay(self, event):
        self.replay_path.parent.mkdir(parents=True, exist_ok=True)
        with self.replay_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(event, sort_keys=True) + "\n")

    def write_report(self, latest):
        write_json_atomic(
            self.report_path,
            {
                "ok": True,
                "runtime": RUNTIME,
                "engine": "chrome_compatibility_control",
                "page_revision": self.page_revision,
                "copilot": self.copilot(),
                "latest": latest,
                "artifacts": self.artifacts(),
            },
        )

    @staticmethod
    def action_requires_user(action):
        sensitivity = action.get("sensitivity") or {}
        if sensitivity.get("kind") not in (None, "", "none"):
            return "sensitive target remains user-owned"
        label = str(action.get("label") or "").strip().lower()
        if any(token in label for token in SAFE_ACTION_BLOCK):
            return "side-effecting action requires the user"
        return None

    def action_response(self, params):
        action_id = str(params.get("action_id") or "")
        basis = params.get("basis_page_revision")
        if not action_id or not isinstance(basis, int):
            raise ValueError("act requires string action_id and integer basis_page_revision")
        before = self.snapshot()
        if not before["ready"]:
            raise ValueError("current compatibility page is loading or has a provider challenge")
        if basis != before["page_revision"]:
            raise ValueError(f"stale action basis: requested {basis}, current {before['page_revision']}")
        action = next(
            (item for item in before["actions"] if item.get("action_id") == action_id), None
        )
        if not action:
            raise ValueError(f"unknown action_id {action_id!r}")
        if not action.get("enabled"):
            raise ValueError(f"action {action_id!r} is not enabled")
        if reason := self.action_requires_user(action):
            raise ValueError(f"user confirmation required before action {action_id!r}: {reason}")
        rect = action.get("rect") or {}
        x = float(rect.get("left", 0)) + float(rect.get("width", 0)) / 2
        y = float(rect.get("top", 0)) + float(rect.get("height", 0)) / 2
        self.client.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y})
        self.client.call(
            "Input.dispatchMouseEvent",
            {"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1},
        )
        self.client.call(
            "Input.dispatchMouseEvent",
            {"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1},
        )
        time.sleep(0.2)
        after = self.snapshot()
        changed = before["fingerprint"] != after["fingerprint"]
        if not changed:
            self.page_revision += 1
            after["page_revision"] = self.page_revision
        event = {
            "kind": "chrome_compat_action",
            "action_id": action_id,
            "label": action.get("label"),
            "basis_page_revision": basis,
            "new_page_revision": after["page_revision"],
            "action_sent": True,
            "changed": changed,
            "url": after["url"],
            "values_logged": False,
        }
        self.append_replay(event)
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": "chrome-compatibility-browser-input-v0",
            "summary": "low-risk action dispatched through browser input in the visible Chrome compatibility tab",
            "same_webview_control": True,
            "url": after["url"],
            "title": after["title"],
            "page_revision": after["page_revision"],
            "actions": after["actions"],
            "verification": {
                "mode": "chrome_cdp_browser_input_v0",
                "action_id": action_id,
                "action_sent": True,
                "changed": changed,
                "no_effect": not changed,
                "basis_page_revision": basis,
                "new_page_revision": after["page_revision"],
            },
            "artifacts": self.artifacts(),
        }

    def evaluate_json_template(self, template, value):
        expression = template.replace("__SACCADE_FIELDS__", json.dumps(value))
        return self.client.call(
            "Runtime.evaluate",
            {"expression": expression, "returnByValue": True, "awaitPromise": True},
        ).get("result", {}).get("value", {})

    def fill_response(self, params):
        fields = params.get("fields")
        if not isinstance(fields, dict) or not fields:
            raise ValueError("fill_agent_fields requires a non-empty object params.fields")
        snapshot = self.snapshot()
        if not snapshot["ready"]:
            raise ValueError("current compatibility page is loading or has a provider challenge")
        result = self.evaluate_json_template(FILL_FIELDS_JS, fields)
        if result.get("filled"):
            self.page_revision += 1
        snapshot = self.snapshot()
        event = {
            "kind": "chrome_compat_fill",
            "requested": len(fields),
            "filled_fields": [item.get("field") for item in result.get("filled", [])],
            "rejected": result.get("rejected", []),
            "page_revision": snapshot["page_revision"],
            "values_logged": False,
        }
        self.append_replay(event)
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": "chrome-compatibility-fill-v0",
            "summary": "explicit agent-owned non-sensitive fields filled through the visible Chrome compatibility tab",
            "same_webview_control": True,
            "url": snapshot["url"],
            "title": snapshot["title"],
            "page_revision": snapshot["page_revision"],
            "requested": len(fields),
            "filled": result.get("filled", []),
            "rejected": result.get("rejected", []),
            "sensitive_fields_seen": result.get("sensitiveFieldsSeen", []),
            "artifacts": self.artifacts(),
        }

    def inspect_response(self, params):
        fields = params.get("fields")
        if not isinstance(fields, list) or not fields or not all(isinstance(field, str) for field in fields):
            raise ValueError("inspect_fields requires a non-empty string array params.fields")
        snapshot = self.snapshot()
        if not snapshot["ready"]:
            raise ValueError("current compatibility page is loading or has a provider challenge")
        result = self.evaluate_json_template(INSPECT_FIELDS_JS, fields)
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": "chrome-compatibility-inspect-fields-v0",
            "summary": "explicit field inspection completed with values withheld",
            "same_webview_control": True,
            "url": snapshot["url"],
            "title": snapshot["title"],
            "page_revision": snapshot["page_revision"],
            "fields": result.get("fields", []),
            "sensitive_fields_seen": result.get("sensitiveFieldsSeen", []),
            "artifacts": self.artifacts(),
        }

    def navigate_response(self, method, params):
        if method == "navigate":
            url = str(params.get("url") or "").strip()
            parsed = urllib.parse.urlparse(url)
            if parsed.scheme not in ("http", "https", "file"):
                raise ValueError("navigate requires an http(s) or file URL")
            self.client.call("Page.navigate", {"url": url})
        elif method == "reload":
            self.client.call("Page.reload", {"ignoreCache": False})
        elif method in ("back", "forward"):
            direction = -1 if method == "back" else 1
            self.client.call(
                "Runtime.evaluate",
                {"expression": f"history.go({direction})", "returnByValue": True},
            )
        else:
            raise ValueError(f"unsupported navigation method {method!r}")
        deadline = time.monotonic() + 20
        snapshot = self.snapshot()
        while time.monotonic() < deadline and not snapshot["ready"]:
            time.sleep(0.2)
            snapshot = self.snapshot()
        self.page_revision += 1
        snapshot["page_revision"] = self.page_revision
        self.append_replay(
            {
                "kind": "chrome_compat_navigation",
                "method": method,
                "url": snapshot["url"],
                "page_revision": snapshot["page_revision"],
                "values_logged": False,
            }
        )
        return {
            "status": "ok",
            "runtime": RUNTIME,
            "engine": f"chrome-compatibility-{method}-v0",
            "summary": "visible Chrome compatibility tab navigation completed",
            "same_webview_control": True,
            "url": snapshot["url"],
            "title": snapshot["title"],
            "page_revision": snapshot["page_revision"],
            "changed": True,
            "artifacts": self.artifacts(),
        }

    def result(self, method, params):
        if method == "ping":
            if params.get("protocol") != PROTOCOL:
                raise ValueError("unsupported control protocol")
            return self.status_response("saccade-chrome-compat-ping-v0")
        if method == "shell_status":
            return self.status_response("saccade-chrome-compat-shell-status-v0")
        if method in ("truth", "actions"):
            return self.truth_response(f"saccade-chrome-compat-{method}-v0")
        if method == "act":
            return self.action_response(params)
        if method == "fill_agent_fields":
            return self.fill_response(params)
        if method == "inspect_fields":
            return self.inspect_response(params)
        if method in ("navigate", "reload", "back", "forward"):
            return self.navigate_response(method, params)
        if method == "shutdown":
            self.shutdown.set()
            return {"status": "ok", "runtime": RUNTIME, "summary": "compatibility bridge shutdown requested"}
        raise ValueError(f"unsupported compatibility control method {method!r}")

    def handle_stream(self, stream):
        with stream:
            line = stream.makefile("r", encoding="utf-8").readline()
            try:
                request = json.loads(line)
                result = self.result(request.get("method", ""), request.get("params") or {})
                self.write_report(result)
                response = {"id": request.get("id"), "ok": True, "result": result}
            except Exception as error:
                response = {"id": None, "ok": False, "error": str(error)}
            stream.sendall((json.dumps(response, sort_keys=True) + "\n").encode("utf-8"))

    def serve(self):
        self.write_grant()
        print(
            f"CHROME COMPAT CONTROL READY grant={self.args.grant_path.resolve()} port={self.port}",
            flush=True,
        )
        while not self.shutdown.is_set():
            try:
                stream, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.handle_stream(stream)
        self.listener.close()


def main():
    args = parse_args()
    _, client = reference.wait_for_cdp_client(args.cdp_port, args.timeout_sec)
    client.call("Page.enable")
    client.call("Runtime.enable")
    control = ChromeCompatControl(args, client)
    try:
        control.serve()
    finally:
        client.close()


if __name__ == "__main__":
    main()
