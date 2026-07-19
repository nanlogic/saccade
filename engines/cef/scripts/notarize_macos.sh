#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
COMMAND=${1:-preflight}
APP=${2:-$REPO_ROOT/target/cef-release/Saccade.app}
OUT=${SACCADE_NOTARY_OUT:-$REPO_ROOT/dist/notarization}
PROFILE=${SACCADE_NOTARY_KEYCHAIN_PROFILE:-}
DMG="$OUT/Saccade.dmg"
APP_ZIP="$OUT/Saccade-notary.zip"
APP_RESULT="$OUT/app-notary-result.json"
DMG_RESULT="$OUT/dmg-notary-result.json"

fail() {
  echo "$1" >&2
  exit 1
}

[ -d "$APP" ] || fail "Missing app: $APP"
command -v codesign >/dev/null 2>&1 || fail "codesign is required."
command -v xcrun >/dev/null 2>&1 || fail "Xcode command-line tools are required."
xcrun notarytool --version >/dev/null
xcrun --find stapler >/dev/null

codesign --verify --deep --strict --verbose=2 "$APP"
MAIN_SIGNATURE=$(codesign -dvvv "$APP" 2>&1)
echo "$MAIN_SIGNATURE" | grep -q '^Authority=Developer ID Application:' ||
  fail "The app is not signed with Developer ID Application."
echo "$MAIN_SIGNATURE" | grep -q 'flags=.*runtime' ||
  fail "The app is missing Hardened Runtime."
echo "$MAIN_SIGNATURE" | grep -q '^Timestamp=' ||
  fail "The app is missing a secure timestamp. Re-sign with SACCADE_CODESIGN_TIMESTAMP=apple."

find "$APP/Contents" -type f -perm -111 -print | while IFS= read -r binary; do
  file "$binary" | grep -q 'Mach-O' || continue
  SIGNATURE=$(codesign -dvvv "$binary" 2>&1) ||
    fail "Unsigned executable: $binary"
  echo "$SIGNATURE" | grep -q 'flags=.*runtime' ||
    fail "Executable is missing Hardened Runtime: $binary"
  echo "$SIGNATURE" | grep -q '^Timestamp=' ||
    fail "Executable is missing a secure timestamp: $binary"
  ENTITLEMENTS=$(codesign -d --entitlements - "$binary" 2>/dev/null || true)
  if echo "$ENTITLEMENTS" | grep -q 'com.apple.security.get-task-allow'; then
    fail "Forbidden get-task-allow entitlement: $binary"
  fi
done

if [ "$COMMAND" = preflight ]; then
  echo "PASS: Developer ID, Hardened Runtime, secure timestamp and executable signatures."
  exit 0
fi
[ "$COMMAND" = submit ] || fail "Usage: notarize_macos.sh [preflight|submit] [Saccade.app]"
[ -n "$PROFILE" ] ||
  fail "Set SACCADE_NOTARY_KEYCHAIN_PROFILE to a notarytool Keychain profile."

mkdir -p "$OUT"
rm -f "$APP_ZIP" "$DMG" "$APP_RESULT" "$DMG_RESULT"
/usr/bin/ditto -c -k --keepParent "$APP" "$APP_ZIP"
xcrun notarytool submit "$APP_ZIP" --keychain-profile "$PROFILE" \
  --wait --output-format json > "$APP_RESULT"
jq -e '.status == "Accepted"' "$APP_RESULT" >/dev/null ||
  fail "App notarization was not accepted; inspect $APP_RESULT"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

STAGING=$(mktemp -d /tmp/saccade-dmg.XXXXXX)
trap 'rm -rf "$STAGING"' EXIT HUP INT TERM
/usr/bin/ditto "$APP" "$STAGING/Saccade.app"
hdiutil create -volname Saccade -srcfolder "$STAGING" -ov -format UDZO "$DMG"
IDENTITY=${SACCADE_CODESIGN_IDENTITY:-auto}
if [ "$IDENTITY" = auto ]; then
  IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null |
    sed -n 's/^[[:space:]]*[0-9][0-9]*) \([A-F0-9][A-F0-9]*\) "Developer ID Application:.*$/\1/p' |
    sed -n '1p')
fi
[ -n "$IDENTITY" ] || fail "No Developer ID Application identity found."
codesign --force --sign "$IDENTITY" --timestamp "$DMG"
hdiutil verify "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" \
  --wait --output-format json > "$DMG_RESULT"
jq -e '.status == "Accepted"' "$DMG_RESULT" >/dev/null ||
  fail "DMG notarization was not accepted; inspect $DMG_RESULT"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
spctl --assess --type execute --verbose=4 "$APP"
spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG"

echo "PASS: notarized and stapled app plus DMG: $DMG"
