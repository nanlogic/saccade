#!/usr/bin/env python3
"""Write a reproducible identity for one local Saccade candidate and browser pair."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import UTC, datetime
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def command(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def browser_version(executable: Path) -> str:
    return subprocess.check_output([str(executable), "--version"], text=True).strip()


def working_tree_fingerprint() -> str:
    digest = hashlib.sha256()
    digest.update(subprocess.check_output(["git", "diff", "--binary"], cwd=ROOT))
    untracked = command("git", "ls-files", "--others", "--exclude-standard").splitlines()
    for relative in sorted(untracked):
        path = ROOT / relative
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chrome", required=True, type=Path)
    parser.add_argument("--edge", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    status = command("git", "status", "--porcelain")
    extension = json.loads((ROOT / "extension" / "manifest.json").read_text(encoding="utf-8"))
    manifest = {
        "schema": "saccade.candidate-manifest/1",
        "created_at": datetime.now(UTC).isoformat().replace("+00:00", "Z"),
        "commit": command("git", "rev-parse", "HEAD"),
        "dirty": bool(status),
        "working_tree_fingerprint_sha256": working_tree_fingerprint(),
        "runtime_version": command("cargo", "metadata", "--no-deps", "--format-version", "1"),
        "extension_version": extension["version"],
        "browsers": {
            "chrome": browser_version(args.chrome.resolve()),
            "edge": browser_version(args.edge.resolve()),
        },
    }
    metadata = json.loads(manifest.pop("runtime_version"))
    runtime = next(package for package in metadata["packages"] if package["name"] == "saccade_runtime")
    manifest["runtime_version"] = runtime["version"]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, "output": str(args.output), "dirty": manifest["dirty"]}))


if __name__ == "__main__":
    main()
