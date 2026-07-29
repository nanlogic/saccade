#!/usr/bin/env python3
"""Install and inspect the human-selected Saccade development Profile."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


def validate(profile: Any) -> dict[str, Any]:
    if not isinstance(profile, dict) or set(profile) != {"name", "behavior", "ban"}:
        raise ValueError("Profile must contain exactly name, behavior, and ban")
    if not isinstance(profile["name"], str) or not profile["name"].strip():
        raise ValueError("Profile name must be a non-empty string")
    if not isinstance(profile["behavior"], str):
        raise ValueError("Profile behavior must be a string")
    if not isinstance(profile["ban"], list):
        raise ValueError("Profile ban must be a list")
    for index, rule in enumerate(profile["ban"]):
        if not isinstance(rule, dict) or not {"control"} <= set(rule) <= {"control", "condition"}:
            raise ValueError(f"Profile ban[{index}] must contain control and optional condition")
        if not isinstance(rule["control"], str) or not rule["control"].strip():
            raise ValueError(f"Profile ban[{index}].control must be non-empty")
        if "condition" in rule and (
            not isinstance(rule["condition"], str) or not rule["condition"].strip()
        ):
            raise ValueError(f"Profile ban[{index}].condition must be non-empty when present")
    return profile


def read_profile(path: Path) -> dict[str, Any]:
    return validate(json.loads(path.read_text(encoding="utf-8")))


def install(source: Path, destination: Path) -> dict[str, Any]:
    profile = read_profile(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.tmp")
    temporary.write_text(json.dumps(profile, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    os.chmod(temporary, 0o600)
    temporary.replace(destination)
    return profile


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("set", "show", "reset"))
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--profiles-dir", required=True, type=Path)
    parser.add_argument("--profile")
    args = parser.parse_args()

    destination = args.runtime_dir / "profile.json"
    if args.command == "show":
        profile = read_profile(destination) if destination.exists() else read_profile(args.profiles_dir / "default.json")
    else:
        if args.command == "reset":
            source = args.profiles_dir / "default.json"
        else:
            if not args.profile:
                parser.error("set requires --profile")
            requested = Path(args.profile)
            source = requested if requested.is_absolute() or requested.parent != Path(".") else args.profiles_dir / f"{requested.name.removesuffix('.json')}.json"
        profile = install(source, destination)
    print(json.dumps(profile, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
