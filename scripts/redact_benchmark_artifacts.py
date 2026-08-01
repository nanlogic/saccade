#!/usr/bin/env python3
"""Apply a benchmark task's editable-value redaction to existing local artifacts."""

from __future__ import annotations

import argparse
from pathlib import Path

from benchmark_agent_fair import load_task, redact_text


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, type=Path)
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    task = load_task(args.task.resolve())
    directory = args.directory.resolve()
    changed = 0
    for path in directory.rglob("*"):
        if path.is_symlink() or not path.is_file() or path.suffix not in {".json", ".jsonl", ".log"}:
            continue
        original = path.read_text(encoding="utf-8", errors="replace")
        redacted = redact_text(original, task["redact"])
        if redacted != original:
            path.write_text(redacted, encoding="utf-8")
            changed += 1
    print(f"redacted {changed} artifact files")


if __name__ == "__main__":
    main()
