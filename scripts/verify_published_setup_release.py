#!/usr/bin/env python3
"""Verify the exact setup manifest consumed by an npm publication."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REQUIRED_PLATFORMS = {"darwin-arm64", "darwin-x64"}


def verify(release_path: Path, tag: str, artifact_dir: Path | None = None) -> None:
    release = json.loads(release_path.read_text(encoding="utf-8"))
    package = json.loads((ROOT / "packages/setup/package.json").read_text(encoding="utf-8"))
    candidate = json.loads((ROOT / "extension/candidate.json").read_text(encoding="utf-8"))
    version = package["version"]
    if tag != f"v{version}" or release.get("version") != version:
        raise ValueError("release tag, setup package, and manifest versions differ")
    if release.get("published") is not True:
        raise ValueError("npm publication requires a published setup manifest")
    if release.get("extension_candidate") != candidate:
        raise ValueError("setup manifest does not name the source Extension candidate")
    publisher = release.get("publisher") or {}
    if publisher.get("organization") != "Nanlogic" or publisher.get("repository") != "https://github.com/nanlogic/saccade":
        raise ValueError("setup manifest is not owned by Nanlogic")
    origins = (release.get("native_host") or {}).get("allowed_origins") or []
    if not origins or any(not re.fullmatch(r"chrome-extension://[a-p]{32}/", item) for item in origins):
        raise ValueError("setup manifest has no valid store Extension origin")
    artifacts = release.get("artifacts") or {}
    if set(artifacts) != REQUIRED_PLATFORMS:
        raise ValueError("setup manifest must contain both macOS architectures")
    prefix = f"https://github.com/nanlogic/saccade/releases/download/{tag}/"
    for platform, artifact in artifacts.items():
        if artifact.get("signed") is not True:
            raise ValueError(f"{platform} Runtime is not signed")
        if not str(artifact.get("url", "")).startswith(prefix):
            raise ValueError(f"{platform} Runtime URL is outside the tagged Nanlogic release")
        if not re.fullmatch(r"[0-9a-f]{64}", str(artifact.get("sha256", ""))):
            raise ValueError(f"{platform} Runtime checksum is invalid")
        if artifact_dir is not None:
            filename = Path(artifact["url"]).name
            local_artifact = artifact_dir / filename
            if not local_artifact.is_file():
                raise ValueError(f"{platform} Runtime artifact is missing")
            actual = hashlib.sha256(local_artifact.read_bytes()).hexdigest()
            if actual != artifact["sha256"]:
                raise ValueError(f"{platform} Runtime artifact checksum differs")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--artifact-dir", type=Path)
    args = parser.parse_args()
    verify(
        args.release.resolve(),
        args.tag,
        args.artifact_dir.resolve() if args.artifact_dir else None,
    )


if __name__ == "__main__":
    main()
