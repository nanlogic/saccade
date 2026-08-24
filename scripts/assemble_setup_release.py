#!/usr/bin/env python3
"""Assemble signed per-platform Runtime drafts into one setup release manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path


REQUIRED_PLATFORMS = {"darwin-arm64", "darwin-x64"}


def assemble(
    drafts: list[Path],
    output_dir: Path,
    *,
    base_url: str,
    allowed_origins: list[str],
) -> dict[str, Path]:
    if not base_url.startswith("https://github.com/nanlogic/saccade/releases/download/"):
        raise ValueError("Runtime base URL must be a nanlogic/saccade GitHub Release")
    if not allowed_origins or any(
        not origin.startswith("chrome-extension://") or not origin.endswith("/")
        for origin in allowed_origins
    ):
        raise ValueError("at least one Chrome/Edge Extension origin is required")

    manifests = [json.loads(path.read_text(encoding="utf-8")) for path in drafts]
    if not manifests:
        raise ValueError("at least one Runtime draft is required")
    identity_fields = ("schema", "version", "mcp_contract_hash", "extension_candidate")
    reference = {field: manifests[0].get(field) for field in identity_fields}
    artifacts: dict[str, dict[str, object]] = {}
    output_dir.mkdir(parents=True, exist_ok=True)

    for draft_path, manifest in zip(drafts, manifests, strict=True):
        if {field: manifest.get(field) for field in identity_fields} != reference:
            raise ValueError("Runtime drafts do not share one release identity")
        draft_artifacts = manifest.get("artifacts") or {}
        if len(draft_artifacts) != 1:
            raise ValueError("each Runtime draft must contain exactly one artifact")
        platform, artifact = next(iter(draft_artifacts.items()))
        if platform in artifacts:
            raise ValueError(f"duplicate Runtime platform {platform}")
        if artifact.get("signed") is not True:
            raise ValueError(f"Runtime draft {platform} is not signed")
        filename = artifact.get("local_file")
        source = draft_path.parent / str(filename)
        if not filename or not source.is_file():
            raise ValueError(f"Runtime artifact is missing for {platform}")
        actual_sha256 = hashlib.sha256(source.read_bytes()).hexdigest()
        if actual_sha256 != artifact.get("sha256"):
            raise ValueError(f"Runtime artifact checksum changed for {platform}")
        shutil.copyfile(source, output_dir / filename)
        artifacts[platform] = {
            "url": f"{base_url.rstrip('/')}/{filename}",
            "sha256": artifact["sha256"],
            "signed": True,
        }

    if set(artifacts) != REQUIRED_PLATFORMS:
        missing = ", ".join(sorted(REQUIRED_PLATFORMS - set(artifacts)))
        raise ValueError(f"release is missing required Runtime platforms: {missing}")

    release = {
        **reference,
        "published": True,
        "publisher": {
            "organization": "Nanlogic",
            "repository": "https://github.com/nanlogic/saccade",
        },
        "native_host": {
            "name": "com.nanlogic.saccade",
            "allowed_origins": allowed_origins,
        },
        "artifacts": artifacts,
    }
    manifest_path = output_dir / "release.json"
    manifest_path.write_text(json.dumps(release, indent=2) + "\n", encoding="utf-8")
    sums_path = output_dir / "SHA256SUMS"
    sums_path.write_text(
        "".join(
            f"{artifact['sha256']}  {Path(artifact['url']).name}\n"
            for _, artifact in sorted(artifacts.items())
        ),
        encoding="utf-8",
    )
    return {"manifest": manifest_path, "checksums": sums_path}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--draft", action="append", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--allowed-origin", action="append", required=True)
    args = parser.parse_args()
    result = assemble(
        [path.resolve() for path in args.draft],
        args.output.resolve(),
        base_url=args.base_url,
        allowed_origins=args.allowed_origin,
    )
    print(json.dumps({key: str(value) for key, value in result.items()}))


if __name__ == "__main__":
    main()
