#!/usr/bin/env python3
"""Wait until the managed browser Host can complete an MCP handshake."""

from __future__ import annotations

import argparse
from pathlib import Path

from dev_probe import wait_for_mcp


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", required=True, type=Path)
    parser.add_argument("--runtime-dir", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    args = parser.parse_args()
    client = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve(), args.timeout)
    client.close()
    print("Saccade MCP ready")


if __name__ == "__main__":
    main()
