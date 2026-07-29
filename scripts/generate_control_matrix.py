#!/usr/bin/env python3
"""Validate the v1 Catalog invariants and generate the public implementation matrix."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "catalog" / "controls.json"
DEVELOPMENT_EVIDENCE = ROOT / "catalog" / "development_evidence.json"
OUTPUT = ROOT / "docs" / "generated" / "control_coverage.md"

ALLOWED_ROLES = {
    "button", "text_field", "search_field", "text_area", "content_editable",
    "spin_button", "checkbox", "radio", "switch", "select", "tab", "menu_item",
    "reflex_target", "link", "file_input",
}
ALLOWED_STATES = {
    "has_value", "checked", "enabled", "selected", "expanded", "required",
    "readonly", "pressed", "current", "invalid", "reflex_occurrence",
}


def load_and_validate() -> tuple[dict, dict]:
    data = json.loads(CATALOG.read_text(encoding="utf-8"))
    development = json.loads(DEVELOPMENT_EVIDENCE.read_text(encoding="utf-8"))
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
    if set(development) != {"evidence_version", "controls"} or development["evidence_version"] != 1:
        raise SystemExit("invalid development evidence envelope")
    if set(development["controls"]) != roles:
        raise SystemExit("development evidence roles do not match the Catalog")
    expected_tiers = {"fixture", "external"}
    expected_browsers = {"chrome", "edge"}
    for role, evidence in development["controls"].items():
        if set(evidence) != expected_tiers:
            raise SystemExit(f"invalid development evidence tiers for {role}")
        for tier, browsers in evidence.items():
            if set(browsers) != expected_browsers or not set(browsers.values()) <= {"passed", "pending"}:
                raise SystemExit(f"invalid {tier} browser evidence for {role}")
        if any(
            evidence["external"][browser] == "passed"
            and evidence["fixture"][browser] != "passed"
            for browser in expected_browsers
        ):
            raise SystemExit(f"external evidence requires fixture evidence for {role}")
    return data, development


def paired(evidence: dict) -> str:
    return f"{evidence['chrome']} / {evidence['edge']}"


def render(data: dict, development: dict) -> str:
    fixture_both = sum(
        all(status == "passed" for status in evidence["fixture"].values())
        for evidence in development["controls"].values()
    )
    external_both = sum(
        all(status == "passed" for status in evidence["external"].values())
        for evidence in development["controls"].values()
    )
    publishable = sum(control["publication_status"] == "publishable" for control in data["controls"])
    lines = [
        "# Generated Control Coverage",
        "",
        "> Generated from `catalog/controls.json` and `catalog/development_evidence.json`; do not edit by hand.",
        "",
        "## Evidence summary",
        "",
        f"Implemented: {len(data['controls'])}. Chrome + Edge fixture: {fixture_both}. Chrome + Edge external: {external_both}. Publishable: {publishable}.",
        "",
        "Chrome / Edge values are shown in that order.",
        "",
        "| Control | Implemented | Fixture C / E | External C / E | Release C / E |",
        "| --- | --- | --- | --- | --- |",
    ]
    for control in data["controls"]:
        evidence = development["controls"][control["role"]]
        lines.append(
            f"| {control['id']} | yes | {paired(evidence['fixture'])} | "
            f"{paired(evidence['external'])} | {paired(control['evidence'])} |"
        )
    lines.extend([
        "",
        "`Fixture` and `External` are local development evidence. `Release` stays pending until a signed release candidate passes.",
        "",
        "## Module details",
        "",
        "| Control | Family | Affordance | Native primitive | Verifier | Chrome | Edge | Status |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ])
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
    data, development = load_and_validate()
    OUTPUT.write_text(render(data, development), encoding="utf-8")
    print(OUTPUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
