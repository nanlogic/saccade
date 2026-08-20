#!/usr/bin/env python3
"""Require the live Extension to match the candidate installed on disk."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--expected", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=10.0)
    args = parser.parse_args()

    expected = json.loads(args.expected.read_text(encoding="utf-8"))
    environment = dict(os.environ)
    environment["SACCADE_RUNTIME_DIR"] = str(args.runtime_dir)
    deadline = time.monotonic() + args.timeout
    last = None
    while time.monotonic() < deadline:
        result = subprocess.run(
            [str(args.runtime), "doctor"],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        try:
            report = json.loads(result.stdout)
            capabilities = report.get("capabilities") or {}
            last = capabilities.get("extension_candidate")
            if report.get("ready") and last == expected:
                print(json.dumps({"ok": True, "candidate": expected}, separators=(",", ":")))
                return
        except json.JSONDecodeError:
            last = result.stderr.strip() or result.stdout.strip() or None
        time.sleep(0.25)

    raise SystemExit(
        "Live Saccade Extension candidate does not match the installed candidate. "
        "Reload Saccade once from Chrome's Extensions page to activate this "
        "pre-handshake bootstrap build, then rerun attach. "
        f"expected={expected!r} live={last!r}"
    )


if __name__ == "__main__":
    main()
