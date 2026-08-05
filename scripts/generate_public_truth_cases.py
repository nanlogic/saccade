#!/usr/bin/env python3
"""Generate the explicit public Truth evidence denominator without hiding gaps."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
TRUTH = ROOT / "catalog/truth_inventory.json"
HISTORICAL = ROOT / "catalog/external_cases.json"
DENOMINATOR = ROOT / "catalog/control_denominator_sources.json"
OUTPUT = ROOT / "catalog/public_truth_cases.json"

LIFECYCLE_SCENARIOS = [
    "dynamic_loading",
    "element_disappearance",
    "overlay_modal",
    "dialog",
    "infinite_scroll",
    "sortable_table",
    "slow_resource",
    "upload_download_truth",
    "large_dom_replacement",
    "viewport_change",
    "delayed_render",
]

LIMITED_TARGETS = {
    "role:unknown": "reserved role must not appear in Agent Truth",
    "variant:drop_target": "observation-only target; no execution authority",
    "variant:built_in_pdf": "browser-owned PDF remains an opaque restricted document",
    "boundary:restricted_frame": "inaccessible frame is reported as restricted",
    "boundary:closed_shadow_root": "closed shadow contents remain opaque",
}


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def historical_sources(cases: dict[str, Any], role: str) -> list[dict[str, str]]:
    seen: set[tuple[str, str]] = set()
    result = []
    for case in cases["cases"]:
        if case["control"] != role:
            continue
        key = (case["source"], case["url"])
        if key in seen:
            continue
        seen.add(key)
        result.append({
            "name": case["source"],
            "url": case["url"],
            "status": "historical_reference_only",
        })
    return result


def classification_index(denominator: dict[str, Any]) -> dict[str, str]:
    result: dict[str, str] = {}
    for classification, ids in denominator["classifications"].items():
        for item_id in ids:
            if item_id in result:
                raise SystemExit(f"duplicate denominator classification: {item_id}")
            result[item_id] = classification
    return result


def standard_sources(denominator: dict[str, Any], classification: str) -> list[dict[str, str]]:
    wanted = {
        "mainstream_control": {"whatwg_html_forms", "wai_aria_roles", "aria_apg_patterns"},
        "uncommon_control": {"whatwg_html_forms", "wai_aria_roles", "aria_apg_patterns"},
        "semantic_object": {"wai_aria_roles"},
        "truthful_boundary": {"whatwg_html_forms", "wai_aria_roles"},
    }[classification]
    return [
        {"name": row["name"], "url": row["url"], "status": row["authority"]}
        for row in denominator["sources"] if row["id"] in wanted
    ]


def item(kind: str, target: str, historical: dict[str, Any], denominator: dict[str, Any], classifications: dict[str, str]) -> dict[str, Any]:
    item_id = f"{kind}:{target}"
    classification = classifications.get(item_id)
    if classification is None:
        raise SystemExit(f"denominator classification missing: {item_id}")
    sources = standard_sources(denominator, classification)
    sources.extend(historical_sources(historical, target) if kind == "role" else [])
    limitation = LIMITED_TARGETS.get(item_id, "")
    reason = "standards_denominator_merged; public_runtime_evidence_pending"
    if sources:
        reason += "; historical_reference_evidence_requires_core_revalidation"
    return {
        "id": item_id,
        "kind": kind,
        "target": target,
        "classification": classification,
        "sources": sources,
        "implementations": sorted({
            case["implementation"]
            for case in historical["cases"]
            if kind == "role" and case["control"] == target
        }),
        "expected_initial": (
            "target is absent from Agent Truth" if item_id == "role:unknown"
            else "truthful role/state/identity projection or declared limitation"
        ),
        "expected_transition": (
            "no transition required for an inaccessible boundary"
            if limitation else "Extension-produced appeared/updated/disappeared delta when the page changes"
        ),
        "expected_limitation": limitation,
        "browsers": ["chrome", "edge"],
        "pass_criteria": (
            limitation or
            "two independent public sources and a truthful initial view plus externally caused semantic delta"
        ),
        "outcome": "blocked",
        "reason": reason,
    }


def render() -> dict[str, Any]:
    truth = load(TRUTH)
    historical = load(HISTORICAL)
    denominator = load(DENOMINATOR)
    classifications = classification_index(denominator)
    items = [item("role", row["role"], historical, denominator, classifications) for row in truth["roles"]]
    items.extend(item("variant", row["id"], historical, denominator, classifications) for row in truth["variants"])
    items.extend(item("boundary", row["id"], historical, denominator, classifications) for row in truth["structural_boundaries"])
    for scenario in LIFECYCLE_SCENARIOS:
        items.append({
            "id": f"lifecycle:{scenario}",
            "kind": "lifecycle",
            "target": scenario,
            "classification": "lifecycle",
            "sources": [],
            "implementations": [],
            "expected_initial": "truthful bounded page state before the lifecycle event",
            "expected_transition": "Extension-produced semantic delta or explicit reset after the lifecycle event",
            "expected_limitation": "",
            "browsers": ["chrome", "edge"],
            "pass_criteria": "one traceable public case in both browsers with retained full/delta evidence",
            "outcome": "blocked",
            "reason": "missing_core_public_case",
        })
    return {
        "schema": "saccade.public-truth-cases/1",
        "generated_from": [
            str(TRUTH.relative_to(ROOT)),
            str(HISTORICAL.relative_to(ROOT)),
            str(DENOMINATOR.relative_to(ROOT)),
        ],
        "source_documents": [
            {
                "id": "truth_inventory",
                "status": "merged",
                "reason": "canonical 34 role, 12 variant, and 6 boundary inventory",
            },
            {
                "id": "legacy_lifecycle_gauntlet_reference",
                "status": "merged",
                "reason": "lifecycle scenarios recorded by the current architecture documents",
            },
            {
                "id": "standards_mainstream_uncommon_controls",
                "status": "merged",
                "reason": "WHATWG HTML, WAI-ARIA roles, and ARIA APG replace the unavailable historical document",
            },
        ],
        "outcomes": ["pass", "truthful_limitation", "unsupported", "blocked"],
        "summary": {
            "roles": len(truth["roles"]),
            "variants": len(truth["variants"]),
            "boundaries": len(truth["structural_boundaries"]),
            "lifecycle_scenarios": len(LIFECYCLE_SCENARIOS),
            "total": len(items),
        },
        "items": items,
    }


def validate(document: dict[str, Any]) -> None:
    expected = {"pass", "truthful_limitation", "unsupported", "blocked"}
    if document["outcomes"] != ["pass", "truthful_limitation", "unsupported", "blocked"]:
        raise SystemExit("public Truth outcome order changed")
    ids = [row["id"] for row in document["items"]]
    if len(ids) != len(set(ids)) or document["summary"]["total"] != len(ids):
        raise SystemExit("public Truth denominator contains duplicate or missing rows")
    if any(row["outcome"] not in expected or not row["reason"] for row in document["items"]):
        raise SystemExit("every public Truth row must have an explicit outcome and reason")
    if document["summary"] | {"total": 0} != {
        "roles": 34, "variants": 12, "boundaries": 6,
        "lifecycle_scenarios": len(LIFECYCLE_SCENARIOS), "total": 0,
    }:
        raise SystemExit("public Truth denominator no longer matches the canonical inventory")


def main() -> None:
    document = render()
    validate(document)
    OUTPUT.write_text(json.dumps(document, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(OUTPUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
