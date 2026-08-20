#!/usr/bin/env python3
"""Gate every implemented non-control Truth role and structural boundary."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any

from dev_probe import fold_truth_change, stable_observation, wait_for_mcp


TARGETS = {
    "heading": ("heading", "text", "Catalog controls"),
    "paragraph": ("paragraph", "text", "This page proves native control loops and bounded structural reading."),
    "list_item": ("list_item", "text", "Observe the current page"),
    "cell": ("cell", "text", "Evidence"),
    "alert": ("alert", "text", "Fixture ready"),
    "status": ("status", "text", "No actions yet"),
    "image": ("image", "name", "Gear Up cover"),
    "option": ("option", "name", "Red"),
    "text": ("text", "text", "Standalone semantic text"),
    "list": ("list", "text", "Workflow list"),
    "table": ("table", "text", "Evidence table"),
    "row": ("row", "text", "Evidence row"),
    "slider": ("slider", "name", "Range"),
    "label": ("label", "name", "Deployment label"),
    "drag_source": ("generic_control", "name", "Drag source"),
    "drop_target": ("generic_control", "name", "Drop target"),
    "date": ("text_field", "name", "Date"),
    "time": ("text_field", "name", "Time"),
    "month": ("text_field", "name", "Month"),
    "week": ("text_field", "name", "Week"),
    "datetime_local": ("text_field", "name", "Date and time"),
    "color": ("text_field", "name", "Color"),
    "opaque_canvas": ("opaque_surface", "name", "Canvas surface"),
    "opaque_webgl": ("opaque_surface", "name", "WebGL surface"),
    "opaque_video": ("opaque_surface", "name", "Video surface"),
    "restricted_document": ("restricted_document", "name", "Restricted document"),
    "built_in_pdf": ("restricted_document", "name", "Built-in PDF"),
    "native_listbox": ("select", "name", "Native listbox"),
    "aria_listbox": ("select", "name", "Priority"),
    "aria_combobox": ("select", "name", "Portal city"),
}


def tool(mcp: Any, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    return mcp.rpc("tools/call", {"name": f"saccade.{name}", "arguments": arguments}, timeout=timeout)["structuredContent"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", choices=("chrome", "edge"), required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--structure-url", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
    expected_roles = {item["role"] for item in inventory["roles"] if item.get("gate") == "semantic"}
    target_roles = {target[0] for target in TARGETS.values()}
    if not expected_roles.issubset(target_roles):
        raise RuntimeError(f"semantic inventory roles lack runner targets: {sorted(expected_roles - target_roles)}")
    expected_variants = {item["id"] for item in inventory.get("variants", []) if item.get("gate") == "semantic"}
    if not expected_variants.issubset(TARGETS):
        raise RuntimeError(f"semantic inventory variants lack runner targets: {sorted(expected_variants - set(TARGETS))}")

    mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
    try:
        opened = tool(mcp, "tabs.open", {"url": args.url, "active": True})
        tab_id = str(opened["tab_id"])
        initial = mcp.tool("truth.read", {"tab_id": tab_id})
        if any(item.get("role") == "unknown" for item in initial.get("objects", [])):
            raise RuntimeError("reserved unknown role escaped into public Truth")
        found: dict[str, dict[str, Any]] = {}
        for role, (expected_role, field, value) in TARGETS.items():
            item = next((x for x in initial.get("objects", []) if x.get("role") == expected_role and x.get(field) == value), None)
            if item is None:
                raise RuntimeError(f"initial Truth omitted semantic role {role}")
            actionable_variants = {"option", "slider", "drag_source", "date", "time", "month", "week", "datetime_local", "color", "native_listbox", "aria_listbox", "aria_combobox"}
            if item.get("affordances") and role not in actionable_variants:
                raise RuntimeError(f"non-control semantic role {role} became actionable")
            if item.get("action_token") is not None:
                raise RuntimeError(f"default Truth leaked action authority for {role}")
            if not isinstance(item.get("document_bounds"), dict) or not isinstance(item.get("viewport_bounds"), dict):
                raise RuntimeError(f"initial Truth omitted geometry for {role}")
            found[role] = {
                "object_id": item["object_id"],
                "initial": {k: item.get(k) for k in ("text", "name", "description", "state")},
                "geometry": {k: item.get(k) for k in ("document_bounds", "viewport_bounds")},
            }

        detached_popup_value = next(
            (item for item in initial.get("objects", []) if item.get("role") == "select" and item.get("name") == "Portal city"),
            None,
        )
        if detached_popup_value is None or detached_popup_value.get("state", {}).get("has_value") != "true":
            raise RuntimeError("closed ARIA combobox lost its value after the popup options detached")

        revision = int(initial["revision"])
        current = {item["object_id"]: item for item in initial.get("objects", [])}
        changed: dict[str, Any] = {}
        pending_page = False
        for _ in range(60):
            try:
                read_arguments = {"tab_id": tab_id}
                if not pending_page:
                    read_arguments.update({"after_revision": revision, "timeout_ms": 3000})
                view = tool(mcp, "truth.read", read_arguments, timeout=5.0)
            except RuntimeError as error:
                if "no observation after revision" in str(error):
                    break
                raise
            revision = int(view["revision"])
            pending_page = (view.get("page") or {}).get("complete") is False
            for change in view.get("changes", []):
                item = fold_truth_change(
                    current, change, view.get("object_defaults")
                ) or {}
                if item.get("role") == "select" and item.get("name") == "Portal city":
                    if item.get("state", {}).get("has_value") != "true":
                        raise RuntimeError("detached combobox popup cleared its retained value")
                if item.get("role") == "select" and item.get("name") == "Portal city popup":
                    raise RuntimeError("detaching choice popup escaped as a second select")
                for role, evidence in found.items():
                    if item.get("object_id") == evidence["object_id"]:
                        after = {k: item.get(k) for k in ("text", "name", "description", "state")}
                        if after != evidence["initial"]:
                            changed[role] = {"revision": revision, "kind": change.get("kind"), "after": after}
            if len(changed) == len(TARGETS):
                break
        missing = sorted(set(TARGETS) - set(changed))
        if missing:
            raise RuntimeError(f"semantic roles without pushed delta: {missing}")

        structural_opened = tool(mcp, "tabs.open", {"url": args.structure_url, "active": True})
        structural_tab_id = str(structural_opened["tab_id"])
        structural_deadline = time.monotonic() + 10
        observed: list[dict[str, Any]] = []
        restricted: list[dict[str, Any]] = []
        while time.monotonic() < structural_deadline:
            structural = stable_observation(mcp, structural_tab_id)
            observed = [frame for frame in structural.get("frames", []) if frame.get("status") == "observed"]
            restricted = [frame for frame in structural.get("frames", []) if frame.get("status") != "observed"]
            if len(observed) == 2 and len(restricted) == 1:
                break
            time.sleep(0.25)
        names = {item.get("name") for item in structural.get("objects", [])}
        if len(observed) != 2 or len(restricted) != 1:
            raise RuntimeError("frame metadata did not report root/same-origin/restricted coverage")
        if not {"Frame toggle", "Open shadow toggle"}.issubset(names):
            raise RuntimeError("same-origin frame or open Shadow DOM truth was omitted")
        if {"Opaque button", "Closed shadow must stay opaque"} & names:
            raise RuntimeError("opaque descendant truth escaped its boundary")
        limitation_kinds = {item.get("kind") for item in initial.get("limitations", [])}
        required_limitations = {"opaque_canvas", "opaque_webgl", "opaque_video", "browser_restricted_page", "built_in_pdf"}
        if not required_limitations.issubset(limitation_kinds):
            raise RuntimeError(f"opaque/restricted limitations missing: {sorted(required_limitations - limitation_kinds)}")

        evidence = {
            "schema": "saccade.semantic-truth-evidence/1",
            "browser": args.browser,
            "semantic_roles": {role: {**found[role], "delta": changed[role]} for role in TARGETS},
            "structure": {"observed_frames": len(observed), "restricted_frames": len(restricted), "open_shadow_observed": True, "closed_shadow_opaque": True},
            "inventory": inventory,
            "negative_roles": {"unknown": "not_emitted"},
        }
        serialized = json.dumps(evidence, indent=2, ensure_ascii=False) + "\n"
        for forbidden in ("action_token", "locator"):
            if forbidden in serialized.casefold():
                raise RuntimeError(f"semantic evidence contains forbidden field {forbidden}")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(serialized, encoding="utf-8")
        print(json.dumps({"ok": True, "browser": args.browser, "semantic_roles": len(TARGETS), "evidence": str(args.output)}))
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
