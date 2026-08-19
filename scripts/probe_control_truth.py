#!/usr/bin/env python3
"""Prove every Truth Catalog control projects and emits a source delta."""

from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from dev_probe import fold_truth_change, wait_for_mcp


TARGETS = {
    "button": ("button", "Save"),
    "link": ("link", "Catalog link"),
    "text_field": ("text_field", "Email"),
    "search_field": ("search_field", "Search"),
    "text_area": ("text_area", "Notes"),
    "content_editable": ("content_editable", "Draft"),
    "spin_button": ("spin_button", "Quantity"),
    "checkbox": ("checkbox", "Remember me"),
    "radio": ("radio", "Fast plan"),
    "switch": ("switch", "Notifications"),
    "select": ("select", "Color"),
    "option": ("option", "Urgent"),
    "tab": ("tab", "Details"),
    "menu_item": ("menu_item", "More actions"),
    "reflex_target": ("reflex_target", "Training target"),
    "file_input": ("file_input", "Evidence file"),
}


def tool(mcp: Any, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    response = mcp.rpc(
        "tools/call",
        {"name": f"saccade.{name}", "arguments": arguments},
        timeout=timeout,
    )
    return response["structuredContent"]


def object_key(item: dict[str, Any]) -> tuple[str | None, str | None]:
    return item.get("role"), item.get("name")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    catalog = json.loads(args.catalog.read_text(encoding="utf-8"))
    catalog_by_id = {item["id"]: item for item in catalog["controls"]}
    if set(catalog_by_id) != set(TARGETS):
        raise RuntimeError("probe target set does not exactly match the Truth Catalog")

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    try:
        capabilities = tool(mcp, "system.capabilities", {})
        opened = tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        initial = mcp.tool("truth.read", {"tab_id": tab_id})
        objects = {object_key(item): item for item in initial.get("objects", [])}
        objects_by_id = {item["object_id"]: item for item in initial.get("objects", [])}
        initial_states: dict[str, Any] = {}
        checks: dict[str, Any] = {}
        for control_id, target in TARGETS.items():
            item = objects.get(target)
            if item is None:
                raise RuntimeError(f"initial Truth omitted {control_id}: {target!r}")
            if item.get("action_token") is not None:
                raise RuntimeError(f"default Truth leaked action authority for {control_id}")
            allowed_state = set(catalog_by_id[control_id]["safe_state"])
            actual_state = set(item.get("state", {}))
            if not actual_state.issubset(allowed_state):
                raise RuntimeError(f"{control_id} projected unsafe state {sorted(actual_state - allowed_state)}")
            actual_affordances = set(item.get("affordances", []))
            allowed_affordances = set(catalog_by_id[control_id]["affordances"])
            if not actual_affordances or not actual_affordances.issubset(allowed_affordances):
                raise RuntimeError(f"{control_id} projected invalid affordances {sorted(actual_affordances)}")
            initial_states[control_id] = item.get("state", {})
            checks[control_id] = {
                "role": item["role"],
                "name": item.get("name"),
                "state": item.get("state", {}),
                "affordances": item.get("affordances", []),
            }

        revision = int(initial["revision"])
        changed: dict[str, Any] = {}
        pending_page = False
        for _ in range(20):
            try:
                read_arguments = {"tab_id": tab_id}
                if not pending_page:
                    read_arguments.update({"after_revision": revision, "timeout_ms": 3000})
                view = tool(
                    mcp,
                    "truth.read",
                    read_arguments,
                    timeout=5.0,
                )
            except RuntimeError as error:
                if "no observation after revision" in str(error):
                    break
                raise
            revision = int(view["revision"])
            pending_page = (view.get("page") or {}).get("complete") is False
            for change in view.get("changes", []):
                item = fold_truth_change(
                    objects_by_id, change, view.get("object_defaults")
                ) or {}
                for control_id, target in TARGETS.items():
                    if object_key(item) == target and item.get("state") != initial_states[control_id]:
                        changed[control_id] = {
                            "kind": change.get("kind"),
                            "revision": revision,
                            "state": item.get("state", {}),
                        }
            if len(changed) == len(TARGETS):
                break
        missing = sorted(set(TARGETS) - set(changed))
        if missing:
            raise RuntimeError(f"controls without a browser-pushed state delta: {missing}")

        evidence = {
            "schema": "saccade.control-truth-evidence/1",
            "browser": args.browser,
            "tested_at": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
            "capabilities": capabilities,
            "initial_revision": initial["revision"],
            "final_revision": revision,
            "controls": {
                control_id: {"initial": checks[control_id], "delta": changed[control_id]}
                for control_id in TARGETS
            },
        }
        serialized = json.dumps(evidence, indent=2, ensure_ascii=False) + "\n"
        if "private" in serialized:
            raise RuntimeError("editable fixture content leaked into Truth evidence")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized, encoding="utf-8")
        print(json.dumps({"ok": True, "browser": args.browser, "controls": len(changed), "evidence": str(args.output)}))
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
