#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: sign_notarize_runtime.sh <runtime>" >&2
  exit 2
fi

: "${APPLE_DEVELOPER_ID_CERT_P12_BASE64:?missing Apple signing certificate}"
: "${APPLE_DEVELOPER_ID_CERT_PASSWORD:?missing Apple signing certificate password}"
: "${APPLE_API_KEY_P8_BASE64:?missing Apple notarization API key}"
: "${APPLE_API_KEY_ID:?missing Apple notarization key ID}"
: "${APPLE_API_ISSUER_ID:?missing Apple notarization issuer ID}"

runtime=$1
if [ ! -f "$runtime" ]; then
  echo "Runtime does not exist: $runtime" >&2
  exit 2
fi

temporary_dir=$(mktemp -d "${TMPDIR:-/tmp}/saccade-notary.XXXXXX")
keychain="$temporary_dir/release.keychain-db"
certificate="$temporary_dir/developer-id.p12"
api_key="$temporary_dir/AuthKey_${APPLE_API_KEY_ID}.p8"
submission="$temporary_dir/saccade-runtime.zip"
keychain_password=$(openssl rand -hex 24)

cleanup() {
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  rm -rf "$temporary_dir"
}
trap cleanup EXIT HUP INT TERM

printf '%s' "$APPLE_DEVELOPER_ID_CERT_P12_BASE64" | /usr/bin/base64 -D > "$certificate"
printf '%s' "$APPLE_API_KEY_P8_BASE64" | /usr/bin/base64 -D > "$api_key"
chmod 600 "$certificate" "$api_key"

security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security list-keychains -d user -s "$keychain"
security import "$certificate" -k "$keychain" -P "$APPLE_DEVELOPER_ID_CERT_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$keychain_password" "$keychain"
identity=$(security find-identity -v -p codesigning "$keychain" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -n 1)
if [ -z "$identity" ]; then
  echo "Developer ID Application identity was not imported" >&2
  exit 1
fi

codesign --force --options runtime --timestamp --keychain "$keychain" --sign "$identity" "$runtime"
codesign --verify --strict --verbose=2 "$runtime"
ditto -c -k --keepParent "$runtime" "$submission"
xcrun notarytool submit "$submission" \
  --key "$api_key" \
  --key-id "$APPLE_API_KEY_ID" \
  --issuer "$APPLE_API_ISSUER_ID" \
  --wait
