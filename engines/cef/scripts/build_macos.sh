#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
CEF_ROOT=${CEF_ROOT:-$($SCRIPT_DIR/fetch_macos.sh)}
$SCRIPT_DIR/prepare_adapter_macos.sh
BUILD_DIR=${SACCADE_CEF_BUILD_DIR:-$REPO_ROOT/target/cef-release}
UPSTREAM_BUILD="$BUILD_DIR/upstream"
SOURCE_APP="$UPSTREAM_BUILD/tests/cefsimple/Release/cefsimple.app"

cmake -G Ninja \
  -DPROJECT_ARCH=arm64 \
  -DCMAKE_BUILD_TYPE=Release \
  -S "$CEF_ROOT" \
  -B "$UPSTREAM_BUILD"

cmake --build "$UPSTREAM_BUILD" --target cefsimple -j "${SACCADE_BUILD_JOBS:-8}"

# CEF's macOS framework packaging macro is not idempotent. On an incremental
# build, ln(1) can follow existing directory symlinks and create nested links
# such as Resources/Resources. Normalize the generated framework after every
# build; -h replaces the link itself instead of following its directory target.
SOURCE_FRAMEWORK="$SOURCE_APP/Contents/Frameworks/Chromium Embedded Framework.framework"
rm -f \
  "$SOURCE_FRAMEWORK/Versions/A/Resources/Resources" \
  "$SOURCE_FRAMEWORK/Versions/A/Libraries/Libraries" \
  "$SOURCE_FRAMEWORK/Versions/A/A"
ln -s -h -f "Versions/A/Chromium Embedded Framework" \
  "$SOURCE_FRAMEWORK/Chromium Embedded Framework"
ln -s -h -f "Versions/A/Libraries" "$SOURCE_FRAMEWORK/Libraries"
ln -s -h -f "Versions/A/Resources" "$SOURCE_FRAMEWORK/Resources"
ln -s -h -f "A" "$SOURCE_FRAMEWORK/Versions/Current"

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

if [ -n "${SACCADE_CODESIGN_IDENTITY:-}" ]; then
  "$SCRIPT_DIR/sign_macos.sh" "$APP"
fi

[ -x "$APP/Contents/MacOS/cefsimple" ] || { echo "Missing $APP" >&2; exit 1; }
printf '%s\n' "$APP"
