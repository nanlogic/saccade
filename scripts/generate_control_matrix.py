#!/usr/bin/env python3
"""Validate the v1 Catalog invariants and generate the public implementation matrix."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "catalog" / "controls.json"
OUTPUT = ROOT / "docs" / "generated" / "control_coverage.md"

ALLOWED_ROLES = {
    "button", "text_field", "search_field", "text_area", "content_editable",
    "spin_button", "checkbox", "select",
}
ALLOWED_STATES = {
    "has_value", "checked", "enabled", "selected", "expanded", "required",
    "readonly", "pressed", "invalid",
}


def load_and_validate() -> dict:
    data = json.loads(CATALOG.read_text(encoding="utf-8"))
    if set(data) != {"catalog_version", "controls"} or data["catalog_version"] != 1:
        raise SystemExit("invalid Catalog envelope")
    ids: set[str] = set()
    roles: set[str] = set()
    for control in data["controls"]:
        if control["id"] in ids or control["role"] in roles:
            raise SystemExit("duplicate Catalog id or role")
        ids.add(control["id"])
        roles.add(control["role"])
        if control["role"] not in ALLOWED_ROLES:
            raise SystemExit(f"unapproved Catalog role: {control['role']}")
        if not set(control["safe_state"]) <= ALLOWED_STATES:
            raise SystemExit(f"unapproved state in {control['id']}")
        evidence = control["evidence"]
        if control["publication_status"] == "publishable" and evidence != {"chrome": "passed", "edge": "passed"}:
            raise SystemExit(f"{control['id']} cannot be publishable without Chrome and Edge evidence")
    if roles != ALLOWED_ROLES:
        raise SystemExit("Catalog roles do not match the implemented control batches")
    return data


def render(data: dict) -> str:
    lines = [
        "# Generated Control Coverage",
        "",
        "> Generated from `catalog/controls.json`; do not edit by hand.",
        "",
        "| Control | Family | Affordance | Native primitive | Verifier | Chrome | Edge | Status |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for control in data["controls"]:
        lines.append(
            "| {id} | {implementation_family} | {affordances} | {native_primitive} | "
            "{verifier} | {chrome} | {edge} | {publication_status} |".format(
                id=control["id"],
                implementation_family=control["implementation_family"],
                affordances=", ".join(control["affordances"]),
                native_primitive=control["native_primitive"],
                verifier=control["verifier"],
                chrome=control["evidence"]["chrome"],
                edge=control["evidence"]["edge"],
                publication_status=control["publication_status"],
            )
        )
    lines.extend(["", "No row is `publishable` until current Chrome and Edge artifacts pass for the same release candidate.", ""])
    return "\n".join(lines)


def main() -> None:
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(render(load_and_validate()), encoding="utf-8")
    print(OUTPUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
