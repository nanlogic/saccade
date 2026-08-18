#!/usr/bin/env python3
"""Stamp an installed unpacked Extension with a reproducible candidate identity."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


EXCLUDED = {"candidate.json", "src/candidate_identity.js"}


def candidate_id(extension_root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(item for item in extension_root.rglob("*") if item.is_file()):
        relative = path.relative_to(extension_root).as_posix()
        if relative in EXCLUDED:
            continue
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--extension-root", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    args = parser.parse_args()

    manifest = json.loads((args.extension_root / "manifest.json").read_text(encoding="utf-8"))
    candidate = {
        "schema": "saccade.extension-candidate/1",
        "id": candidate_id(args.extension_root),
        "version": manifest["version"],
    }
    encoded = json.dumps(candidate, indent=2) + "\n"
    (args.extension_root / "candidate.json").write_text(encoded, encoding="utf-8")
    identity = (
        "(() => {\n"
        f"  globalThis.SaccadeCandidate = Object.freeze({json.dumps(candidate, separators=(',', ':'))});\n"
        "})();\n"
    )
    (args.extension_root / "src" / "candidate_identity.js").write_text(
        identity, encoding="utf-8"
    )
    args.expected.parent.mkdir(parents=True, exist_ok=True)
    args.expected.write_text(encoded, encoding="utf-8")
    print(json.dumps(candidate, separators=(",", ":")))


if __name__ == "__main__":
    main()
