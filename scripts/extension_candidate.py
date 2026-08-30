"""Cross-platform identity for one Extension source candidate."""

from __future__ import annotations

import hashlib
from pathlib import Path


EXCLUDED = {"candidate.json", "src/candidate_identity.js"}
EXCLUDED_PREFIXES = ("tests/",)
TEXT_SUFFIXES = {".css", ".html", ".js", ".json", ".md", ".svg", ".txt"}


def canonical_bytes(path: Path) -> bytes:
    data = path.read_bytes()
    if path.suffix.casefold() in TEXT_SUFFIXES:
        return data.replace(b"\r\n", b"\n").replace(b"\r", b"\n")
    return data


def candidate_id(extension_root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(item for item in extension_root.rglob("*") if item.is_file()):
        relative = path.relative_to(extension_root).as_posix()
        if relative in EXCLUDED or relative.startswith(EXCLUDED_PREFIXES):
            continue
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(canonical_bytes(path))
        digest.update(b"\0")
    return digest.hexdigest()
