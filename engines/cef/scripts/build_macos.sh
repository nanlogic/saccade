#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
CEF_ROOT=${CEF_ROOT:-$($SCRIPT_DIR/fetch_macos.sh)}
BUILD_DIR=${SACCADE_CEF_BUILD_DIR:-$REPO_ROOT/target/cef-release}
UPSTREAM_BUILD="$BUILD_DIR/upstream"

cmake -G Ninja \
  -DPROJECT_ARCH=arm64 \
  -DCMAKE_BUILD_TYPE=Release \
  -S "$CEF_ROOT" \
  -B "$UPSTREAM_BUILD"
cmake --build "$UPSTREAM_BUILD" --target cefsimple -j "${SACCADE_BUILD_JOBS:-8}"

SOURCE_APP="$UPSTREAM_BUILD/tests/cefsimple/Release/cefsimple.app"
APP="$BUILD_DIR/Saccade.app"
[ -x "$SOURCE_APP/Contents/MacOS/cefsimple" ] || {
  echo "Missing upstream app: $SOURCE_APP" >&2
  exit 1
}

rm -rf "$APP"
ditto "$SOURCE_APP" "$APP"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Saccade' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier ai.saccade.browser' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleDisplayName string Saccade' \
  "$APP/Contents/Info.plist" 2>/dev/null || \
  /usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Saccade' "$APP/Contents/Info.plist"
cp "$CEF_ROOT/LICENSE.txt" "$APP/Contents/Resources/CEF_LICENSE.txt"
cp "$CEF_ROOT/CREDITS.html" "$APP/Contents/Resources/CHROMIUM_CREDITS.html"

[ -x "$APP/Contents/MacOS/cefsimple" ] || { echo "Missing $APP" >&2; exit 1; }
printf '%s\n' "$APP"
