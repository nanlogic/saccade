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
mv "$APP/Contents/MacOS/cefsimple" "$APP/Contents/MacOS/Saccade"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName Saccade' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier ai.saccade.browser' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleExecutable Saccade' "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Add :CFBundleDisplayName string Saccade' \
  "$APP/Contents/Info.plist" 2>/dev/null || \
  /usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName Saccade' "$APP/Contents/Info.plist"
"$SCRIPT_DIR/build_icon_macos.sh" \
  "$REPO_ROOT/engines/cef/assets/Saccade.icns"
cp "$REPO_ROOT/engines/cef/assets/Saccade.icns" \
  "$APP/Contents/Resources/Saccade.icns"
sips -z 64 64 "$REPO_ROOT/engines/cef/assets/saccade-icon-windows.png" \
  --out "$APP/Contents/Resources/Saccade-tab.png" >/dev/null
rm -f "$APP/Contents/Resources/cefsimple.icns"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIconFile Saccade.icns' \
  "$APP/Contents/Info.plist"

set_plist_string() {
  KEY=$1
  VALUE=$2
  /usr/libexec/PlistBuddy -c "Add :$KEY string $VALUE" \
    "$APP/Contents/Info.plist" 2>/dev/null || \
    /usr/libexec/PlistBuddy -c "Set :$KEY $VALUE" "$APP/Contents/Info.plist"
}

SACCADE_VERSION=${SACCADE_VERSION:-0.1.0}
SACCADE_BUILD_NUMBER=${SACCADE_BUILD_NUMBER:-41}
set_plist_string CFBundleShortVersionString "$SACCADE_VERSION"
set_plist_string CFBundleVersion "$SACCADE_BUILD_NUMBER"
set_plist_string CFBundleGetInfoString \
  "Saccade $SACCADE_VERSION ($SACCADE_BUILD_NUMBER) by NaN Logic LLC, based on Chromium"
set_plist_string NSHumanReadableCopyright \
  "Copyright © 2026 NaN Logic LLC. Based on Chromium."
set_plist_string SaccadePublisherName "NaN Logic LLC"
set_plist_string SaccadePublisherURL "https://nanlogic.com/"
set_plist_string SaccadeHelpURL "https://nanlogic.com/"

rename_helper() {
  OLD_NAME=$1
  NEW_NAME=$2
  BUNDLE_ID=$3
  OLD_HELPER="$APP/Contents/Frameworks/$OLD_NAME.app"
  HELPER="$APP/Contents/Frameworks/$NEW_NAME.app"
  if [ -d "$OLD_HELPER" ]; then
    mv "$OLD_HELPER" "$HELPER"
  fi
  [ -d "$HELPER" ] || {
    echo "Missing helper: $NEW_NAME" >&2
    exit 1
  }
  OLD_EXE="$HELPER/Contents/MacOS/$OLD_NAME"
  NEW_EXE="$HELPER/Contents/MacOS/$NEW_NAME"
  if [ -x "$OLD_EXE" ]; then
    mv "$OLD_EXE" "$NEW_EXE"
  fi
  [ -x "$NEW_EXE" ] || {
    echo "Missing helper executable: $NEW_EXE" >&2
    exit 1
  }
  HELPER_PLIST="$HELPER/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $NEW_NAME" "$HELPER_PLIST"
  /usr/libexec/PlistBuddy -c "Set :CFBundleExecutable $NEW_NAME" "$HELPER_PLIST"
  /usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$HELPER_PLIST"
  /usr/libexec/PlistBuddy -c "Set :CFBundleName $NEW_NAME" "$HELPER_PLIST"
  /usr/libexec/PlistBuddy -c \
    "Set :CFBundleShortVersionString $SACCADE_VERSION" "$HELPER_PLIST"
  /usr/libexec/PlistBuddy -c \
    "Set :CFBundleVersion $SACCADE_BUILD_NUMBER" "$HELPER_PLIST"
}

rename_helper 'cefsimple Helper' 'Saccade Helper' \
  ai.saccade.browser.helper
rename_helper 'cefsimple Helper (Alerts)' 'Saccade Helper (Alerts)' \
  ai.saccade.browser.helper.alerts
rename_helper 'cefsimple Helper (GPU)' 'Saccade Helper (GPU)' \
  ai.saccade.browser.helper.gpu
rename_helper 'cefsimple Helper (Plugin)' 'Saccade Helper (Plugin)' \
  ai.saccade.browser.helper.plugin
rename_helper 'cefsimple Helper (Renderer)' 'Saccade Helper (Renderer)' \
  ai.saccade.browser.helper.renderer

rm -f "$APP/Contents/Resources/Info.plist.in"

# Chromium may enumerate nearby security keys after a user explicitly chooses
# a passkey. Without this key macOS terminates the browser instead of showing a
# permission prompt.
set_plist_string NSBluetoothAlwaysUsageDescription \
  'Saccade uses Bluetooth only when you choose a nearby passkey or security key.'
set_plist_string NSBluetoothPeripheralUsageDescription \
  'Saccade uses Bluetooth only when you choose a nearby passkey or security key.'
cp "$CEF_ROOT/LICENSE.txt" "$APP/Contents/Resources/CEF_LICENSE.txt"
cp "$CEF_ROOT/CREDITS.html" "$APP/Contents/Resources/CHROMIUM_CREDITS.html"

if [ -n "${SACCADE_CODESIGN_IDENTITY:-}" ]; then
  "$SCRIPT_DIR/sign_macos.sh" "$APP"
fi

[ -x "$APP/Contents/MacOS/Saccade" ] || { echo "Missing $APP" >&2; exit 1; }
printf '%s\n' "$APP"
