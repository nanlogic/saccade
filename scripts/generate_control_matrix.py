#!/usr/bin/env python3
"""Validate the v1 Catalog invariants and generate the public implementation matrix."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "catalog" / "controls.json"
DEVELOPMENT_EVIDENCE = ROOT / "catalog" / "development_evidence.json"
EXTERNAL_CASES = ROOT / "catalog" / "external_cases.json"
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


def load_and_validate() -> tuple[dict, dict, dict]:
    data = json.loads(CATALOG.read_text(encoding="utf-8"))
    development = json.loads(DEVELOPMENT_EVIDENCE.read_text(encoding="utf-8"))
    external_cases = json.loads(EXTERNAL_CASES.read_text(encoding="utf-8"))
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
        if control["input_policy"] == "software_preferred" and control["native_primitive"] not in {"primary_click", "select_option"}:
            raise SystemExit(f"software_preferred requires a registered software primitive in {control['id']}")
        evidence = control["evidence"]
        if control["publication_status"] == "publishable" and evidence != {"chrome": "passed", "edge": "passed"}:
            raise SystemExit(f"{control['id']} cannot be publishable without Chrome and Edge evidence")
    if roles != ALLOWED_ROLES:
        raise SystemExit("Catalog roles do not match the implemented control batches")
    if set(development) != {"evidence_version", "external_requirements", "controls", "records"} or development["evidence_version"] != 2:
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
    requirements = development["external_requirements"]
    if requirements != {"independent_sources_per_control": 2, "implementation_types_per_family": 3}:
        raise SystemExit("external evidence requirements changed without an architecture decision")
    record_keys: set[tuple[str, str, str, str]] = set()
    for record in development["records"]:
        required = {"control", "browser", "source", "implementation", "url", "outcome", "dispatch_status", "postcondition", "candidate_commit", "tested_at", "evidence_path"}
        if set(record) != required or record["control"] not in roles:
            raise SystemExit("invalid external evidence record")
        if record["browser"] not in expected_browsers or record["outcome"] != "verified" or record["postcondition"] != "verified":
            raise SystemExit("only verified Chrome/Edge records belong in the development index")
        key = (record["control"], record["browser"], record["source"], record["url"])
        if key in record_keys:
            raise SystemExit("duplicate external evidence record")
        record_keys.add(key)
    for role, evidence in development["controls"].items():
        for browser in expected_browsers:
            sources = {
                record["source"] for record in development["records"]
                if record["control"] == role and record["browser"] == browser
            }
            expected = "passed" if len(sources) >= requirements["independent_sources_per_control"] else "pending"
            if evidence["external"][browser] != expected:
                raise SystemExit(f"external summary for {role}/{browser} does not match traceable records")
    if external_cases.get("schema") != "saccade.external-cases/1":
        raise SystemExit("invalid external case manifest")
    case_ids = [case["id"] for case in external_cases.get("cases", [])]
    if len(case_ids) != len(set(case_ids)):
        raise SystemExit("duplicate external case id")
    return data, development, external_cases


def paired(evidence: dict) -> str:
    return f"{evidence['chrome']} / {evidence['edge']}"


def render(data: dict, development: dict, external_cases: dict) -> str:
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
        "External status requires two independent traceable public sources per control and browser.",
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
        "## Public case inventory",
        "",
        "| Control | Declared cases | Sources | Implementations |",
        "| --- | ---: | --- | --- |",
    ])
    for control in data["controls"]:
        cases = [case for case in external_cases["cases"] if case["control"] == control["role"]]
        sources = sorted({case["source"] for case in cases})
        implementations = sorted({case["implementation"] for case in cases})
        lines.append(
            f"| {control['id']} | {len(cases)} | {', '.join(sources) or 'gap'} | "
            f"{', '.join(implementations) or 'gap'} |"
        )
    lines.extend([
        "",
        "## Module details",
        "",
        "| Control | Family | Affordance | Input policy | Primitive | Verifier | Chrome | Edge | Status |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ])
    for control in data["controls"]:
        lines.append(
            "| {id} | {implementation_family} | {affordances} | {input_policy} | {native_primitive} | "
            "{verifier} | {chrome} | {edge} | {publication_status} |".format(
                id=control["id"],
                implementation_family=control["implementation_family"],
                affordances=", ".join(control["affordances"]),
                input_policy=control["input_policy"],
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
    data, development, external_cases = load_and_validate()
    OUTPUT.write_text(render(data, development, external_cases), encoding="utf-8")
    print(OUTPUT.relative_to(ROOT))


if __name__ == "__main__":
    main()
