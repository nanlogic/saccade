#!/usr/bin/env python3
"""Install and restore the development Saccade Codex MCP entry."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


def run(codex: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(codex), "mcp", *args],
        check=check,
        text=True,
        capture_output=True,
    )


def backup(codex: Path, path: Path) -> None:
    if path.exists():
        return
    existing = run(codex, "get", "saccade", "--json", check=False)
    value = json.loads(existing.stdout) if existing.returncode == 0 else {"missing": True}
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def install(codex: Path, saved: Path, runtime: Path, runtime_dir: Path) -> None:
    backup(codex, saved)
    run(codex, "remove", "saccade", check=False)
    run(
        codex,
        "add",
        "saccade",
        "--env",
        f"SACCADE_RUNTIME_DIR={runtime_dir}",
        "--",
        str(runtime),
        "mcp",
    )


def restore(codex: Path, saved: Path) -> None:
    if not saved.exists():
        return
    value = json.loads(saved.read_text(encoding="utf-8"))
    run(codex, "remove", "saccade", check=False)
    if not value.get("missing"):
        transport = value.get("transport", {})
        if transport.get("type") != "stdio" or transport.get("cwd"):
            raise SystemExit("saved Saccade MCP entry is not a restorable stdio configuration")
        command = ["add", "saccade"]
        for key, item in (transport.get("env") or {}).items():
            command.extend(["--env", f"{key}={item}"])
        command.extend(["--", transport["command"], *(transport.get("args") or [])])
        run(codex, *command)
    saved.unlink()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["install", "restore"])
    parser.add_argument("--codex", type=Path, required=True)
    parser.add_argument("--backup", type=Path, required=True)
    parser.add_argument("--runtime", type=Path)
    parser.add_argument("--runtime-dir", type=Path)
    args = parser.parse_args()
    if args.action == "install":
        if args.runtime is None or args.runtime_dir is None:
            parser.error("install requires --runtime and --runtime-dir")
        install(args.codex, args.backup, args.runtime, args.runtime_dir)
    else:
        restore(args.codex, args.backup)


if __name__ == "__main__":
    main()
