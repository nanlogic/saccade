#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${SACCADE_CEF_APP:-$REPO_ROOT/target/cef-release/Saccade.app}
MODE=${1:-normal}
[ "$#" -eq 0 ] || shift
URL=${1:-https://example.com}
[ "$#" -eq 0 ] || shift
EXE="$APP/Contents/MacOS/cefsimple"

[ -x "$EXE" ] || {
  echo "Saccade CEF app is missing; run engines/cef/scripts/build_macos.sh" >&2
  exit 1
}

case "$MODE" in
  normal)
    NAME=${SACCADE_PROFILE_NAME:-default}
    case "$NAME" in
      *[!A-Za-z0-9_-]*|'')
        echo "Invalid SACCADE_PROFILE_NAME" >&2
        exit 2
        ;;
    esac
    PROFILE="$HOME/Library/Application Support/Saccade/CEF/Profiles/$NAME"
    mkdir -p "$PROFILE"
    chmod 700 "$PROFILE"
    exec "$EXE" --url="$URL" --user-data-dir="$PROFILE" \
      --no-first-run --no-default-browser-check "$@"
    ;;
  incognito)
    ROOT="$HOME/Library/Caches/Saccade/Incognito"
    mkdir -p "$ROOT"
    SESSION=$(mktemp -d "$ROOT/session.XXXXXX")
    chmod 700 "$SESSION"
    cleanup() { rm -rf "$SESSION"; }
    trap cleanup EXIT HUP INT TERM
    "$EXE" --url="$URL" --user-data-dir="$SESSION" --incognito \
      --no-first-run --no-default-browser-check "$@"
    ;;
  *)
    echo "Usage: $0 normal|incognito [url] [CEF switches...]" >&2
    exit 2
    ;;
esac
