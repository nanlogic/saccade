#!/usr/bin/env python3
"""Package an unsigned Windows candidate for an explicit local-machine test."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLATFORM = "win32-x64"
EXTENSION_EXCLUDES = {"candidate.json"}
EXTENSION_EXCLUDED_PREFIXES = ("tests/",)
SETUP_ITEMS = (
    "bin",
    "src",
    "README.md",
    "default-profile.json",
    "package.json",
    "release.json",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_extension(output: Path) -> None:
    source = ROOT / "extension"
    target = output / "extension"
    for path in sorted(item for item in source.rglob("*") if item.is_file()):
        relative = path.relative_to(source).as_posix()
        if relative in EXTENSION_EXCLUDES or relative.startswith(EXTENSION_EXCLUDED_PREFIXES):
            continue
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if relative == "manifest.json":
            manifest = json.loads(path.read_text(encoding="utf-8"))
            manifest.pop("key", None)
            destination.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        else:
            shutil.copyfile(path, destination)


def copy_setup(output: Path) -> None:
    source = ROOT / "packages/setup"
    target = output / "package"
    for item in SETUP_ITEMS:
        path = source / item
        destination = target / item
        if path.is_dir():
            shutil.copytree(path, destination)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)


def package(draft_path: Path, output: Path) -> dict[str, Path]:
    draft = json.loads(draft_path.read_text(encoding="utf-8"))
    artifacts = draft.get("artifacts") or {}
    if set(artifacts) != {PLATFORM}:
        raise ValueError("candidate draft must contain exactly one win32-x64 artifact")
    artifact = artifacts[PLATFORM]
    source_runtime = draft_path.parent / str(artifact.get("local_file", ""))
    if not source_runtime.is_file() or sha256(source_runtime) != artifact.get("sha256"):
        raise ValueError("Windows candidate Runtime is missing or its checksum changed")
    if draft.get("published") is not False or artifact.get("signed") is not False:
        raise ValueError("Windows candidate must remain an unpublished unsigned test build")

    if output.exists():
        raise ValueError(f"candidate output already exists: {output}")
    output.mkdir(parents=True)
    runtime = output / "runtime" / "saccade-runtime.exe"
    runtime.parent.mkdir()
    shutil.copyfile(source_runtime, runtime)
    copy_extension(output)
    copy_setup(output)

    release = {
        **{key: draft[key] for key in (
            "schema", "version", "mcp_contract_hash", "extension_candidate"
        )},
        "published": True,
        "candidate_only": True,
        "native_host": {
            "name": "com.nanlogic.saccade",
            "allowed_origins": [],
        },
        "artifacts": {
            PLATFORM: {
                "url": None,
                "sha256": sha256(runtime),
                "signed": False,
            }
        },
    }
    template = output / "release-template.json"
    template.write_text(json.dumps(release, indent=2) + "\n", encoding="utf-8")
    shutil.copyfile(ROOT / "scripts/windows_candidate/install.ps1", output / "install.ps1")
    shutil.copyfile(ROOT / "scripts/windows_candidate/README.md", output / "README.md")
    checksums = output / "SHA256SUMS"
    checksums.write_text(f"{sha256(runtime)}  runtime/saccade-runtime.exe\n", encoding="utf-8")
    return {"output": output, "runtime": runtime, "release": template}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--draft", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = package(args.draft.resolve(), args.output.resolve())
    print(json.dumps({key: str(value) for key, value in result.items()}))


if __name__ == "__main__":
    main()
