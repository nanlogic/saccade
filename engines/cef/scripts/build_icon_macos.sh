#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
SOURCE=${SACCADE_ICON_SOURCE:-$REPO_ROOT/engines/cef/assets/saccade-icon-macos.png}
OUTPUT=${1:-$REPO_ROOT/engines/cef/assets/Saccade.icns}
WORK=$(mktemp -d /tmp/saccade-icon.XXXXXX)
ICONSET="$WORK/Saccade.iconset"

cleanup() {
  rm -rf "$WORK"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$ICONSET"
if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 1024 -h 1024 "$SOURCE" -o "$WORK/base.png"
elif sips -s format png "$SOURCE" --out "$WORK/base.png" >/dev/null 2>&1; then
  :
else
  qlmanage -t -s 1024 -o "$WORK" "$SOURCE" >/dev/null 2>&1
  BASE=$(find "$WORK" -maxdepth 1 -type f -name '*.png' -print | head -1)
  [ -n "$BASE" ] || { echo "Could not render Saccade icon source: $SOURCE" >&2; exit 1; }
  mv "$BASE" "$WORK/base.png"
fi

render_size() {
  pixels=$1
  name=$2
  sips -z "$pixels" "$pixels" "$WORK/base.png" \
    --out "$ICONSET/$name" >/dev/null
}

render_size 16 icon_16x16.png
render_size 32 icon_16x16@2x.png
render_size 32 icon_32x32.png
render_size 64 icon_32x32@2x.png
render_size 128 icon_128x128.png
render_size 256 icon_128x128@2x.png
render_size 256 icon_256x256.png
render_size 512 icon_256x256@2x.png
render_size 512 icon_512x512.png
render_size 1024 icon_512x512@2x.png

mkdir -p "$(dirname -- "$OUTPUT")"
iconutil -c icns "$ICONSET" -o "$OUTPUT"
printf '%s\n' "$OUTPUT"
