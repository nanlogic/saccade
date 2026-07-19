#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${1:-$REPO_ROOT/target/cef-release/Saccade.app}
IDENTITY=${SACCADE_CODESIGN_IDENTITY:-}
HARDENED_RUNTIME=${SACCADE_HARDENED_RUNTIME:-1}
JIT_ENTITLEMENTS="$REPO_ROOT/engines/cef/entitlements/cef-helper-jit.plist"

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
  TARGET=$1
  ENTITLEMENTS=${2:-}
  OPTIONS=
  if [ "$HARDENED_RUNTIME" = 1 ]; then
    OPTIONS=runtime
  elif [ "$HARDENED_RUNTIME" != 0 ]; then
    echo "SACCADE_HARDENED_RUNTIME must be 0 or 1." >&2
    exit 2
  fi

  set -- codesign --force --sign "$IDENTITY"
  [ -z "$OPTIONS" ] || set -- "$@" --options "$OPTIONS"
  [ -z "$ENTITLEMENTS" ] || set -- "$@" --entitlements "$ENTITLEMENTS"
  if [ "${SACCADE_CODESIGN_TIMESTAMP:-apple}" = none ]; then
    set -- "$@" --timestamp=none
  else
    set -- "$@" --timestamp
  fi
  "$@" "$TARGET"
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

# Sign every executable auxiliary file inside the app before sealing the outer
# bundle. This covers the self-contained MCP binary and its installed launcher.
find "$APP/Contents/MacOS" -maxdepth 1 -type f -perm -111 -print |
  while IFS= read -r binary; do
    [ "$(basename "$binary")" = Saccade ] || sign_path "$binary"
  done

find "$APP/Contents/Frameworks" -maxdepth 1 -type d -name '*.app' -print |
  while IFS= read -r helper; do
    case "$(basename "$helper")" in
      'Saccade Helper.app'|'Saccade Helper (GPU).app'|'Saccade Helper (Renderer).app')
        sign_path "$helper" "$JIT_ENTITLEMENTS"
        ;;
      *) sign_path "$helper" ;;
    esac
  done
sign_path "$APP"

find "$APP/Contents/Frameworks" -maxdepth 1 -type d -name '*.app' -print |
  while IFS= read -r helper; do
    codesign --verify --strict --verbose=2 "$helper"
  done
codesign --verify --strict --verbose=2 "$APP"
[ ! -x "$APP/Contents/MacOS/saccade-mcp" ] ||
  codesign --verify --strict --verbose=2 \
    "$APP/Contents/MacOS/saccade-mcp"

verify_helper() {
  NAME=$1
  EXPECTED_ID=$2
  HELPER="$APP/Contents/Frameworks/$NAME.app"
  PLIST="$HELPER/Contents/Info.plist"
  [ -x "$HELPER/Contents/MacOS/$NAME" ] || {
    echo "Missing branded helper executable: $NAME" >&2
    exit 1
  }
  ACTUAL_NAME=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName' "$PLIST")
  ACTUAL_EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$PLIST")
  ACTUAL_ID=$(codesign -dvv "$HELPER" 2>&1 | sed -n 's/^Identifier=//p')
  [ "$ACTUAL_NAME" = "$NAME" ] && [ "$ACTUAL_EXECUTABLE" = "$NAME" ] &&
    [ "$ACTUAL_ID" = "$EXPECTED_ID" ] || {
      echo "Unexpected branded helper identity: $NAME / $ACTUAL_ID" >&2
      exit 1
    }
}

verify_helper 'Saccade Helper' ai.saccade.browser.helper
verify_helper 'Saccade Helper (Alerts)' ai.saccade.browser.helper.alerts
verify_helper 'Saccade Helper (GPU)' ai.saccade.browser.helper.gpu
verify_helper 'Saccade Helper (Plugin)' ai.saccade.browser.helper.plugin
verify_helper 'Saccade Helper (Renderer)' ai.saccade.browser.helper.renderer

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

if [ "$HARDENED_RUNTIME" = 1 ]; then
  codesign -dvvv "$APP" 2>&1 | grep -q 'flags=.*runtime' || {
    echo "Main app is missing hardened runtime." >&2
    exit 1
  }
  for helper_name in 'Saccade Helper' 'Saccade Helper (GPU)' \
    'Saccade Helper (Renderer)'; do
    HELPER="$APP/Contents/Frameworks/$helper_name.app"
    codesign -dvvv "$HELPER" 2>&1 | grep -q 'flags=.*runtime' || {
      echo "$helper_name is missing hardened runtime." >&2
      exit 1
    }
    codesign -d --entitlements - "$HELPER" 2>/dev/null |
      grep -q 'com.apple.security.cs.allow-jit' || {
        echo "$helper_name is missing the JIT entitlement." >&2
        exit 1
      }
  done
fi

printf 'Signed %s as %s (team %s)\n' "$APP" "$MAIN_IDENTIFIER" "$MAIN_TEAM"
