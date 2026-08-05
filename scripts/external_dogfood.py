#!/usr/bin/env python3
"""Run declarative public-page proofs through Saccade's production route."""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from dev_probe import action_arguments, named, open_fixture, stable_observation, wait_for_mcp


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CASES = ROOT / "catalog" / "external_cases.json"
INPUT_PROFILES = {
    "unicode_single_line": "SACCADE-PUBLIC-UNICODE-Ω",
    "unicode_multiline": "SACCADE PUBLIC LINE ONE\nLINE TWO Ω",
}
OUTCOMES = {
    "verified", "unsupported", "not_observed", "prepare_rejected", "dispatch_failed", "unverified",
}


def utc_now() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def load_cases(path: Path) -> list[dict[str, Any]]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != "saccade.external-cases/1" or not isinstance(value.get("cases"), list):
        raise ValueError("unsupported external case manifest")
    seen: set[str] = set()
    for case in value["cases"]:
        required = {"id", "control", "source", "implementation", "url", "goal", "target", "action", "postcondition"}
        if not required.issubset(case):
            raise ValueError(f"external case is missing {sorted(required - set(case))}")
        if case["id"] in seen:
            raise ValueError(f"duplicate external case id {case['id']}")
        seen.add(case["id"])
        if not str(case["url"]).startswith(("http://", "https://")):
            raise ValueError(f"external case {case['id']} is not HTTP(S)")
        serialized = json.dumps(case).casefold()
        for forbidden in ("selector", "xpath", "locator", "coordinate", "javascript"):
            if forbidden in serialized:
                raise ValueError(f"external case {case['id']} contains forbidden execution knowledge")
    return value["cases"]


def compact_object(item: dict[str, Any]) -> dict[str, Any]:
    return {
        key: item[key]
        for key in ("object_id", "role", "name", "description", "text", "state", "affordances")
        if key in item
    }


def compact_view(view: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": "saccade.external-view-evidence/1",
        "document_id": view["document_id"],
        "revision": view["revision"],
        "coverage": view.get("coverage", {}),
        "limitations": view.get("limitations", []),
        "objects": [compact_object(item) for item in view.get("objects", [])],
    }


def classify_error(error: Exception, observed: bool) -> str:
    message = str(error).casefold()
    if not observed or "observation has no" in message:
        return "not_observed"
    if "unsupported" in message or "not advertised" in message:
        return "unsupported"
    if "stale" in message or "prepare" in message or "not current" in message:
        return "prepare_rejected"
    return "dispatch_failed"


def action_payload(case: dict[str, Any], observation: dict[str, Any]) -> dict[str, Any]:
    action = case["action"]
    if "input_profile" in action:
        return {"kind": "text", "text": INPUT_PROFILES[action["input_profile"]]}
    if "option" in action:
        option = named(observation, "option", action["option"])
        return {"kind": "select", "option_object_id": option["object_id"]}
    return {"kind": "none"}


def run_case(mcp: Any, case: dict[str, Any]) -> dict[str, Any]:
    started = time.perf_counter()
    initial: dict[str, Any] | None = None
    receipt: dict[str, Any] | None = None
    observed = False
    outcome = "dispatch_failed"
    limitation = ""
    dispatch_status = ""
    postcondition = ""
    changes: list[dict[str, Any]] = []
    try:
        initial = stable_observation(mcp, open_fixture(mcp, case["url"])["tab_id"])
        observed = True
        working = initial
        target: dict[str, Any] | None = None
        for _attempt in range(8):
            working = stable_observation(mcp, working["tab_id"])
            target = named(working, case["target"]["role"], case["target"]["name"])
            arguments = action_arguments(
                working,
                target,
                case["action"]["operation"],
                action_payload(case, working),
            )
            receipt = mcp.tool("web.act", arguments, timeout=45.0)
            if receipt.get("dispatch_status") != "stale_before_dispatch":
                break
            working = receipt["post_action_observation"]
        assert target is not None and receipt is not None
        after = receipt["post_action_observation"]
        changes = after.get("changes", [])
        dispatch_status = str(receipt.get("dispatch_status", ""))
        postcondition = str(receipt.get("postcondition", ""))
        expected = case["postcondition"]
        current = next((item for item in after.get("objects", []) if (
            item.get("role") == expected.get("role", case["target"]["role"])
            and item.get("name") == expected.get("name", case["target"]["name"])
        )), None)
        state_matches = current is not None and current.get("state", {}).get(expected["state"]) == expected["equals"]
        if postcondition == "verified" and state_matches:
            outcome = "verified"
        elif dispatch_status.startswith("accepted_by_"):
            outcome = "unverified"
            limitation = "Input was accepted, but fresh semantic evidence did not prove the declared postcondition."
        elif dispatch_status == "stale_before_dispatch":
            outcome = "prepare_rejected"
            limitation = "The page did not provide a stable current action basis after eight fresh observations."
        else:
            outcome = "dispatch_failed"
            limitation = "The registered input backend did not accept the action."
    except Exception as error:  # noqa: BLE001
        outcome = classify_error(error, observed)
        limitation = str(error)[:500]
    assert outcome in OUTCOMES
    return {
        "id": case["id"],
        "control": case["control"],
        "source": case["source"],
        "implementation": case["implementation"],
        "url": case["url"],
        "outcome": outcome,
        "observed": observed,
        **({"dispatch_status": dispatch_status} if dispatch_status else {}),
        **({"postcondition": postcondition} if postcondition else {}),
        "elapsed_ms": round((time.perf_counter() - started) * 1000, 3),
        **({"limitation": limitation} if limitation else {}),
        "evidence": {
            "initial_view": compact_view(initial) if initial else {},
            "changes": changes,
            "receipt": (
                {key: receipt[key] for key in ("dispatch_status", "postcondition") if key in receipt}
                if receipt else None
            ),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--cases", default=DEFAULT_CASES, type=Path)
    parser.add_argument("--case", action="append", dest="selected_cases")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    selected = set(args.selected_cases or [])
    cases = [case for case in load_cases(args.cases.resolve()) if not selected or case["id"] in selected]
    if selected - {case["id"] for case in cases}:
        raise SystemExit(f"unknown external case ids: {sorted(selected - {case['id'] for case in cases})}")
    started_at = utc_now()
    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve(), reference=True)
    try:
        results = [run_case(mcp, case) for case in cases]
    finally:
        mcp.close()
    by_outcome = {outcome: sum(result["outcome"] == outcome for result in results) for outcome in sorted(OUTCOMES)}
    report = {
        "schema": "saccade.external-evidence/1",
        "browser": args.browser,
        "candidate_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip(),
        "started_at": started_at,
        "finished_at": utc_now(),
        "summary": {
            "total": len(results),
            "verified": by_outcome["verified"],
            "failed": len(results) - by_outcome["verified"],
            "by_outcome": by_outcome,
        },
        "cases": results,
    }
    serialized = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    for secret in INPUT_PROFILES.values():
        if secret in serialized:
            raise RuntimeError("external evidence contains editable test data")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(serialized, encoding="utf-8")
    print(json.dumps({"verified": by_outcome["verified"], "total": len(results), "evidence": str(args.output)}))
    return 0 if by_outcome["verified"] == len(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
