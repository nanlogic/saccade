#!/usr/bin/env python3
"""Generate a deterministic CycloneDX SBOM for a Saccade dogfood package."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import subprocess
import urllib.parse
import uuid
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=pathlib.Path, required=True)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--target", default="aarch64-apple-darwin")
    return parser.parse_args()


def cargo_metadata(target: str) -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


def package_ref(package: dict[str, Any], workspace_ids: set[str]) -> str:
    name = urllib.parse.quote(str(package["name"]), safe="-._~")
    version = urllib.parse.quote(str(package["version"]), safe="-._~+")
    base = f"pkg:cargo/{name}@{version}"
    if package["id"] in workspace_ids:
        return base + "?workspace=true"
    source = str(package.get("source") or "")
    if source.startswith("registry+"):
        return base
    if source:
        digest = hashlib.sha256(source.encode()).hexdigest()[:12]
        return base + f"?source_hash={digest}"
    return base + "?vendored=true"


def component(package: dict[str, Any], bom_ref: str) -> dict[str, Any]:
    package_type = (
        "application"
        if any("bin" in target.get("kind", []) for target in package.get("targets", []))
        else "library"
    )
    item: dict[str, Any] = {
        "type": package_type,
        "bom-ref": bom_ref,
        "name": package["name"],
        "version": package["version"],
        "purl": bom_ref.split("?", 1)[0],
    }
    license_expression = package.get("license")
    if license_expression:
        item["licenses"] = [{"expression": license_expression}]
    description = package.get("description")
    if description:
        item["description"] = description
    repository = package.get("repository")
    if repository:
        item["externalReferences"] = [
            {"type": "vcs", "url": repository}
        ]
    return item


def main() -> int:
    args = parse_args()
    package_dir = args.package.resolve()
    output = args.output.resolve()
    version = json.loads((package_dir / "VERSION.json").read_text())
    cef_lock = json.loads((ROOT / "engines" / "cef" / "cef.lock.json").read_text())
    metadata = cargo_metadata(args.target)
    workspace_ids = set(metadata["workspace_members"])
    packages = sorted(
        metadata["packages"], key=lambda item: (item["name"], item["version"], item["id"])
    )
    invalid_workspace_licenses = [
        item["name"]
        for item in packages
        if item["id"] in workspace_ids and item.get("license") != "Apache-2.0"
    ]
    if invalid_workspace_licenses:
        raise RuntimeError(
            "workspace packages missing Apache-2.0 metadata: "
            + ", ".join(invalid_workspace_licenses)
        )
    refs = {item["id"]: package_ref(item, workspace_ids) for item in packages}
    components = [component(item, refs[item["id"]]) for item in packages]
    components.extend(
        [
            {
                "type": "framework",
                "bom-ref": f"pkg:generic/cef@{cef_lock['cef_version']}",
                "name": "Chromium Embedded Framework",
                "version": cef_lock["cef_version"],
                "licenses": [{"license": {"id": "BSD-3-Clause"}}],
                "properties": [
                    {"name": "saccade:license-file", "value": "licenses/CEF_LICENSE.txt"}
                ],
            },
            {
                "type": "framework",
                "bom-ref": f"pkg:generic/chromium@{cef_lock['chromium_version']}",
                "name": "Chromium",
                "version": cef_lock["chromium_version"],
                "properties": [
                    {
                        "name": "saccade:credits-file",
                        "value": "licenses/CHROMIUM_CREDITS.html",
                    }
                ],
            },
        ]
    )
    dependencies = []
    for node in metadata["resolve"]["nodes"]:
        if node["id"] not in refs:
            continue
        dependencies.append(
            {
                "ref": refs[node["id"]],
                "dependsOn": sorted(
                    refs[item["pkg"]]
                    for item in node.get("deps", [])
                    if item["pkg"] in refs
                ),
            }
        )
    dependencies.extend(
        [
            {
                "ref": f"pkg:generic/cef@{cef_lock['cef_version']}",
                "dependsOn": [
                    f"pkg:generic/chromium@{cef_lock['chromium_version']}"
                ],
            },
            {
                "ref": f"pkg:generic/chromium@{cef_lock['chromium_version']}",
                "dependsOn": [],
            },
        ]
    )
    serial_seed = "|".join(
        [
            str(version["source_commit"]),
            str(version["app_version"]),
            str(version["app_build"]),
            str(cef_lock["cef_version"]),
        ]
    )
    document = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, serial_seed)}",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "bom-ref": f"pkg:generic/saccade@{version['app_version']}?build={version['app_build']}",
                "name": "Saccade",
                "version": f"{version['app_version']}+{version['app_build']}",
                "manufacturer": {"name": "NaN Logic LLC"},
                "licenses": [{"license": {"id": "Apache-2.0"}}],
                "externalReferences": [
                    {"type": "website", "url": "https://nanlogic.com/"}
                ],
            },
            "properties": [
                {"name": "saccade:source-description", "value": version["source_description"]},
                {"name": "saccade:source-dirty", "value": str(version["source_dirty"]).lower()},
            ],
        },
        "components": sorted(components, key=lambda item: item["bom-ref"]),
        "dependencies": sorted(dependencies, key=lambda item: item["ref"]),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
    print(f"SBOM components={len(components)} output={output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
