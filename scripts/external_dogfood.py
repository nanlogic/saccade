#!/usr/bin/env python3
"""Run public-page control proofs through Saccade's production route."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from dev_probe import act, named, open_fixture, stable_observation, wait_for_mcp


CASES = (
    {
        "control": "radio",
        "url": "https://www.w3.org/WAI/ARIA/apg/patterns/radio/examples/radio/",
        "role": "radio",
        "name": "Deep dish",
        "state": "checked",
    },
    {
        "control": "switch",
        "url": "https://www.w3.org/WAI/ARIA/apg/patterns/switch/examples/switch/",
        "role": "switch",
        "name": "Notifications",
        "state": "checked",
    },
    {
        "control": "tab",
        "url": "https://www.w3.org/WAI/ARIA/apg/patterns/tabs/examples/tabs-manual/",
        "role": "tab",
        "name": "Carl Andersen",
        "state": "selected",
    },
    {
        "control": "menu_item",
        "url": "https://www.w3.org/WAI/ARIA/apg/patterns/menubar/examples/menubar-navigation/",
        "role": "menu_item",
        "name": "About",
        "state": "expanded",
    },
)


def projected(item: dict[str, Any]) -> dict[str, Any]:
    return {
        key: item.get(key)
        for key in ("role", "name", "description", "state", "affordances")
        if item.get(key) is not None
    }


def run_case(mcp: Any, case: dict[str, str]) -> dict[str, Any]:
    observation = open_fixture(mcp, case["url"])
    observation = stable_observation(mcp, observation["tab_id"])
    before = named(observation, case["role"], case["name"])
    receipt, after = act(
        mcp,
        observation,
        case["role"],
        case["name"],
        "click",
        lambda _: {"kind": "none"},
    )
    after_target = next(
        (item for item in after["objects"] if item.get("object_id") == before["object_id"]),
        None,
    )
    if not after_target or after_target.get("state", {}).get(case["state"]) != "true":
        raise RuntimeError(f"{case['control']} did not expose {case['state']}=true")
    return {
        "control": case["control"],
        "url": case["url"],
        "target_before": projected(before),
        "target_after": projected(after_target),
        "dispatch_status": receipt["dispatch_status"],
        "postcondition": receipt["postcondition"],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    result: dict[str, Any]
    cases: list[dict[str, Any]] = []
    active_control: str | None = None
    try:
        mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
        try:
            for case in CASES:
                active_control = case["control"]
                cases.append(run_case(mcp, case))
        finally:
            mcp.close()
        result = {
            "ok": True,
            "mode": "external_dogfood",
            "browser": args.browser,
            "source": "W3C WAI-ARIA Authoring Practices public examples",
            "cases": cases,
        }
    except Exception as error:  # noqa: BLE001
        result = {
            "ok": False,
            "mode": "external_dogfood",
            "browser": args.browser,
            "failed_control": active_control,
            "error": str(error),
            "completed_cases": cases,
        }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": result["ok"], "evidence": str(args.output)}))
    if not result["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
