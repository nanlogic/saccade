#!/usr/bin/env python3
"""Install a cached Chrome for Testing build and print its executable path."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import tempfile
import urllib.request
from pathlib import Path


VERSIONS_URL = (
    "https://googlechromelabs.github.io/chrome-for-testing/"
    "last-known-good-versions-with-downloads.json"
)


def platform_name() -> str:
    machine = platform.machine().lower()
    if platform.system() != "Darwin":
        raise SystemExit("managed Chrome for Testing currently supports macOS only")
    return "mac-arm64" if machine in {"arm64", "aarch64"} else "mac-x64"


def executable(root: Path) -> Path:
    return root / "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"


def install(cache: Path) -> Path:
    current = cache / "current.json"
    if current.exists():
        saved_value = json.loads(current.read_text(encoding="utf-8"))
        saved = Path(saved_value["executable"])
        if saved.is_file() and os.access(saved, os.X_OK):
            return saved
        broken_root = cache / saved_value["version"]
        if broken_root.is_dir():
            shutil.rmtree(broken_root)
        current.unlink()

    with urllib.request.urlopen(VERSIONS_URL, timeout=30) as response:
        metadata = json.load(response)
    stable = metadata["channels"]["Stable"]
    target = platform_name()
    download = next(item for item in stable["downloads"]["chrome"] if item["platform"] == target)
    version_root = cache / stable["version"]
    binary = executable(version_root)
    if not binary.is_file():
        cache.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="saccade-chrome-") as temporary:
            archive = Path(temporary) / "chrome.zip"
            with urllib.request.urlopen(download["url"], timeout=120) as response:
                with archive.open("wb") as output:
                    shutil.copyfileobj(response, output)
            version_root.mkdir(parents=True, exist_ok=True)
            subprocess.run(["ditto", "-x", "-k", str(archive), str(version_root)], check=True)
        binary = next(version_root.glob("*/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"))
    if not os.access(binary, os.X_OK):
        raise SystemExit("Chrome for Testing archive did not preserve executable permissions")
    current.write_text(
        json.dumps({"version": stable["version"], "executable": str(binary)}, indent=2) + "\n",
        encoding="utf-8",
    )
    return binary


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache", type=Path, required=True)
    args = parser.parse_args()
    print(install(args.cache.resolve()))


if __name__ == "__main__":
    main()
