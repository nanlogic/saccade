#!/usr/bin/env python3
"""Package an exact, production-named Extension candidate for store review."""

from __future__ import annotations

import argparse
import json
import tempfile
import zipfile
from pathlib import Path

try:
    from scripts.extension_candidate import candidate_id
except ModuleNotFoundError:  # Direct `python scripts/...` execution.
    from extension_candidate import candidate_id

STORE_EXCLUDED_PREFIXES = ("tests/",)
ZIP_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


def include_in_store(relative: str) -> bool:
    return not relative.startswith(STORE_EXCLUDED_PREFIXES)


def store_manifest(extension_root: Path) -> bytes:
    manifest = json.loads((extension_root / "manifest.json").read_text(encoding="utf-8"))
    # Chrome Web Store assigns the installed Extension identity and rejects a
    # developer-supplied key. The source key remains available to keep the
    # unpacked development Extension identity stable.
    manifest.pop("key", None)
    return (json.dumps(manifest, indent=2) + "\n").encode("utf-8")


def archive_info(relative: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(relative, date_time=ZIP_TIMESTAMP)
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


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
                if include_in_store(relative):
                    contents = store_manifest(extension_root) if relative == "manifest.json" else path.read_bytes()
                    archive.writestr(
                        archive_info(relative),
                        contents,
                        compress_type=zipfile.ZIP_DEFLATED,
                        compresslevel=9,
                    )
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
