#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${1:-$REPO_ROOT/target/cef-release/Saccade.app}
IDENTITY=${SACCADE_CODESIGN_IDENTITY:-}

[ -d "$APP" ] || {
  echo "Missing CEF app: $APP" >&2
  exit 1
}

if [ "$IDENTITY" = auto ]; then
  IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null |
    sed -n 's/^[[:space:]]*[0-9][0-9]*) \([A-F0-9][A-F0-9]*\) "Developer ID Application:.*$/\1/p' |
    sed -n '1p')
fi

[ -n "$IDENTITY" ] || {
  echo "Set SACCADE_CODESIGN_IDENTITY to a certificate SHA-1 or use 'auto'." >&2
  exit 2
}

sign_path() {
  if [ "${SACCADE_CODESIGN_TIMESTAMP:-apple}" = none ]; then
    codesign --force --sign "$IDENTITY" --timestamp=none "$1"
  else
    codesign --force --sign "$IDENTITY" --timestamp "$1"
  fi
}

FRAMEWORK="$APP/Contents/Frameworks/Chromium Embedded Framework.framework"
[ -d "$FRAMEWORK" ] || {
  echo "Missing CEF framework: $FRAMEWORK" >&2
  exit 1
}

# Sign leaf Mach-O files before the bundles that seal them.
find "$FRAMEWORK/Versions/A/Libraries" -type f -perm -111 -print |
  while IFS= read -r binary; do sign_path "$binary"; done
sign_path "$FRAMEWORK"

find "$APP/Contents/Frameworks" -maxdepth 1 -type d -name '*.app' -print |
  while IFS= read -r helper; do sign_path "$helper"; done
sign_path "$APP"

find "$APP/Contents/Frameworks" -maxdepth 1 -type d -name '*.app' -print |
  while IFS= read -r helper; do
    codesign --verify --strict --verbose=2 "$helper"
  done
codesign --verify --strict --verbose=2 "$APP"

MAIN_TEAM=$(codesign -dvv "$APP" 2>&1 |
  sed -n 's/^TeamIdentifier=//p')
MAIN_IDENTIFIER=$(codesign -dvv "$APP" 2>&1 |
  sed -n 's/^Identifier=//p')
[ "$MAIN_IDENTIFIER" = ai.saccade.browser ] || {
  echo "Unexpected signed app identifier: $MAIN_IDENTIFIER" >&2
  exit 1
}
[ -n "$MAIN_TEAM" ] && [ "$MAIN_TEAM" != "not set" ] || {
  echo "Signed app has no stable TeamIdentifier" >&2
  exit 1
}

printf 'Signed %s as %s (team %s)\n' "$APP" "$MAIN_IDENTIFIER" "$MAIN_TEAM"
