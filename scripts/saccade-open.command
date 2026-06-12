#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_URL="https://example.com"

printf "Saccade dogfood URL [%s]: " "$DEFAULT_URL"
read -r URL
URL="${URL:-$DEFAULT_URL}"

cd "$ROOT"
export RUST_LOG="${RUST_LOG:-error}"
cargo run -q -p saccade-shell -- browse --url "$URL" --width 1440 --height 1000
