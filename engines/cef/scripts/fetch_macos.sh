#!/bin/sh
set -eu

VERSION='150.0.11'
ARCHIVE='cef_binary_150.0.11+gb887805+chromium-150.0.7871.115_macosarm64.tar.bz2'
URL='https://cef-builds.spotifycdn.com/cef_binary_150.0.11%2Bgb887805%2Bchromium-150.0.7871.115_macosarm64.tar.bz2'
SHA1='d4ad036deeea6531773a7190fceff2b45d75647b'
SHA256='fbd7652b5ac4224e446fdde909fa2fcd88ac75c173ef042363738a86d5ad3f0a'
CACHE_ROOT="${SACCADE_CEF_CACHE:-$HOME/Library/Caches/Saccade/cef/$VERSION}"
ARCHIVE_PATH="$CACHE_ROOT/$ARCHIVE"
EXTRACTED="$CACHE_ROOT/${ARCHIVE%.tar.bz2}"

mkdir -p "$CACHE_ROOT"
if [ ! -f "$ARCHIVE_PATH" ]; then
  curl --fail --location --output "$ARCHIVE_PATH.part" "$URL"
  mv "$ARCHIVE_PATH.part" "$ARCHIVE_PATH"
fi

actual_sha1=$(shasum "$ARCHIVE_PATH" | awk '{print $1}')
actual_sha256=$(shasum -a 256 "$ARCHIVE_PATH" | awk '{print $1}')
[ "$actual_sha1" = "$SHA1" ] || { echo "CEF SHA-1 mismatch" >&2; exit 1; }
[ "$actual_sha256" = "$SHA256" ] || { echo "CEF SHA-256 mismatch" >&2; exit 1; }

if [ ! -d "$EXTRACTED" ]; then
  tar -xjf "$ARCHIVE_PATH" -C "$CACHE_ROOT"
fi

printf '%s\n' "$EXTRACTED"
