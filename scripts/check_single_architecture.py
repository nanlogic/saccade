#!/usr/bin/env python3
"""Fail when production files drift from the Node-only Saccade route."""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require(path: str, needle: str) -> None:
    if needle not in text(path):
        raise SystemExit(f"{path}: missing {needle!r}")


def forbid_tree(needles: tuple[str, ...]) -> None:
    production = [
        ROOT / "packages",
        ROOT / "extension",
        ROOT / ".github" / "workflows",
    ]
    for base in production:
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix in {".png", ".zip"}:
                continue
            if "test" in path.parts or "tests" in path.parts:
                continue
            value = path.read_text(encoding="utf-8", errors="ignore").lower()
            for needle in needles:
                if needle.lower() in value:
                    raise SystemExit(f"{path.relative_to(ROOT)}: obsolete production route {needle!r}")


def main() -> None:
    if (ROOT / "Cargo.toml").exists() or (ROOT / "Cargo.lock").exists():
        raise SystemExit("Cargo workspace must not exist")
    if any((ROOT / "crates").rglob("*.rs")) or any((ROOT / "bins").rglob("*.rs")):
        raise SystemExit("Rust source remains in the production tree")

    package = json.loads(text("packages/setup/package.json"))
    if package.get("name") != "@nanlogic/saccade" or package.get("version") != "0.2.1":
        raise SystemExit("Node package identity drifted")
    if set(package.get("bin", {})) != {"saccade", "saccade-setup"}:
        raise SystemExit("Node package must publish only the supported CLI aliases")
    if "postinstall" in package.get("scripts", {}):
        raise SystemExit("npm installation must have no mutation hook")

    manifest = json.loads(text("extension/manifest.json"))
    if "nativeMessaging" in manifest.get("permissions", []):
        raise SystemExit("Extension retained nativeMessaging")
    if "http://127.0.0.1:32177/*" not in manifest.get("host_permissions", []):
        raise SystemExit("Extension lost the loopback Node Broker permission")

    require("packages/setup/src/broker.js", "saccade.node-broker/1")
    require("packages/setup/src/broker.js", "OUTCOME_UNKNOWN")
    require("packages/setup/src/broker.js", "TAB_ALREADY_LEASED")
    require("packages/setup/src/mcp.js", "saccade.truth.read")
    require("extension/src/service_worker.js", "http://127.0.0.1:32177")
    require("docs/current/product-execution-boundary.md", "loopback Node Broker")
    forbid_tree((
        "connectNative", "nativeMessaging", "cargo build", "cargo test",
        "saccade-runtime", "Native Messaging Host", "codesign", "notarize",
        "install_windows_from_source", "platform_input",
    ))

    tools = text("packages/setup/src/mcp.js")
    for forbidden in ("playwright", "page.evaluate", "xpath", "selector"):
        if forbidden in tools.lower():
            raise SystemExit(f"MCP exposed forbidden route {forbidden!r}")


if __name__ == "__main__":
    main()
