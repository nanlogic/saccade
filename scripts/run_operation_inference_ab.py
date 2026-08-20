#!/usr/bin/env python3
"""Compare explicit and Runtime-inferred saccade.act operations on fresh tasks."""

from __future__ import annotations

import argparse
import json
import secrets
import statistics
from pathlib import Path
from typing import Any

from benchmark_agent_fair import load_playwright_lock, parse_events, run_lane, tool_name
from generate_unknown_benchmark import KINDS, build
from run_same_model_matrix import assert_attached_browser, prepare_output


ORDER_BY_KIND = {
    "native": ("explicit", "inferred"),
    "reveal": ("inferred", "explicit"),
    "replace": ("explicit", "inferred"),
}


def action_entries(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for event in events:
        item = event.get("item")
        if event.get("type") != "item.completed" or not isinstance(item, dict):
            continue
        if "saccade.act" not in tool_name(item).casefold():
            continue
        arguments = item.get("arguments")
        if not isinstance(arguments, dict):
            continue
        actions = arguments.get("actions")
        if isinstance(actions, list):
            entries.extend(action for action in actions if isinstance(action, dict))
        else:
            entries.append(arguments)
    return entries


def operation_compliance(events: list[dict[str, Any]], mode: str) -> dict[str, Any]:
    if mode not in {"explicit", "inferred"}:
        raise ValueError("mode must be explicit or inferred")
    entries = action_entries(events)
    operation_fields = sum("operation" in entry for entry in entries)
    compliant = bool(entries) and (
        operation_fields == len(entries) if mode == "explicit" else operation_fields == 0
    )
    return {
        "action_entries": len(entries),
        "operation_fields": operation_fields,
        "failed_tool_calls": sum(
            event.get("type") == "item.completed"
            and isinstance(event.get("item"), dict)
            and event["item"].get("status") == "failed"
            for event in events
        ),
        "compliant": compliant,
    }


def infrastructure_failure(summary: dict[str, Any]) -> str | None:
    text = json.dumps(
        {"final": summary.get("final"), "stderr": summary.get("stderr_tail")},
        ensure_ascii=False,
    ).casefold()
    for needle in ("529", "overloaded", "rate limit", "service unavailable"):
        if needle in text:
            return needle
    return None


def evidence_errors(summary: dict[str, Any], compliance: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if not summary.get("passed"):
        errors.append("task_not_browser_proven")
    if not compliance.get("compliant"):
        errors.append("operation_mode_not_obeyed")
    if compliance.get("failed_tool_calls"):
        errors.append("tool_call_failed")
    usage = summary.get("usage") or {}
    metrics = summary.get("browser_metrics") or {}
    if not isinstance(usage.get("input_tokens"), int) or usage["input_tokens"] <= 0:
        errors.append("input_tokens_missing")
    if not isinstance(usage.get("output_tokens"), int) or usage["output_tokens"] <= 0:
        errors.append("output_tokens_missing")
    if not isinstance(summary.get("elapsed_ms"), (int, float)) or summary["elapsed_ms"] <= 0:
        errors.append("elapsed_ms_missing")
    if not isinstance(metrics.get("initial_transfer_bytes"), int) or metrics["initial_transfer_bytes"] <= 0:
        errors.append("initial_transfer_bytes_missing")
    failure = infrastructure_failure(summary)
    if failure:
        errors.append(f"infrastructure_failure:{failure}")
    return errors


def metric_row(summary: dict[str, Any]) -> dict[str, int | float | None]:
    usage = summary.get("usage") or {}
    metrics = summary.get("browser_metrics") or {}
    return {
        "elapsed_ms": summary.get("elapsed_ms"),
        "input_tokens": usage.get("input_tokens"),
        "output_tokens": usage.get("output_tokens"),
        "tool_calls": summary.get("tool_calls"),
        "initial_transfer_bytes": metrics.get("initial_transfer_bytes"),
        "transcript_bytes": metrics.get("transcript_bytes"),
        "post_initial_reobservation_calls": metrics.get("post_initial_reobservation_calls"),
        "stale_events": metrics.get("stale_events"),
    }


def aggregate(pairs: list[dict[str, Any]]) -> dict[str, Any]:
    valid = [pair for pair in pairs if pair["valid"]]
    result: dict[str, Any] = {"valid_pairs": len(valid), "total_pairs": len(pairs), "modes": {}}
    for mode in ("explicit", "inferred"):
        rows = [pair["runs"][mode]["metrics"] for pair in valid]
        result["modes"][mode] = {
            key: round(statistics.mean(row[key] for row in rows), 3)
            for key in metric_row({}).keys()
            if rows and all(isinstance(row.get(key), (int, float)) for row in rows)
        }
    deltas: dict[str, Any] = {}
    explicit = result["modes"]["explicit"]
    inferred = result["modes"]["inferred"]
    for key in explicit.keys() & inferred.keys():
        baseline = explicit[key]
        deltas[key] = {
            "absolute_inferred_minus_explicit": round(inferred[key] - baseline, 3),
            "percent_inferred_minus_explicit": (
                round((inferred[key] - baseline) / baseline * 100, 2) if baseline else None
            ),
        }
    result["deltas"] = deltas
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--fixture-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--base-url", default="http://127.0.0.1:8765/fixtures/benchmarks")
    parser.add_argument("--browser", choices=("chrome", "edge"), default="chrome")
    parser.add_argument("--model")
    parser.add_argument("--effort", choices=("low", "medium", "high", "xhigh"), default="low")
    args = parser.parse_args()

    runtime = args.runtime.resolve()
    runtime_dir = args.runtime_dir.resolve()
    assert_attached_browser(runtime, runtime_dir, args.browser)
    output = args.output.resolve()
    archived = prepare_output(output)
    if archived:
        print(f"Archived previous attempt at {archived}", flush=True)
    generated = output / "generated"
    generated.mkdir(parents=True, exist_ok=True)
    live = args.fixture_root.resolve() / "fixtures" / "benchmarks"
    live.mkdir(parents=True, exist_ok=True)
    lock = load_playwright_lock()
    playwright_package = f"{lock['package']}@{lock['version']}"

    pairs: list[dict[str, Any]] = []
    for kind in KINDS:
        seed = secrets.token_hex(12)
        slug = f"operation-ab-{kind}-{seed[:8]}"
        url = f"{args.base_url.rstrip('/')}/{slug}.html"
        page, task = build(kind, seed, url)
        (live / f"{slug}.html").write_text(page, encoding="utf-8")
        (generated / f"{slug}.html").write_text(page, encoding="utf-8")
        (generated / f"{slug}.json").write_text(
            json.dumps(task, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        pair: dict[str, Any] = {
            "kind": kind,
            "task": {"name": task["name"], "url": task["url"]},
            "order": list(ORDER_BY_KIND[kind]),
            "runs": {},
        }
        for mode in ORDER_BY_KIND[kind]:
            print(f"=== {kind}: {mode} ===", flush=True)
            run_output = output / slug / mode
            run_output.mkdir(parents=True, exist_ok=True)
            summary = run_lane(
                "saccade", task, args.model, args.effort, runtime, runtime_dir,
                playwright_package, run_output, operation_mode=mode,
            )
            events = parse_events((run_output / "saccade.jsonl").read_text(encoding="utf-8"))
            compliance = operation_compliance(events, mode)
            errors = evidence_errors(summary, compliance)
            pair["runs"][mode] = {
                "passed": summary["passed"],
                "metrics": metric_row(summary),
                "operation_compliance": compliance,
                "evidence_errors": errors,
                "failure_reason": summary.get("failure_reason"),
            }
            print(json.dumps({"passed": summary["passed"], "compliance": compliance, "errors": errors}), flush=True)
        pair["valid"] = all(not run["evidence_errors"] for run in pair["runs"].values())
        pairs.append(pair)

    report = {
        "schema": "saccade-operation-inference-ab/1",
        "agent": {"driver": "codex exec", "model": args.model or "codex-default-recommended", "effort": args.effort},
        "candidate": "live candidate asserted by saccade.system.capabilities preflight",
        "publication_claim_authorized": False,
        "limitations": [
            "Directional developer evidence: three fresh tasks, not a publication matrix.",
            "Order is alternated but not fully counterbalanced for every task.",
            "End-to-end latency includes model and service variance.",
        ],
        "pairs": pairs,
        "aggregate": aggregate(pairs),
    }
    report["verdict"] = "PASS" if pairs and all(pair["valid"] for pair in pairs) else "INVALID"
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"verdict": report["verdict"], "aggregate": report["aggregate"]}, ensure_ascii=False))
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
