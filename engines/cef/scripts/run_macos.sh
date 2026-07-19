#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${SACCADE_CEF_APP:-$REPO_ROOT/target/cef-release/Saccade.app}
MODE=${1:-normal}
[ "$#" -eq 0 ] || shift
URL=${1:-https://example.com}
[ "$#" -eq 0 ] || shift
EXE="$APP/Contents/MacOS/Saccade"

if [ -n "${SACCADE_ENGINE_SESSION_DIR:-}" ]; then
  SESSION_ROOT=$SACCADE_ENGINE_SESSION_DIR
  SESSION_ROOT_OWNED=0
else
  SESSION_ROOT=$(mktemp -d "/tmp/saccade-${UID}.XXXXXX")
  SESSION_ROOT_OWNED=1
fi
mkdir -p "$SESSION_ROOT"
chmod 700 "$SESSION_ROOT"
export SACCADE_ENGINE_SOCKET="$SESSION_ROOT/control.sock"
export SACCADE_ENGINE_GRANT_PATH="$SESSION_ROOT/grant.json"
export SACCADE_ENGINE_GRANT_CURRENT_TAB=0
INCOGNITO_PROFILE=
cleanup() {
  rm -f "$SACCADE_ENGINE_SOCKET" "$SACCADE_ENGINE_GRANT_PATH"
  rm -f "$SACCADE_ENGINE_GRANT_PATH.replay.jsonl"
  [ -z "$INCOGNITO_PROFILE" ] || rm -rf "$INCOGNITO_PROFILE"
  [ "$SESSION_ROOT_OWNED" -eq 0 ] || rm -rf "$SESSION_ROOT"
}
trap cleanup EXIT HUP INT TERM

[ -x "$EXE" ] || {
  echo "Saccade CEF app is missing; run engines/cef/scripts/build_macos.sh" >&2
  exit 1
}

require_stable_signature() {
  SIGNATURE=$(codesign -dvv "$APP" 2>&1 || true)
  echo "$SIGNATURE" | grep -q '^Identifier=ai.saccade.browser$' &&
    echo "$SIGNATURE" | grep -q '^Authority=Developer ID Application:' &&
    echo "$SIGNATURE" | grep -q '^TeamIdentifier=[A-Z0-9][A-Z0-9]*$' || {
      echo "Normal profiles require the fixed signed Saccade build." >&2
      echo "Build the dogfood release; do not open saved login state with an ad-hoc debug app." >&2
      exit 3
    }
}

case "$MODE" in
  normal)
    require_stable_signature
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
    export SACCADE_PROFILE_MODE=normal
    export SACCADE_PROFILE_NAME="$NAME"
    "$EXE" --url="$URL" --user-data-dir="$PROFILE" \
      --no-first-run --no-default-browser-check "$@"
    ;;
  incognito)
    ROOT="$HOME/Library/Caches/Saccade/Incognito"
    mkdir -p "$ROOT"
    INCOGNITO_PROFILE=$(mktemp -d "$ROOT/session.XXXXXX")
    chmod 700 "$INCOGNITO_PROFILE"
    export SACCADE_PROFILE_MODE=incognito
    export SACCADE_PROFILE_NAME=private
    "$EXE" --url="$URL" --user-data-dir="$INCOGNITO_PROFILE" --incognito \
      --no-first-run --no-default-browser-check "$@"
    ;;
  *)
    echo "Usage: $0 normal|incognito [url] [CEF switches...]" >&2
    exit 2
    ;;
esac
