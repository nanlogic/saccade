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


def main() -> None:
    require("crates/saccade_protocol/src/lib.rs", '"saccade.observation/1"')
    require("crates/saccade_protocol/src/lib.rs", '"saccade-extension-host/1"')
    require("AGENTS.md", "docs/PROFILE_ARCHITECTURE.md")
    require("README.md", "docs/PROFILE_ARCHITECTURE.md")
    require("docs/FINAL_ARCHITECTURE.md", "`PROFILE_ARCHITECTURE.md`")
    require("docs/extension_observation_contract.md", "`PROFILE_ARCHITECTURE.md`")
    require(
        "docs/decisions.md",
        "Profiles provide behavior and ban named controls",
    )
    for needle in (
        '"name"',
        '"behavior"',
        '"ban"',
        '"control"',
        '"condition"',
        "saccade.observation/1",
        "saccade-extension-host/1",
    ):
        require("docs/PROFILE_ARCHITECTURE.md", needle)
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
    if default_profile != {"name": "default", "behavior": "", "ban": []}:
        raise SystemExit("default Profile changed")
    require("crates/saccade_runtime/src/profile.rs", "pub struct Profile")
    require("crates/saccade_runtime/src/profile.rs", "pub struct BanRule")
    require("crates/saccade_runtime/src/session.rs", '"saccade.capabilities/5"')
    require("crates/saccade_runtime/src/session.rs", '"product":"truth_layer"')
    require("crates/saccade_runtime/src/session.rs", '"execution_owner":"agent_client"')
    require("crates/saccade_runtime/src/session.rs", '"browser_owned_confirm"')
    require("crates/saccade_runtime/src/session.rs", "filter_observation")
    require("crates/saccade_runtime/src/session.rs", '"tabs.list"')
    require("crates/saccade_runtime/src/session.rs", '"tabs.open"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.list"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.open"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.truth.read"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.agent-view/1"')
    require("crates/saccade_runtime/src/mcp.rs", '"notifications/resources/updated"')
    require("crates/saccade_runtime/src/mcp.rs", "spawn_resource_watcher")
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.reference.form.fill"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.reference.reflex.run"')
    require("extension/src/service_worker.js", "com.nanlogic.saccade.dev")
    require("extension/src/service_worker.js", "prepare_action")
    require("extension/src/service_worker.js", "saccade.collector")
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
        raise SystemExit("development Extension lost its fixed Manifest V3 identity")
    if "scripting" in extension_manifest.get("permissions", []):
        raise SystemExit("production Extension must not programmatically inject the compiler")
    content_scripts = extension_manifest.get("content_scripts", [])
    if len(content_scripts) != 1 or content_scripts[0].get("js", [])[-1:] != ["src/collector.js"]:
        raise SystemExit("Extension lost its ordered static Collector bundle")
    runtime = (ROOT / "bins/saccade-runtime/src/main.rs").read_text(encoding="utf-8")
    for mode in ("native-host", "mcp", "reference-actuator-mcp", "doctor", "repair"):
        if f'"{mode}"' not in runtime:
            raise SystemExit(f"runtime mode missing: {mode}")
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
        "spin_button", "checkbox", "radio", "switch", "select", "tab", "menu_item",
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
        "alert", "status", "option", "slider", "label", "generic_control", "image",
        "opaque_surface", "restricted_document",
    }:
        raise SystemExit("Truth inventory semantic gate changed without runner coverage")
    if {item["role"] for item in inventory["roles"] if item.get("gate") == "negative"} != {"unknown"}:
        raise SystemExit("reserved Truth roles lost their negative gate")
    print("single architecture gate: ok")


if __name__ == "__main__":
    main()
