#!/usr/bin/env python3
"""Build an unpublished, checksummed macOS Runtime release draft."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def runtime_contract_hash(runtime: Path) -> str:
    with tempfile.TemporaryDirectory(prefix="saccade-release-doctor-") as directory:
        completed = subprocess.run(
            [str(runtime), "doctor"],
            check=False,
            capture_output=True,
            text=True,
            env={**os.environ, "SACCADE_RUNTIME_DIR": directory},
        )
    try:
        value = json.loads(completed.stdout)
        contract_hash = value["mcp_contract_hash"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise ValueError("Runtime doctor did not report mcp_contract_hash") from error
    if not isinstance(contract_hash, str) or len(contract_hash) != 64 or any(
        character not in "0123456789abcdef" for character in contract_hash
    ):
        raise ValueError("Runtime doctor reported an invalid mcp_contract_hash")
    return contract_hash


def build(
    runtime: Path,
    platform: str,
    output_dir: Path,
    *,
    signed: bool = False,
) -> dict[str, object]:
    if platform not in {"darwin-arm64", "darwin-x64"}:
        raise ValueError("setup preview artifacts support darwin-arm64 or darwin-x64")
    if not runtime.is_file():
        raise ValueError(f"Runtime artifact does not exist: {runtime}")
    extension_candidate = json.loads((ROOT / "extension/candidate.json").read_text())
    setup_package = json.loads((ROOT / "packages/setup/package.json").read_text())
    output_dir.mkdir(parents=True, exist_ok=True)
    artifact = output_dir / f"saccade-runtime-{setup_package['version']}-{platform}"
    shutil.copyfile(runtime, artifact)
    artifact.chmod(0o755)
    manifest = {
        "schema": "saccade.setup-release/1",
        "published": False,
        "version": setup_package["version"],
        "mcp_contract_hash": runtime_contract_hash(runtime),
        "extension_candidate": extension_candidate,
        "native_host": {"name": "com.nanlogic.saccade", "allowed_origins": []},
        "artifacts": {
            platform: {
                "local_file": artifact.name,
                "sha256": digest(artifact),
                "url": None,
                "signed": signed,
            }
        },
        "external_blockers": [
            "macOS signing material",
            "Chrome Web Store Extension origin",
            "Edge Add-ons Extension origin",
            "HTTPS artifact URL",
            "@nanlogic/saccade trusted-publisher binding",
        ],
    }
    manifest_path = output_dir / "release.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (output_dir / "SHA256SUMS").write_text(
        f"{manifest['artifacts'][platform]['sha256']}  {artifact.name}\n", encoding="utf-8"
    )
    return {"manifest": manifest_path, "artifact": artifact, "sha256": manifest["artifacts"][platform]["sha256"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=("darwin-arm64", "darwin-x64"))
    parser.add_argument("--output", default=ROOT / "dist/setup-preview", type=Path)
    parser.add_argument("--signed", action="store_true")
    args = parser.parse_args()
    result = build(
        args.runtime.resolve(),
        args.platform,
        args.output.resolve(),
        signed=args.signed,
    )
    print(json.dumps({key: str(value) for key, value in result.items()}))


if __name__ == "__main__":
    main()
