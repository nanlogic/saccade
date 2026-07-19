#!/usr/bin/env python3
"""Verify Saccade and third-party licensing in a signed macOS release kit."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import plistlib
import struct
import subprocess
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--expected-build", required=True)
    return parser.parse_args()


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_checksums(package: pathlib.Path) -> tuple[bool, int]:
    checksum_file = package / "SHA256SUMS"
    checked = 0
    for line in checksum_file.read_text().splitlines():
        expected, relative = line.split("  ", 1)
        path = package / relative.removeprefix("./")
        if not path.is_file() or sha256(path) != expected:
            return False, checked
        checked += 1
    return checked > 0, checked


def png_size(path: pathlib.Path) -> tuple[int, int] | None:
    if not path.is_file():
        return None
    header = path.read_bytes()[:24]
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack(">II", header[16:24])


def main() -> int:
    args = parse_args()
    package = args.package.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    app = package / "Saccade.app"
    resources = app / "Contents" / "Resources" / "Saccade"
    licenses = package / "licenses"
    version_path = package / "VERSION.json"
    inventory_path = licenses / "INVENTORY.json"
    plist_path = app / "Contents" / "Info.plist"

    version: dict[str, Any] = json.loads(version_path.read_text())
    inventory: dict[str, Any] = json.loads(inventory_path.read_text())
    with plist_path.open("rb") as source:
        plist = plistlib.load(source)
    localized_plist_path = (
        app / "Contents" / "Resources" / "English.lproj" / "InfoPlist.strings"
    )
    localized_plist_text = localized_plist_path.read_text()

    signature = subprocess.run(
        ["codesign", "-dvvv", str(app)], capture_output=True, text=True
    )
    signature_text = signature.stdout + signature.stderr
    strict = subprocess.run(
        ["codesign", "--verify", "--strict", "--verbose=2", str(app)],
        capture_output=True,
        text=True,
    )
    checksums_ok, checksum_count = verify_checksums(package)

    paired_files = [
        (ROOT / "LICENSE", licenses / "SACCADE_LICENSE.txt"),
        (ROOT / "NOTICE", licenses / "SACCADE_NOTICE.txt"),
        (ROOT / "TRADEMARKS.md", licenses / "SACCADE_TRADEMARKS.md"),
        (ROOT / "LICENSE", resources / "SACCADE_LICENSE.txt"),
        (ROOT / "NOTICE", resources / "SACCADE_NOTICE.txt"),
        (ROOT / "TRADEMARKS.md", resources / "SACCADE_TRADEMARKS.md"),
    ]
    identical_license_files = all(
        left.is_file() and right.is_file() and left.read_bytes() == right.read_bytes()
        for left, right in paired_files
    )
    required_files = [
        licenses / "CEF_LICENSE.txt",
        licenses / "CHROMIUM_CREDITS.html",
        licenses / "SACCADE_LICENSE.txt",
        licenses / "SACCADE_NOTICE.txt",
        licenses / "SACCADE_TRADEMARKS.md",
        licenses / "SBOM.cdx.json",
        package / "docs" / "public_release_licensing.md",
    ]
    no_placeholder = "license-decision-required" not in inventory_path.read_text()
    team = str(version.get("codesign_team", ""))
    bundle = str(version.get("bundle_identifier", ""))
    tab_icon = app / "Contents" / "Resources" / "Saccade-tab.png"

    checks = {
        "expected_app_build": str(version.get("app_build")) == args.expected_build
        and str(plist.get("CFBundleVersion")) == args.expected_build,
        "saccade_apache_2_0": version.get("source_license") == "Apache-2.0"
        and inventory.get("saccade", {}).get("license") == "Apache-2.0",
        "license_files_identical": identical_license_files,
        "required_license_files_present": all(path.is_file() for path in required_files),
        "license_placeholder_removed": no_placeholder,
        "publisher_metadata_present": version.get("publisher_name")
        == "NaN Logic LLC"
        and version.get("publisher_url") == "https://nanlogic.com/"
        and inventory.get("saccade", {}).get("copyright_owner")
        == "NaN Logic LLC"
        and plist.get("SaccadePublisherName") == "NaN Logic LLC"
        and plist.get("SaccadePublisherURL") == "https://nanlogic.com/",
        "help_metadata_present": version.get("help_url")
        == "https://nanlogic.com/"
        and plist.get("SaccadeHelpURL") == "https://nanlogic.com/",
        "copyright_present": "NaN Logic LLC"
        in str(plist.get("NSHumanReadableCopyright", ""))
        and "NaN Logic LLC"
        in localized_plist_text,
        "official_identity_matches_signature": bundle == "ai.saccade.browser"
        and f"Identifier={bundle}" in signature_text
        and bool(team)
        and f"TeamIdentifier={team}" in signature_text,
        "strict_codesign_valid": strict.returncode == 0,
        "hardened_runtime_enabled": version.get("hardened_runtime") is True
        and "runtime" in signature_text,
        "secure_timestamp_present": version.get("secure_timestamp") is True
        and "Timestamp=" in signature_text,
        "saccade_fallback_favicon_present": version.get(
            "browser_fallback_favicon"
        )
        == "Saccade-tab.png"
        and png_size(tab_icon) == (64, 64),
        "checksums_valid": checksums_ok,
    }
    report = {
        "schema": "saccade-release-license-gate-v1",
        "package": str(package),
        "app_version": version.get("app_version"),
        "app_build": version.get("app_build"),
        "source_license": version.get("source_license"),
        "bundle_identifier": bundle,
        "codesign_team": team,
        "checksum_files_verified": checksum_count,
        "checks": checks,
        "verdict": "PASS" if all(checks.values()) else "FAIL",
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"CEF_RELEASE_LICENSE verdict={report['verdict']} "
        f"report={output / 'report.json'}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
