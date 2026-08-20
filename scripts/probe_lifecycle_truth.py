#!/usr/bin/env python3
"""Exercise page-driven lifecycle transitions through default Truth MCP."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any

from dev_probe import Mcp, fold_truth_change, open_when_browser_ready, wait_for_mcp


EXPECTED_MARKERS = {
    "LC|delayed-render",
    "LC|slow-resource",
    "LC|disappearance",
    "LC|replacement",
    "LC|modal-open",
    "LC|modal-close",
    "LC|infinite-append",
    "LC|large-dom-replacement",
    "LC|table-reorder",
    "LC|viewport-change",
    "LC|done",
}


def raw_tool(mcp: Mcp, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
    result = mcp.rpc("tools/call", {"name": f"saccade.{name}", "arguments": arguments}, timeout=timeout)
    return result["structuredContent"]


def rejected_tool(mcp: Mcp, name: str, arguments: dict[str, Any]) -> str:
    try:
        raw_tool(mcp, name, arguments)
    except RuntimeError as error:
        return str(error)
    raise RuntimeError(f"saccade.{name} unexpectedly succeeded")


def label(item: dict[str, Any]) -> str:
    return str(item.get("name") or item.get("text") or "")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", required=True, choices=("chrome", "edge"))
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    mcp = wait_for_mcp(args.runtime, args.runtime_dir)
    try:
        capabilities = raw_tool(mcp, "system.capabilities", {})
        opened = open_when_browser_ready(mcp, args.url)
        tab_id = str(opened["tab_id"])
        listed_after_open = raw_tool(mcp, "tabs.list", {})
        listed_open_tab = next(
            (tab for tab in listed_after_open.get("tabs", []) if str(tab.get("tab_id")) == tab_id),
            None,
        )
        # The lifecycle fixture can exceed the automatic full-view budget.
        # Use the shared diagnostic materializer so a catalog is completed and
        # expanded before this probe starts consuming raw deltas.
        initial = mcp.tool("truth.read", {"tab_id": str(opened["tab_id"])})
        revision = int(initial["revision"])
        current = {item["object_id"]: item for item in initial.get("objects", [])}

        def find_id(expected: str) -> str:
            return next((key for key, item in current.items() if label(item) == expected), "")

        remove_id = find_id("Lifecycle remove target")
        replace_id = find_id("Lifecycle replacement old")
        anchor_id = find_id("Viewport anchor")
        anchor_initial_y = current.get(anchor_id, {}).get("viewport_bounds", {}).get("y")
        initial_roles = {(item.get("role"), label(item)): item for item in current.values()}
        required_initial = {
            ("file_input", "Upload lifecycle file"),
            ("link", "Download lifecycle sample"),
            ("generic_control", "Drag source"),
            ("generic_control", "Drop target"),
        }
        missing_initial = sorted(f"{role}:{name}" for role, name in required_initial if (role, name) not in initial_roles)
        structural_ids = {key for key, item in current.items() if item.get("role") in {"row", "cell"}}

        seen_markers: set[str] = set()
        remove_disappeared = False
        replace_disappeared = False
        replacement_appeared = False
        dialog_id = ""
        modal_appeared = False
        modal_disappeared = False
        infinite_items: set[str] = set()
        structural_identity_churn = 0
        anchor_moved = False
        views: list[dict[str, Any]] = []
        wait_error = ""
        pending_page = False
        deadline = time.monotonic() + 15
        while "LC|done" not in seen_markers and time.monotonic() < deadline:
            try:
                read_arguments = {"tab_id": str(opened["tab_id"])}
                if not pending_page:
                    read_arguments.update({"after_revision": revision, "timeout_ms": 5000})
                view = raw_tool(
                    mcp,
                    "truth.read",
                    read_arguments,
                    timeout=7,
                )
            except RuntimeError as error:
                if "no observation after revision" not in str(error):
                    raise
                wait_error = str(error)
                break
            revision = int(view["revision"])
            pending_page = (view.get("page") or {}).get("complete") is False
            changes = view.get("changes", [])
            views.append({
                "revision": revision,
                "mode": view.get("mode"),
                "change_count": len(changes),
                "object_count": len(view.get("objects", [])),
            })
            if view.get("mode") == "full":
                next_current = {item["object_id"]: item for item in view.get("objects", [])}
                next_ids = set(next_current)
                remove_disappeared |= bool(remove_id) and remove_id not in next_ids
                replace_disappeared |= bool(replace_id) and replace_id not in next_ids
                modal_disappeared |= bool(dialog_id) and dialog_id not in next_ids
                structural_identity_churn += len(structural_ids - next_ids)
                current = next_current
                observed_items = list(next_current.values())
            else:
                observed_items = []
            for change in changes:
                object_id = str(change.get("object_id") or change.get("object", {}).get("object_id") or "")
                kind = change.get("kind")
                if kind == "disappeared":
                    remove_disappeared |= object_id == remove_id
                    replace_disappeared |= object_id == replace_id
                    modal_disappeared |= object_id == dialog_id
                    structural_identity_churn += int(object_id in structural_ids)
                    current.pop(object_id, None)
                    continue
                item = fold_truth_change(current, change, view.get("object_defaults")) or {}
                observed_items.append(item)
                structural_identity_churn += int(kind == "appeared" and item.get("role") in {"row", "cell"})

            for item in observed_items:
                object_id = str(item.get("object_id") or "")
                item_label = label(item)
                if item_label in EXPECTED_MARKERS:
                    seen_markers.add(item_label)
                replacement_appeared |= item_label == "Lifecycle replacement new"
                if item_label == "Lifecycle modal" and item.get("state", {}).get("modal") == "true":
                    dialog_id = object_id
                    modal_appeared = True
                if item_label.startswith("Infinite item "):
                    infinite_items.add(item_label)
                if object_id == anchor_id and anchor_initial_y is not None:
                    anchor_moved |= item.get("viewport_bounds", {}).get("y") != anchor_initial_y

        missing_markers = sorted(EXPECTED_MARKERS - seen_markers)
        transitions_passed = all(
            (
                not missing_initial,
                not missing_markers,
                bool(remove_id) and remove_disappeared,
                bool(replace_id) and replace_disappeared and replacement_appeared,
                modal_appeared and modal_disappeared,
                len(infinite_items) == 20,
                structural_identity_churn == 0,
                anchor_moved,
            )
        )
        closed = raw_tool(mcp, "tabs.close", {"tab_id": tab_id})
        listed_after_close = raw_tool(mcp, "tabs.list", {})
        absent_after_close = all(
            str(tab.get("tab_id")) != tab_id for tab in listed_after_close.get("tabs", [])
        )
        repeated_close_error = rejected_tool(mcp, "tabs.close", {"tab_id": tab_id})
        retired_truth_error = rejected_tool(mcp, "truth.read", {"tab_id": tab_id})
        session_retired = "outside this MCP session" in repeated_close_error
        truth_session_retired = "outside this MCP session" in retired_truth_error
        lifecycle_cleanup = {
            "listed_ownership": None if listed_open_tab is None else listed_open_tab.get("ownership"),
            "closed": closed.get("closed") is True,
            "absent_after_close": absent_after_close,
            "repeat_close_rejected": (
                "only Agent-owned tabs" in repeated_close_error or session_retired
            ),
            "retired_truth_rejected": (
                "no current observation for tab" in retired_truth_error
                or truth_session_retired
            ),
            "repeat_close_error": repeated_close_error,
            "retired_truth_error": retired_truth_error,
        }
        cleanup_passed = lifecycle_cleanup["listed_ownership"] == "agent" and all(
            lifecycle_cleanup[key]
            for key in (
                "closed",
                "absent_after_close",
                "repeat_close_rejected",
                "retired_truth_rejected",
            )
        )
        passed = transitions_passed and cleanup_passed
        evidence = {
            "schema": "saccade.lifecycle-evidence/1",
            "browser": args.browser,
            "tab_id": str(opened["tab_id"]),
            "execution_owner": "agent_client",
            "extension_candidate": capabilities.get("extension_candidate"),
            "stimulus": "page_driven_fixture",
            "initial_representation": {
                "missing": missing_initial,
                "file_input": "value-free file_input Truth",
                "download": "link Truth; download execution remains Agent-owned",
                "drag_drop": "generic_control Truth with explicit affordance/limitation",
            },
            "transitions": {
                "markers_seen": sorted(seen_markers),
                "markers_missing": missing_markers,
                "remove_disappeared": remove_disappeared,
                "replace_disappeared": replace_disappeared,
                "replacement_appeared": replacement_appeared,
                "modal_appeared": modal_appeared,
                "modal_disappeared": modal_disappeared,
                "infinite_items": len(infinite_items),
                "table_identity_churn": structural_identity_churn,
                "viewport_geometry_updated": anchor_moved,
            },
            "views": views,
            "terminal_wait": wait_error or None,
            "tab_lifecycle": lifecycle_cleanup,
            "passed": passed,
        }
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"ok": passed, "browser": args.browser, "evidence": str(args.output)}))
        if not passed:
            raise SystemExit(1)
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
