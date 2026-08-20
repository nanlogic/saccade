#!/usr/bin/env python3
"""Package an exact, production-named Extension candidate for store review."""

from __future__ import annotations

import argparse
import hashlib
import json
import tempfile
import zipfile
from pathlib import Path


IDENTITY_FILES = {"candidate.json", "src/candidate_identity.js"}


def is_test_file(relative: str) -> bool:
    return relative == "tests" or relative.startswith("tests/")


def candidate_id(extension_root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(item for item in extension_root.rglob("*") if item.is_file()):
        relative = path.relative_to(extension_root).as_posix()
        if relative in IDENTITY_FILES or is_test_file(relative):
            continue
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def package(extension_root: Path, output_dir: Path) -> Path:
    manifest = json.loads((extension_root / "manifest.json").read_text(encoding="utf-8"))
    candidate = json.loads((extension_root / "candidate.json").read_text(encoding="utf-8"))
    if "development" in str(manifest.get("name", "")).casefold():
        raise ValueError("store Extension manifest still has a development name")
    expected = {
        "schema": "saccade.extension-candidate/1",
        "id": candidate_id(extension_root),
        "version": manifest.get("version"),
    }
    if candidate != expected:
        raise ValueError("Extension candidate identity is stale")
    output_dir.mkdir(parents=True, exist_ok=True)
    target = output_dir / f"saccade-extension-{candidate['version']}.zip"
    with tempfile.NamedTemporaryFile(dir=output_dir, delete=False) as temporary:
        temporary_path = Path(temporary.name)
    try:
        with zipfile.ZipFile(temporary_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(item for item in extension_root.rglob("*") if item.is_file()):
                relative = path.relative_to(extension_root).as_posix()
                if is_test_file(relative):
                    continue
                info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                archive.writestr(info, path.read_bytes())
        temporary_path.replace(target)
    finally:
        temporary_path.unlink(missing_ok=True)
    return target


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extension-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    print(package(args.extension_root.resolve(), args.output.resolve()))


if __name__ == "__main__":
    main()
