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
    require("crates/saccade_runtime/src/session.rs", '"saccade.capabilities/4"')
    require("crates/saccade_runtime/src/session.rs", '"native_accessibility_trusted"')
    require("crates/saccade_runtime/src/session.rs", "filter_observation")
    require("crates/saccade_runtime/src/session.rs", '"tabs.list"')
    require("crates/saccade_runtime/src/session.rs", '"tabs.open"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.list"')
    require("crates/saccade_runtime/src/mcp.rs", '"saccade.tabs.open"')
    require("extension/src/service_worker.js", "com.nanlogic.saccade.dev")
    require("extension/src/service_worker.js", "prepare_action")
    require("extension/src/collector.js", "registry.observe(role")
    require("extension/src/collector.js", "option_object_id")
    require("scripts/dev.sh", "--load-extension")
    require("scripts/dev.sh", "dev_probe.py")
    extension_manifest = json.loads(
        (ROOT / "extension/manifest.json").read_text(encoding="utf-8")
    )
    if extension_manifest.get("manifest_version") != 3 or not extension_manifest.get("key"):
        raise SystemExit("development Extension lost its fixed Manifest V3 identity")
    runtime = (ROOT / "bins/saccade-runtime/src/main.rs").read_text(encoding="utf-8")
    for mode in ("native-host", "mcp", "doctor", "repair"):
        if f'"{mode}"' not in runtime:
            raise SystemExit(f"runtime mode missing: {mode}")
    manifests = "\n".join(path.read_text(encoding="utf-8") for path in ROOT.rglob("Cargo.toml"))
    for forbidden in ("playwright", "cef", "servo", "chromiumoxide", "headless_chrome"):
        if forbidden in manifests.lower():
            raise SystemExit(f"forbidden production dependency: {forbidden}")
    catalog = json.loads((ROOT / "catalog/controls.json").read_text(encoding="utf-8"))
    if {item["role"] for item in catalog["controls"]} != {"button", "text_field", "checkbox", "select"}:
        raise SystemExit("first-slice Catalog roles changed")
    if any(item["publication_status"] != "implementation" for item in catalog["controls"]):
        raise SystemExit("local development evidence cannot publish a Catalog row")
    print("single architecture gate: ok")


if __name__ == "__main__":
    main()
