#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
SOURCE=${SACCADE_ICON_SOURCE:-$REPO_ROOT/engines/cef/assets/saccade-icon-windows.png}
OUTPUT=${1:-$REPO_ROOT/engines/cef/assets/Saccade.ico}

command -v magick >/dev/null 2>&1 || {
  echo "ImageMagick (magick) is required to build the Windows icon." >&2
  exit 1
}

mkdir -p "$(dirname -- "$OUTPUT")"
magick "$SOURCE" \
  -background none \
  -define icon:auto-resize=256,128,64,48,32,24,16 \
  "$OUTPUT"
printf '%s\n' "$OUTPUT"
