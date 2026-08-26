#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CLI="$ROOT/packages/setup/bin/saccade.js"

usage() {
  printf '%s\n' 'Usage: ./scripts/dev.sh broker|mcp|test|doctor|pack'
}

case "${1:-}" in
  broker) exec node "$CLI" broker ;;
  mcp) exec node "$CLI" mcp ;;
  doctor) exec node "$CLI" doctor ;;
  test)
    node --test "$ROOT"/packages/setup/test/*.test.js
    node --test "$ROOT"/extension/tests/*.test.js
    python3 "$ROOT/scripts/check_single_architecture.py"
    ;;
  pack) npm pack "$ROOT/packages/setup" --dry-run ;;
  *) usage; exit 2 ;;
esac
