#!/usr/bin/env python3
"""Compare independent Saccade and Playwright public-page results."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def by_control(data: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {item["control"]: item for item in data.get("cases", [])}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--saccade", required=True, type=Path)
    parser.add_argument("--playwright", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    saccade = json.loads(args.saccade.read_text(encoding="utf-8"))
    playwright = json.loads(args.playwright.read_text(encoding="utf-8"))
    if not saccade.get("ok") or not playwright.get("ok"):
        raise SystemExit("both independent runs must pass before comparison")
    saccade_cases = by_control(saccade)
    playwright_cases = by_control(playwright)
    if set(saccade_cases) != set(playwright_cases):
        raise SystemExit("Saccade and Playwright control sets differ")

    comparisons = []
    for control in sorted(saccade_cases):
        native = saccade_cases[control]
        oracle = playwright_cases[control]
        native_name = native["target_after"].get("name")
        matched = (
            native["dispatch_status"] == "accepted_by_os"
            and native["postcondition"] == "verified"
            and native_name == oracle["name"]
            and oracle["before"] == "false"
            and oracle["after"] == "true"
            and oracle["passed"] is True
        )
        comparisons.append(
            {
                "control": control,
                "saccade": f"{native['dispatch_status']} + {native['postcondition']}",
                "playwright": "passed" if oracle["passed"] else "failed",
                "semantic_name": native_name,
                "state_transition": "false -> true",
                "matched": matched,
                "playwright_screenshot": oracle["screenshot"],
            }
        )
    result = {
        "ok": all(item["matched"] for item in comparisons),
        "browser": saccade["browser"],
        "saccade_route": "Extension -> Native Host -> Runtime -> MCP -> native input",
        "playwright_role": "out-of-band reference oracle only",
        "comparisons": comparisons,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    markdown = [
        "# External control comparison",
        "",
        f"Browser: {result['browser']}",
        "",
        "| Control | Saccade | Playwright | Name | State | Matched |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for item in comparisons:
        markdown.append(
            f"| {item['control']} | {item['saccade']} | {item['playwright']} | "
            f"{item['semantic_name']} | {item['state_transition']} | {str(item['matched']).lower()} |"
        )
    args.output.with_suffix(".md").write_text("\n".join(markdown) + "\n", encoding="utf-8")
    print(json.dumps({"ok": result["ok"], "output": str(args.output)}))
    if not result["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
