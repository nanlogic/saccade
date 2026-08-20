#!/usr/bin/env python3
"""Static gate for the accepted single production route and wire versions."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def require(path: str, needle: str) -> None:
    text = (ROOT / path).read_text(encoding="utf-8")
    if needle not in text:
        raise SystemExit(f"{path}: missing {needle!r}")


def forbid(path: str, needle: str) -> None:
    text = (ROOT / path).read_text(encoding="utf-8")
    if needle in text:
        raise SystemExit(f"{path}: retained obsolete release route {needle!r}")


def main() -> None:
    require("crates/saccade_protocol/src/lib.rs", '"saccade.observation/1"')
    require("crates/saccade_protocol/src/lib.rs", '"saccade-extension-host/1"')
    for private_path in (
        ".authority",
        ".claude",
        ".codex",
        "AGENTS.md",
        "CLAUDE.md",
        "MEMORY.md",
        "PROJECT_AUTHORITY.md",
        "docs/current",
        "docs/migrations",
        "docs/proposals",
    ):
        if (ROOT / private_path).exists():
            raise SystemExit(f"public source tree retained local or historical path: {private_path}")
    public_reports = sorted(
        path.relative_to(ROOT).as_posix()
        for path in (ROOT / "docs/reports").glob("*.md")
    )
    if public_reports != [
        "docs/reports/2026-08-20-saccade-playwright-public-results.md"
    ]:
        raise SystemExit(f"public source tree retained stale reports: {public_reports}")
    require("docs/FINAL_ARCHITECTURE.md", "`npx -y @nanlogic/saccade`")
    require("docs/SETUP_TARGET.md", "Status: normative for the first public release.")
    require("docs/SETUP_TARGET.md", "`postinstall`")
    require("docs/SETUP_TARGET.md", "Cloud-only Agent sessions cannot reach")
    setup_package = json.loads(
        (ROOT / "packages/setup/package.json").read_text(encoding="utf-8")
    )
    if setup_package.get("name") != "@nanlogic/saccade":
        raise SystemExit("setup package lost its public npm name")
    if "postinstall" in setup_package.get("scripts", {}):
        raise SystemExit("setup package must not mutate the system from postinstall")
    setup_release = json.loads(
        (ROOT / "packages/setup/release.json").read_text(encoding="utf-8")
    )
    if setup_release.get("published") is not False:
        raise SystemExit("placeholder setup release must stay unpublished")
    if setup_release.get("native_host", {}).get("name") != "com.nanlogic.saccade":
        raise SystemExit("setup release lost the production Native Host identity")
    extension_candidate = json.loads(
        (ROOT / "extension/candidate.json").read_text(encoding="utf-8")
    )
    if setup_release.get("extension_candidate") != extension_candidate:
        raise SystemExit("setup release does not name the exact Extension candidate")
    require("packages/setup/src/setup.js", "exact Extension → Native Host → Runtime → MCP candidate")
    require("packages/setup/src/setup.js", "saccade.capabilities/6")
    require(".github/workflows/prepare-release.yml", "macos-15-intel")
    require(".github/workflows/prepare-release.yml", "sign_notarize_runtime.sh")
    require(".github/workflows/prepare-release.yml", "--draft")
    require(".github/workflows/publish-npm.yml", "id-token: write")
    require(".github/workflows/publish-npm.yml", "npm publish --access public --provenance")
    require("scripts/assemble_setup_release.py", "https://github.com/nanlogic/saccade/releases/download/")
    require("scripts/verify_published_setup_release.py", '"darwin-arm64", "darwin-x64"')
    require("scripts/package_extension_release.py", "store Extension manifest still has a development name")
    require(
        "docs/HOW_SACCADE_WORKS.md",
        "A closed-loop Saccade test must include the Agent-owned action",
    )
    require(
        "docs/RELEASE_PLAN.md",
        "Codex and Claude each complete the public MCP loop",
    )
    require(
        "docs/RELEASE_PLAN.md",
        "are not a release gate",
    )
    require("scripts/audit_public_evidence.py", "Reference Actuator reports are rejected")
    require("scripts/summarize_fair_matrix.py", "public_comparison_claims_authorized")
    for path, needle in (
        ("README.md", "scripts/package_preview_macos.py"),
        ("README.md", "unsigned DMG"),
        ("docs/RELEASE_PLAN.md", "signed DMG"),
        ("docs/RELEASE_PLAN.md", "Windows Setup"),
    ):
        forbid(path, needle)
    for needle in (
        "Agent-facing",
        "bounded filtering policy",
        "canonical control recognition",
        "action authority",
        "protected values",
    ):
        require("docs/FINAL_ARCHITECTURE.md", needle)
    profile_schema = json.loads(
        (ROOT / "catalog/profile.schema.json").read_text(encoding="utf-8")
    )
    if profile_schema.get("required") != ["name", "behavior", "ban"]:
        raise SystemExit("public Profile must require name, behavior, and ban")
    if set(profile_schema.get("properties", {})) != {"name", "behavior", "ban"}:
        raise SystemExit("public Profile gained an unapproved top-level field")
    default_profile = json.loads(
        (ROOT / "profiles/default.json").read_text(encoding="utf-8")
    )
    setup_profile = json.loads(
        (ROOT / "packages/setup/default-profile.json").read_text(encoding="utf-8")
    )
    if setup_profile != default_profile:
        raise SystemExit("setup package default Profile drifted from the product default")
    if set(default_profile) != {"name", "behavior", "ban"}:
        raise SystemExit("default Profile changed")
    if default_profile["name"] != "default" or default_profile["ban"] != []:
        raise SystemExit("default Profile identity or bans changed")
    behavior = default_profile["behavior"]
    if (
        not isinstance(behavior, str)
        or "autonomously" not in behavior
        or "Agent-owned tab is Agent On" not in behavior
        or "MCP adds no safety taxonomy or action gate" not in behavior
    ):
        raise SystemExit("default Profile must require autonomous Agent-owned access without an MCP safety gate")
    require("crates/saccade_runtime/src/profile.rs", "pub struct Profile")
    require("crates/saccade_runtime/src/profile.rs", "pub struct BanRule")
    require("crates/saccade_runtime/src/session.rs", '"saccade.capabilities/6"')
    require("crates/saccade_runtime/src/session.rs", '"product":"truth_layer"')
    require("crates/saccade_runtime/src/session.rs", '"execution_owner":"agent_client"')
    require("crates/saccade_runtime/src/session.rs", '"extension_candidate"')
    require("crates/saccade_runtime/src/session.rs", '"expected_extension_candidate"')
    require("crates/saccade_runtime/src/session.rs", "filter_observation")
    require("crates/saccade_runtime/src/session.rs", '"tabs.list"')
    require("crates/saccade_runtime/src/session.rs", '"tabs.open"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.list"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.close"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.open"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.truth.read"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.agent-view/1"')
    require("crates/saccade_runtime/src/mcp.rs", '"notifications/resources/updated"')
    require("crates/saccade_runtime/src/mcp.rs", "spawn_resource_watcher")
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.reference.form.fill"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.reference.reflex.run"')
    require("extension/src/service_worker.js", "com.nanlogic.saccade.dev")
    require("extension/src/service_worker.js", "com.nanlogic.saccade'")
    require("extension/src/service_worker.js", "getManifest().name.includes('(Development)')")
    require("extension/src/service_worker.js", "prepare_action")
    require("extension/src/service_worker.js", "saccade.collector")
    require("extension/src/service_worker.js", "reloadIfCandidateChanged")
    require("extension/src/service_worker.js", "sameCandidate(ping.extension_candidate)")
    require("extension/src/collector.js", "extension_candidate: globalThis.SaccadeCandidate")
    require("extension/src/collector.js", "registry.observe(role")
    require("extension/src/collector.js", "option_object_id")
    require("extension/src/collector.js", "!element.classList.contains('hit')")
    require("extension/src/collector.js", "SOFTWARE_CLICK_ROLES")
    require("scripts/dev.sh", "--load-extension")
    require("scripts/dev.sh", "dev_probe.py")
    require("scripts/dev.sh", "Microsoft Edge/NativeMessagingHosts")
    require("scripts/dev.sh", 'truth_test_route "${2:-chrome}"')
    require("scripts/dev.sh", 'test_route "${2:-chrome}"')
    require("scripts/dev_probe.py", '"browser": browser')
    extension_manifest = json.loads(
        (ROOT / "extension/manifest.json").read_text(encoding="utf-8")
    )
    if extension_manifest.get("manifest_version") != 3 or not extension_manifest.get("key"):
        raise SystemExit("production Extension lost its fixed Manifest V3 identity")
    if extension_manifest.get("name") != "Saccade":
        raise SystemExit("production Extension manifest must use the Saccade name")
    if "scripting" in extension_manifest.get("permissions", []):
        raise SystemExit("production Extension must not programmatically inject the compiler")
    content_scripts = extension_manifest.get("content_scripts", [])
    if len(content_scripts) != 1 or content_scripts[0].get("js", [])[-1:] != ["src/collector.js"]:
        raise SystemExit("Extension lost its ordered static Collector bundle")
    runtime = (ROOT / "bins/saccade-runtime/src/main.rs").read_text(encoding="utf-8")
    for mode in (
        "native-host",
        "mcp",
        "doctor",
        "reference-actuator-mcp",
        "reference-actuator-repair",
    ):
        if f'"{mode}"' not in runtime:
            raise SystemExit(f"runtime mode missing: {mode}")
    if 'Some("repair")' in runtime:
        raise SystemExit("default Runtime retained the legacy Accessibility repair command")
    manifests = "\n".join(path.read_text(encoding="utf-8") for path in ROOT.rglob("Cargo.toml"))
    for forbidden in ("playwright", "cef", "servo", "chromiumoxide", "headless_chrome"):
        if forbidden in manifests.lower():
            raise SystemExit(f"forbidden production dependency: {forbidden}")
    reference = (ROOT / "tests/reference/playwright/README.md").read_text(encoding="utf-8")
    if "not a Saccade action route" not in reference or "cannot" not in reference:
        raise SystemExit("Playwright reference harness lost its non-production boundary")
    catalog = json.loads((ROOT / "catalog/controls.json").read_text(encoding="utf-8"))
    expected_roles = {
        "button", "text_field", "search_field", "text_area", "content_editable",
        "spin_button", "checkbox", "radio", "switch", "select", "option", "tab", "menu_item",
        "reflex_target", "link", "file_input",
    }
    if {item["role"] for item in catalog["controls"]} != expected_roles:
        raise SystemExit("Catalog roles changed outside the implemented control batches")
    catalog_schema = json.loads(
        (ROOT / "catalog/control_catalog.schema.json").read_text(encoding="utf-8")
    )
    item_properties = catalog_schema["properties"]["controls"]["items"]["properties"]
    if set(item_properties["role"]["enum"]) != expected_roles:
        raise SystemExit("Control Catalog schema roles do not match implemented roles")
    for forbidden in ("native_primitive", "input_policy", "verifier", "secondary_actions"):
        if forbidden in item_properties or any(forbidden in item for item in catalog["controls"]):
            raise SystemExit(f"Truth Catalog contains reference-actuator field: {forbidden}")
    if any(item["publication_status"] != "implementation" for item in catalog["controls"]):
        raise SystemExit("local development evidence cannot publish a Catalog row")
    inventory = json.loads((ROOT / "catalog/truth_inventory.json").read_text(encoding="utf-8"))
    inventory_roles = {item["role"] for item in inventory["roles"]}
    protocol_roles = {
        "text", "heading", "paragraph", "list", "list_item", "table", "row", "cell",
        "alert", "status", "button", "link", "text_field", "search_field", "text_area",
        "content_editable", "checkbox", "radio", "switch", "select", "option",
        "file_input", "slider", "spin_button", "tab", "menu_item", "label",
        "generic_control", "reflex_target", "image", "frame", "opaque_surface",
        "restricted_document", "unknown",
    }
    if inventory_roles != protocol_roles:
        raise SystemExit("Truth inventory does not account for every protocol semantic role")
    if {item["role"] for item in inventory["roles"] if item.get("gate") == "control"} != expected_roles:
        raise SystemExit("Truth inventory control gate does not match the Catalog")
    if {item["role"] for item in inventory["roles"] if item.get("gate") == "semantic"} != {
        "text", "heading", "paragraph", "list", "list_item", "table", "row", "cell",
        "alert", "status", "slider", "label", "generic_control", "image",
        "opaque_surface", "restricted_document",
    }:
        raise SystemExit("Truth inventory semantic gate changed without runner coverage")
    if {item["role"] for item in inventory["roles"] if item.get("gate") == "negative"} != {"unknown"}:
        raise SystemExit("reserved Truth roles lost their negative gate")
    print("single architecture gate: ok")


if __name__ == "__main__":
    main()
