#!/usr/bin/env python3
"""Prove redacted renderer truth and a millisecond Chrome input loop.

This is a bounded CDP proof, not a CEF integration. The injected observer emits
only target geometry and timing. Sensitive field values stay inside Chrome.
"""

from __future__ import annotations

import argparse
import json
import math
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from typing import Any

import chrome_reference_cdp as reference


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_FIXTURE = ROOT / "test_pages" / "chrome_truth_reflex" / "index.html"
SENSITIVE_SENTINELS = ("123-45-6789", "correct-horse-battery")

OBSERVER_JS = r"""
(() => {
  if (window.__saccadeChromeTruthReflex) return window.__saccadeChromeTruthReflex;
  let sequence = 0;
  const seen = new WeakSet();
  const epochNow = () => performance.timeOrigin + performance.now();
  const emit = (kind, payload) => window.saccadeTruthReflex(JSON.stringify({
    kind,
    epoch_ms: epochNow(),
    ...payload
  }));
  const scan = () => {
    for (const element of document.querySelectorAll('.target:not(.hit)')) {
      if (seen.has(element)) continue;
      const rect = element.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) continue;
      seen.add(element);
      sequence += 1;
      element.dataset.saccadePocTarget = String(sequence);
      emit('target_seen', {
        target_id: sequence,
        rect: {
          left: rect.left,
          top: rect.top,
          width: rect.width,
          height: rect.height
        }
      });
    }
  };
  document.addEventListener('mousedown', event => {
    const target = event.target && event.target.closest
      ? event.target.closest('.target')
      : null;
    if (!target) return;
    emit('target_input', {
      target_id: Number(target.dataset.saccadePocTarget || 0),
      client_x: event.clientX,
      client_y: event.clientY
    });
  }, true);
  const observer = new MutationObserver(scan);
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ['class', 'style']
  });
  window.__saccadeChromeTruthReflex = {
    kind: 'renderer_observer_v1',
    values_read: false,
    start: () => {
      scan();
      window.__saccadeStart();
      return true;
    }
  };
  return window.__saccadeChromeTruthReflex;
})()
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure Chrome renderer truth -> browser input -> page receipt latency."
    )
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--targets", type=int, default=30)
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument(
        "--headed",
        action="store_true",
        help="Show the Chrome window. The default headless run uses the same engine and CDP path.",
    )
    parser.add_argument("--dispatch-p95-ms", type=float, default=5.0)
    parser.add_argument("--receipt-p95-ms", type=float, default=20.0)
    return parser.parse_args()


def percentile(values: list[float], percentile_value: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, math.ceil(percentile_value * len(ordered)) - 1)
    return round(ordered[index], 3)


def metric(values: list[float]) -> dict[str, float | int | None]:
    return {
        "count": len(values),
        "p50_ms": round(statistics.median(values), 3) if values else None,
        "p95_ms": percentile(values, 0.95),
        "max_ms": round(max(values), 3) if values else None,
    }


def launch_chrome(
    chrome: str,
    port: int,
    profile_dir: pathlib.Path,
    width: int,
    height: int,
    headed: bool,
) -> subprocess.Popen[str]:
    command = [
        chrome,
        f"--user-data-dir={profile_dir}",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={port}",
        f"--window-size={width},{height}",
        "--force-device-scale-factor=1",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "about:blank",
    ]
    if not headed:
        command.insert(1, "--headless=new")
    return subprocess.Popen(
        command,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
    )


def binding_payload(event: dict[str, Any] | None) -> dict[str, Any] | None:
    if not event or event.get("method") != "Runtime.bindingCalled":
        return None
    params = event.get("params") or {}
    if params.get("name") != "saccadeTruthReflex":
        return None
    try:
        payload = json.loads(params.get("payload") or "{}")
    except json.JSONDecodeError:
        return None
    return payload if isinstance(payload, dict) else None


def pop_binding(client: reference.CdpClient, kind: str) -> dict[str, Any] | None:
    for index, event in enumerate(client.events):
        payload = binding_payload(event)
        if payload and payload.get("kind") == kind:
            client.events.pop(index)
            return payload
    return None


def wait_binding(
    client: reference.CdpClient,
    kind: str,
    timeout_sec: float,
) -> tuple[dict[str, Any], int, float]:
    deadline = time.monotonic() + timeout_sec
    while time.monotonic() < deadline:
        payload = pop_binding(client, kind)
        if payload:
            return payload, time.perf_counter_ns(), time.time_ns() / 1_000_000
        client.wait_for_event(
            "Runtime.bindingCalled", min(0.5, max(0.01, deadline - time.monotonic()))
        )
    raise TimeoutError(f"timed out waiting for renderer fact {kind!r}")


def evaluate_value(client: reference.CdpClient, expression: str) -> Any:
    result = client.call(
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": True},
    )
    return result.get("result", {}).get("value")


def dispatch_target(
    client: reference.CdpClient,
    target: dict[str, Any],
    received_ns: int,
) -> dict[str, float]:
    rect = target["rect"]
    x = float(rect["left"]) + float(rect["width"]) / 2
    y = float(rect["top"]) + float(rect["height"]) / 2
    press_started_ns = time.perf_counter_ns()
    client.next_id += 1
    client.ws.send_json(
        {
            "id": client.next_id,
            "method": "Input.dispatchMouseEvent",
            "params": {
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
            },
        }
    )
    press_sent_ns = time.perf_counter_ns()
    client.next_id += 1
    client.ws.send_json(
        {
            "id": client.next_id,
            "method": "Input.dispatchMouseEvent",
            "params": {
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
            },
        }
    )
    return {
        "x": x,
        "y": y,
        "host_truth_to_dispatch_start_ms": (press_started_ns - received_ns) / 1_000_000,
        "host_truth_to_dispatch_send_ms": (press_sent_ns - received_ns) / 1_000_000,
        "press_socket_write_ms": (press_sent_ns - press_started_ns) / 1_000_000,
    }


def wait_for_loaded(client: reference.CdpClient, timeout_sec: float) -> None:
    event = client.wait_for_event("Page.loadEventFired", timeout_sec)
    if event:
        return
    ready = evaluate_value(client, "document.readyState")
    if ready not in ("interactive", "complete"):
        raise TimeoutError(f"fixture did not load; readyState={ready!r}")


def main() -> int:
    args = parse_args()
    if args.targets <= 0:
        raise SystemExit("--targets must be positive")
    fixture = args.fixture.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    report_path = output_dir / "report.json"
    profile_dir = pathlib.Path(tempfile.mkdtemp(prefix="saccade-chrome-reflex-"))
    chrome = reference.find_chrome()
    port = reference.free_port()
    process = None
    client = None
    started = time.monotonic()
    samples: list[dict[str, Any]] = []
    report: dict[str, Any]
    try:
        process = launch_chrome(
            chrome, port, profile_dir, args.width, args.height, args.headed
        )
        _, client = reference.wait_for_cdp_client(port, args.timeout_sec)
        client.call("Page.enable")
        client.call("Runtime.enable")
        client.call("Runtime.addBinding", {"name": "saccadeTruthReflex"})
        client.call("Page.navigate", {"url": f"{fixture.as_uri()}?count={args.targets}"})
        wait_for_loaded(client, args.timeout_sec)
        observer = evaluate_value(client, OBSERVER_JS)
        if not isinstance(observer, dict) or observer.get("kind") != "renderer_observer_v1":
            raise RuntimeError(f"renderer truth observer did not install: {observer!r}")
        evaluate_value(client, "window.__saccadeChromeTruthReflex.start()")

        for _ in range(args.targets):
            target, received_ns, received_epoch_ms = wait_binding(
                client, "target_seen", args.timeout_sec
            )
            dispatch = dispatch_target(client, target, received_ns)
            receipt, _, _ = wait_binding(client, "target_input", args.timeout_sec)
            if receipt.get("target_id") != target.get("target_id"):
                raise RuntimeError(
                    f"receipt target mismatch: seen={target.get('target_id')} receipt={receipt.get('target_id')}"
                )
            samples.append(
                {
                    "target_id": target["target_id"],
                    "rect": target["rect"],
                    **dispatch,
                    "renderer_seen_to_host_receive_ms": received_epoch_ms
                    - float(target["epoch_ms"]),
                    "host_receive_to_renderer_input_receipt_ms": float(
                        receipt["epoch_ms"]
                    )
                    - received_epoch_ms,
                    "renderer_seen_to_input_receipt_ms": float(receipt["epoch_ms"])
                    - float(target["epoch_ms"]),
                    "values_logged": False,
                }
            )

        client.drain(0.12)
        final_truth = evaluate_value(client, "document.querySelector('#truth').textContent")
        redacted_truth = json.loads(evaluate_value(client, reference.PROBE_JS))
        serialized_truth = json.dumps(redacted_truth, sort_keys=True)
        leaked = [value for value in SENSITIVE_SENTINELS if value in serialized_truth]
        sensitive_actions = [
            action
            for action in redacted_truth.get("actions", [])
            if (action.get("sensitivity") or {}).get("kind") not in (None, "", "none")
        ]
        dispatch_start = [sample["host_truth_to_dispatch_start_ms"] for sample in samples]
        dispatch_send = [sample["host_truth_to_dispatch_send_ms"] for sample in samples]
        truth_transport = [
            sample["renderer_seen_to_host_receive_ms"] for sample in samples
        ]
        input_transport = [
            sample["host_receive_to_renderer_input_receipt_ms"] for sample in samples
        ]
        receipt_latency = [sample["renderer_seen_to_input_receipt_ms"] for sample in samples]
        press_socket_write = [sample["press_socket_write_ms"] for sample in samples]
        hits_ok = f"hits={args.targets}" in str(final_truth) and "misses=0" in str(final_truth)
        dispatch_ok = (percentile(dispatch_start, 0.95) or math.inf) <= args.dispatch_p95_ms
        receipt_ok = (percentile(receipt_latency, 0.95) or math.inf) <= args.receipt_p95_ms
        redaction_ok = not leaked and len(sensitive_actions) >= 2
        ok = hits_ok and dispatch_ok and receipt_ok and redaction_ok
        report = {
            "ok": ok,
            "verdict": "pass" if ok else "fail",
            "engine": "chrome-cdp-truth-reflex-poc-v1",
            "scope": "chrome_engine_poc_not_cef_embedding",
            "headed": args.headed,
            "fixture": str(fixture),
            "targets_requested": args.targets,
            "targets_receipted": len(samples),
            "final_public_state": final_truth,
            "truth": {
                "route": "renderer_mutation_observer_to_runtime_binding",
                "structured_target_facts": len(samples),
                "sensitive_fields_seen": len(sensitive_actions),
                "sensitive_values_exposed": bool(leaked),
                "cookies_exported": False,
                "storage_exported": False,
                "screenshots_taken": False,
                "observer_values_read": False,
            },
            "latency": {
                "host_truth_to_dispatch_start": metric(dispatch_start),
                "host_truth_to_dispatch_send": metric(dispatch_send),
                "renderer_seen_to_host_receive": metric(truth_transport),
                "host_receive_to_renderer_input_receipt": metric(input_transport),
                "renderer_seen_to_input_receipt": metric(receipt_latency),
                "cdp_press_socket_write": metric(press_socket_write),
            },
            "gates": {
                "zero_miss_completion": hits_ok,
                "redacted_truth": redaction_ok,
                "dispatch_p95_ms_max": args.dispatch_p95_ms,
                "dispatch_p95_pass": dispatch_ok,
                "receipt_p95_ms_max": args.receipt_p95_ms,
                "receipt_p95_pass": receipt_ok,
            },
            "samples": samples,
            "elapsed_ms": round((time.monotonic() - started) * 1000, 3),
            "limitations": [
                "This proves a DOM fact path in Chrome, not canvas/display-list truth.",
                "CDP is the POC transport; CEF renderer-process integration remains separate work.",
            ],
        }
    except Exception as error:  # noqa: BLE001 - always preserve a diagnostic report.
        report = {
            "ok": False,
            "verdict": "error",
            "engine": "chrome-cdp-truth-reflex-poc-v1",
            "error": repr(error),
            "targets_receipted": len(samples),
            "samples": samples,
            "elapsed_ms": round((time.monotonic() - started) * 1000, 3),
        }
    finally:
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
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
                process.wait(timeout=3)
        shutil.rmtree(profile_dir, ignore_errors=True)

    print(
        "CHROME TRUTH REFLEX "
        f"verdict={report['verdict']} targets={report.get('targets_receipted', 0)} "
        f"report={report_path}"
    )
    if report.get("latency"):
        latency = report["latency"]
        print(
            "LATENCY "
            f"truth_to_dispatch_p95_ms={latency['host_truth_to_dispatch_start']['p95_ms']} "
            f"seen_to_receipt_p95_ms={latency['renderer_seen_to_input_receipt']['p95_ms']}"
        )
    return 0 if report.get("ok") else 1


if __name__ == "__main__":
    sys.exit(main())
