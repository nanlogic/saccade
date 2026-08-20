#!/usr/bin/env python3
"""Merge current two-browser Truth and lifecycle evidence into the 63-row denominator."""

from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


CONTROL_ROLES = {
    "button", "link", "text_field", "search_field", "text_area",
    "content_editable", "spin_button", "checkbox", "radio", "switch",
    "select", "option", "tab", "menu_item", "reflex_target", "file_input",
}
LIMITED = {
    "role:opaque_surface", "role:restricted_document", "role:unknown",
    "variant:drop_target", "variant:built_in_pdf",
    "boundary:restricted_frame", "boundary:closed_shadow_root",
}


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def semantic_pass(target: str, evidence: dict[str, Any]) -> bool:
    roles = evidence["semantic_roles"]
    if target == "opaque_surface":
        return all(name in roles for name in ("opaque_canvas", "opaque_webgl", "opaque_video"))
    if target == "generic_control":
        return all(name in roles for name in ("drag_source", "drop_target"))
    if target == "frame":
        return evidence["structure"].get("observed_frames", 0) >= 2
    if target == "unknown":
        return evidence["negative_roles"].get("unknown") == "not_emitted"
    return target in roles


def lifecycle_pass(target: str, evidence: dict[str, Any]) -> bool:
    transitions = evidence["transitions"]
    markers = set(transitions["markers_seen"])
    checks = {
        "dynamic_loading": "LC|replacement" in markers,
        "element_disappearance": transitions["remove_disappeared"],
        "overlay_modal": transitions["modal_appeared"] and transitions["modal_disappeared"],
        "dialog": transitions["modal_appeared"] and transitions["modal_disappeared"],
        "infinite_scroll": transitions["infinite_items"] == 20,
        "sortable_table": "LC|table-reorder" in markers and transitions["table_identity_churn"] == 0,
        "slow_resource": "LC|slow-resource" in markers,
        "upload_download_truth": not evidence["initial_representation"]["missing"],
        "large_dom_replacement": "LC|large-dom-replacement" in markers,
        "viewport_change": transitions["viewport_geometry_updated"],
        "delayed_render": "LC|delayed-render" in markers,
    }
    return evidence.get("passed") is True and checks[target]


def item_pass(item: dict[str, Any], bundle: dict[str, Any]) -> tuple[bool, str]:
    kind, target = item["kind"], item["target"]
    if kind == "role":
        if target in CONTROL_ROLES:
            return target in bundle["controls"]["controls"], "controls.json"
        return semantic_pass(target, bundle["semantics"]), "semantics.json"
    if kind == "variant":
        return target in bundle["semantics"]["semantic_roles"], "semantics.json"
    if kind == "lifecycle":
        return lifecycle_pass(target, bundle["lifecycle"]), "lifecycle.json"
    structure = bundle["semantics"]["structure"]
    boundary_checks = {
        "same_origin_frame": structure.get("observed_frames", 0) >= 2,
        "restricted_frame": structure.get("restricted_frames", 0) >= 1,
        "open_shadow_root": structure.get("open_shadow_observed") is True,
        "closed_shadow_root": structure.get("closed_shadow_opaque") is True,
        "stream_gap_reset": bundle["pushed_delta"].get("stream_gap_reset", {}).get("gap") is True,
        "resource_notification": bundle["resource_subscription"].get("notification", {}).get("method") == "notifications/resources/updated",
    }
    source = "pushed-delta.json" if target == "stream_gap_reset" else (
        "resource-subscription.json" if target == "resource_notification" else "semantics.json"
    )
    return boundary_checks[target], source


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--denominator", required=True, type=Path)
    parser.add_argument("--truth-root", required=True, type=Path)
    parser.add_argument("--lifecycle-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    denominator = load(args.denominator)
    candidate_manifest = load(args.truth_root / "candidate.json")
    bundles: dict[str, dict[str, Any]] = {}
    for browser in ("chrome", "edge"):
        truth = args.truth_root / browser / "truth"
        lifecycle_path = args.lifecycle_root / browser / "truth" / "lifecycle.json"
        bundles[browser] = {
            "controls": load(truth / "controls.json"),
            "semantics": load(truth / "semantics.json"),
            "pushed_delta": load(truth / "pushed-delta.json"),
            "resource_subscription": load(truth / "resource-subscription.json"),
            "lifecycle": load(lifecycle_path),
            "paths": {"truth": str(truth), "lifecycle": str(lifecycle_path)},
        }
    extension_candidates = {
        json.dumps(bundle["controls"]["capabilities"].get("extension_candidate"), sort_keys=True)
        for bundle in bundles.values()
    }
    if len(extension_candidates) != 1:
        raise RuntimeError("Chrome and Edge Truth evidence used different Extension candidates")
    extension_candidate = json.loads(extension_candidates.pop())
    for browser, bundle in bundles.items():
        if bundle["lifecycle"].get("extension_candidate") != extension_candidate:
            raise RuntimeError(f"{browser} lifecycle evidence used a different Extension candidate")

    rows = []
    for item in denominator["items"]:
        browsers = {}
        for browser, bundle in bundles.items():
            passed, source = item_pass(item, bundle)
            browsers[browser] = {
                "passed": passed,
                "source": str(
                    Path(bundle["paths"]["lifecycle"]) if source == "lifecycle.json"
                    else Path(bundle["paths"]["truth"]) / source
                ),
            }
        passed = all(value["passed"] for value in browsers.values())
        if not passed:
            raise RuntimeError(f"local denominator evidence failed for {item['id']}: {browsers}")
        rows.append({
            "id": item["id"],
            "classification": item["classification"],
            "local_outcome": "truthful_limitation" if item["id"] in LIMITED else "pass",
            "browsers": browsers,
            "publication_outcome": item["outcome"],
            "publication_reason": item["reason"],
        })

    summary = {
        "total": len(rows),
        "local_pass": sum(row["local_outcome"] == "pass" for row in rows),
        "local_truthful_limitation": sum(row["local_outcome"] == "truthful_limitation" for row in rows),
        "local_blocked": 0,
        "publication_blocked": sum(row["publication_outcome"] == "blocked" for row in rows),
    }
    report = {
        "schema": "saccade.denominator-evidence/1",
        "generated_at": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
        "extension_candidate": extension_candidate,
        "candidate_commit": candidate_manifest.get("commit"),
        "candidate_dirty": candidate_manifest.get("dirty"),
        "summary": summary,
        "boundary": {
            "local_gate": "current candidate in clean Chrome and Edge profiles",
            "publication": "unchanged; still requires declared public/client-owned evidence",
        },
        "items": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, "summary": summary, "output": str(args.output)}))


if __name__ == "__main__":
    main()
