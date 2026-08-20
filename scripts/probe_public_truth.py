#!/usr/bin/env python3
"""Prove default Truth views and deltas on public pages with a test-only stimulus."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from dev_probe import action_arguments, is_stale_action_error, named, stable_observation, wait_for_mcp
from external_dogfood import INPUT_PROFILES, compact_object, load_cases


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CASES = ROOT / "catalog" / "external_cases.json"


def utc_now() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def action_payload(case: dict[str, Any], observation: dict[str, Any]) -> dict[str, Any]:
    action = case["action"]
    if "input_profile" in action:
        return {"kind": "text", "text": INPUT_PROFILES[action["input_profile"]]}
    if "option" in action:
        option = named(observation, "option", action["option"])
        return {"kind": "select", "option_object_id": option["object_id"]}
    return {"kind": "none"}


def compact_initial(view: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": "saccade.public-truth-view/1",
        "document_id": view["document_id"],
        "revision": view["revision"],
        "coverage": view.get("coverage", {}),
        "limitations": view.get("limitations", []),
        "objects": [compact_object(item) for item in view.get("objects", [])],
    }


def compact_changes(changes: list[dict[str, Any]]) -> list[dict[str, Any]]:
    compacted = []
    for change in changes:
        if change.get("kind") == "disappeared":
            compacted.append({"kind": "disappeared", "object_id": change.get("object_id")})
        else:
            compacted.append({"kind": change.get("kind"), "object": compact_object(change.get("object", {}))})
    return compacted


def expected_object(case: dict[str, Any], view: dict[str, Any]) -> dict[str, Any] | None:
    expected = case["postcondition"]
    role = expected.get("role", case["target"]["role"])
    name = expected.get("name", case["target"]["name"])
    return next(
        (item for item in view.get("objects", []) if item.get("role") == role and item.get("name") == name),
        None,
    )


def run_case(core: Any, stimulus: Any, case: dict[str, Any]) -> dict[str, Any]:
    started = time.perf_counter()
    opened = core.tool("tabs.open", {"url": case["url"], "active": True})
    tab_id = str(opened["tab_id"])
    target_deadline = time.monotonic() + 20
    target = None
    while target is None and time.monotonic() < target_deadline:
        initial = stable_observation(core, tab_id)
        target = next(
            (
                item
                for item in initial.get("objects", [])
                if item.get("role") == case["target"]["role"]
                and item.get("name") == case["target"]["name"]
            ),
            None,
        )
        if target is None:
            time.sleep(0.5)
    if target is None:
        raise RuntimeError("default Truth omitted the declared target")
    if target.get("action_token") is not None:
        raise RuntimeError("default Truth leaked action authority")

    recognition = {
        "recognized": True,
        "role": target.get("role"),
        "name": target.get("name"),
        "has_document_bounds": isinstance(target.get("document_bounds"), dict),
        "has_viewport_bounds": isinstance(target.get("viewport_bounds"), dict),
    }

    working = stable_observation(stimulus, tab_id)
    receipt: dict[str, Any] | None = None
    for _attempt in range(8):
        working = stable_observation(stimulus, tab_id)
        stimulus_target = named(working, case["target"]["role"], case["target"]["name"])
        if case["action"].get("open"):
            try:
                opened_receipt = stimulus.tool(
                    "web.act_soft",
                    action_arguments(working, stimulus_target, "click", {"kind": "none"}),
                    timeout=45.0,
                )
            except RuntimeError as error:
                if is_stale_action_error(error):
                    continue
                raise
            if not str(opened_receipt.get("dispatch_status", "")).startswith("accepted_by_"):
                raise RuntimeError("test-only stimulus could not open the declared control")
            working = stable_observation(stimulus, tab_id)
            stimulus_target = named(working, case["target"]["role"], case["target"]["name"])
        arguments = action_arguments(
            working,
            stimulus_target,
            case["action"]["operation"],
            action_payload(case, working),
        )
        stimulus_tool = (
            "web.act_soft"
            if case["action"]["operation"] in {"click", "select"}
            else "web.act"
        )
        try:
            receipt = stimulus.tool(stimulus_tool, arguments, timeout=45.0)
        except RuntimeError as error:
            if is_stale_action_error(error):
                continue
            raise
        if receipt.get("dispatch_status") != "stale_before_dispatch":
            break
        working = receipt["post_action_observation"]
    dispatch_status = "" if receipt is None else str(receipt.get("dispatch_status", ""))
    if not dispatch_status.startswith("accepted_by_"):
        raise RuntimeError(f"test-only stimulus was not accepted: {dispatch_status or 'missing receipt'}")

    expected = case["postcondition"]
    initial_expected = expected_object(case, initial)
    initial_state = None if initial_expected is None else initial_expected.get("state", {}).get(expected["state"])
    revision = int(initial["revision"])
    updated = initial
    observed_changes: list[dict[str, Any]] = []
    current: dict[str, Any] | None = None
    delivery = "unknown"
    for _attempt in range(10):
        updated = core.tool(
            "web.observe",
            {"tab_id": tab_id, "after_revision": revision, "timeout_ms": 3_000},
        )
        revision = int(updated["revision"])
        observed_changes.extend(updated.get("changes", []))
        current = expected_object(case, updated)
        changed_ids = {
            change.get("object_id") or change.get("object", {}).get("object_id")
            for change in observed_changes
        }
        if (
            current is not None
            and current.get("state", {}).get(expected["state"]) == expected["equals"]
            and (
                current.get("object_id") in changed_ids
                or initial_state != expected["equals"]
            )
        ):
            delivery = "delta" if current.get("object_id") in changed_ids else "full_reset"
            break
    else:
        raise RuntimeError("default Truth postcondition was absent from the pushed delta")
    assert current is not None

    return {
        "id": case["id"],
        "source": case["source"],
        "implementation": case["implementation"],
        "url": case["url"],
        "outcome": "pass",
        "elapsed_ms": round((time.perf_counter() - started) * 1000, 3),
        "stimulus": "reference_actuator_test_only",
        "recognition": recognition,
        "evidence": {
            "initial": compact_initial(initial),
            "pushed_revision": updated["revision"],
            "changes": compact_changes(observed_changes),
            "delivery": delivery,
            "postcondition": {
                "role": current.get("role"),
                "name": current.get("name"),
                "state": expected["state"],
                "equals": expected["equals"],
            },
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--browser-version", required=True)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--cases", default=DEFAULT_CASES, type=Path)
    parser.add_argument("--extra-cases", action="append", default=[], type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    cases = load_cases(args.cases.resolve())
    for extra in args.extra_cases:
        cases.extend(load_cases(extra.resolve()))
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        raise RuntimeError("public Truth case ids must be unique across manifests")
    started_at = utc_now()
    core = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    stimulus = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve(), reference=True)
    results: list[dict[str, Any]] = []
    try:
        for case in cases:
            try:
                results.append(run_case(core, stimulus, case))
            except Exception as error:  # noqa: BLE001
                reason = str(error)[:500]
                blocked_fragments = (
                    "permission_required",
                    "user-local input policy requires native input",
                    "prepared action failed",
                    "stale action basis",
                    "test-only stimulus",
                    "no observation after revision",
                )
                results.append({
                    "id": case["id"],
                    "source": case["source"],
                    "implementation": case["implementation"],
                    "url": case["url"],
                    "outcome": "blocked" if any(part in reason for part in blocked_fragments) else "fail",
                    "reason": reason,
                    "recognition": {
                        "recognized": "default Truth omitted the declared target" not in reason,
                    },
                })
            finally:
                try:
                    tabs = core.tool("tabs.list", {}).get("tabs", [])
                    for tab in tabs:
                        if tab.get("ownership") == "agent":
                            core.tool("tabs.close", {"tab_id": str(tab["tab_id"])})
                except Exception:  # noqa: BLE001 -- cleanup must not mask case evidence
                    pass
    finally:
        stimulus.close()
        core.close()

    passed = sum(result["outcome"] == "pass" for result in results)
    blocked = sum(result["outcome"] == "blocked" for result in results)
    failed = sum(result["outcome"] == "fail" for result in results)
    recognized = sum(result.get("recognition", {}).get("recognized") is True for result in results)
    report = {
        "schema": "saccade.public-truth-evidence/1",
        "browser": args.browser,
        "browser_version": args.browser_version.strip(),
        "candidate_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "candidate_dirty": bool(subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=ROOT, text=True
        ).strip()),
        "started_at": started_at,
        "finished_at": utc_now(),
        "summary": {
            "total": len(results),
            "passed": passed,
            "failed": failed,
            "blocked": blocked,
            "recognized": recognized,
            "recognition_rate": f"{recognized}/{len(results)}",
            "closed_loop_rate": f"{passed}/{len(results)}",
            "sources": sorted({result["source"] for result in results}),
            "implementations": sorted({result["implementation"] for result in results}),
        },
        "execution_boundary": {
            "observation": "default_truth_mcp",
            "stimulus": "reference_actuator_test_only",
            "receipt_in_evidence": False,
            "authority_in_evidence": False,
        },
        "cases": results,
    }
    serialized = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    for forbidden in (*INPUT_PROFILES.values(), "action_token"):
        if forbidden.casefold() in serialized.casefold():
            raise RuntimeError(f"public Truth evidence contains forbidden content: {forbidden}")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(serialized, encoding="utf-8")
    print(json.dumps({"passed": passed, "total": len(results), "evidence": str(args.output)}))
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
