#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${SACCADE_CEF_APP:-$REPO_ROOT/target/cef-release/Saccade.app}
EXE="$APP/Contents/MacOS/cefsimple"
MODE=${1:-normal}
[ "$#" -eq 0 ] || shift
URL=${1:-https://example.com}
[ "$#" -eq 0 ] || shift

[ -x "$EXE" ] || {
  echo "Saccade CEF app is missing; run engines/cef/scripts/build_macos.sh" >&2
  exit 1
}

PRIVATE_ROOT=${SACCADE_ENGINE_SESSION_DIR:-$(mktemp -d "/tmp/saccade-${UID}.XXXXXX")}
mkdir -p "$PRIVATE_ROOT"
chmod 700 "$PRIVATE_ROOT"
export SACCADE_ENGINE_SOCKET="$PRIVATE_ROOT/control.sock"
export SACCADE_ENGINE_GRANT_PATH="$PRIVATE_ROOT/grant.json"
export SACCADE_ENGINE_GRANT_CURRENT_TAB=1

cleanup() { rm -rf "$PRIVATE_ROOT"; }
trap cleanup EXIT HUP INT TERM
printf 'SACCADE_GRANT_PATH=%s\n' "$SACCADE_ENGINE_GRANT_PATH"

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
    "$EXE" --url="$URL" --user-data-dir="$PROFILE" \
      --no-first-run --no-default-browser-check "$@"
    ;;
  incognito)
    PROFILE="$PRIVATE_ROOT/incognito-profile"
    mkdir -p "$PROFILE"
    chmod 700 "$PROFILE"
    "$EXE" --url="$URL" --user-data-dir="$PROFILE" --incognito \
      --no-first-run --no-default-browser-check "$@"
    ;;
  *)
    echo "Usage: $0 normal|incognito [url] [CEF switches...]" >&2
    exit 2
    ;;
esac
