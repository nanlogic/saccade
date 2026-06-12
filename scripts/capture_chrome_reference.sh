#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 4 ]]; then
  echo "usage: $0 <url> <output-dir> [width] [height]" >&2
  exit 2
fi

URL="$1"
OUTPUT_DIR="$2"
WIDTH="${3:-1920}"
HEIGHT="${4:-1080}"

CHROME="${CHROME:-}"
if [[ -z "$CHROME" ]]; then
  for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge" \
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"; do
    if [[ -x "$candidate" ]]; then
      CHROME="$candidate"
      break
    fi
  done
fi

if [[ -z "$CHROME" || ! -x "$CHROME" ]]; then
  echo "could not find Chrome/Chromium; set CHROME=/path/to/browser" >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
ABS_OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
SCREENSHOT="$ABS_OUTPUT_DIR/chrome_page.png"
MANIFEST="$ABS_OUTPUT_DIR/chrome_reference_manifest.json"
STDERR_LOG="$ABS_OUTPUT_DIR/chrome_stderr.log"
USER_DATA_DIR="$(mktemp -d)"
trap 'rm -rf "$USER_DATA_DIR"' EXIT

"$CHROME" \
  --headless=new \
  --disable-gpu \
  --disable-background-networking \
  --disable-component-update \
  --disable-crash-reporter \
  --disable-default-apps \
  --disable-features=OptimizationHints,MediaRouter \
  --disable-sync \
  --metrics-recording-only \
  --no-first-run \
  --no-default-browser-check \
  --password-store=basic \
  --use-mock-keychain \
  --user-data-dir="$USER_DATA_DIR" \
  --force-device-scale-factor=1 \
  --window-size="${WIDTH},${HEIGHT}" \
  --screenshot="$SCREENSHOT" \
  "$URL" >/dev/null 2>"$STDERR_LOG" &
CHROME_PID=$!

DEADLINE=$((SECONDS + 30))
while kill -0 "$CHROME_PID" 2>/dev/null; do
  if [[ -s "$SCREENSHOT" ]]; then
    sleep 0.5
    kill "$CHROME_PID" 2>/dev/null || true
    wait "$CHROME_PID" 2>/dev/null || true
    break
  fi
  if (( SECONDS >= DEADLINE )); then
    kill "$CHROME_PID" 2>/dev/null || true
    wait "$CHROME_PID" 2>/dev/null || true
    echo "Chrome reference capture timed out; stderr follows:" >&2
    tail -80 "$STDERR_LOG" >&2 || true
    exit 1
  fi
  sleep 0.25
done

if [[ ! -s "$SCREENSHOT" ]]; then
  echo "Chrome reference capture failed to produce $SCREENSHOT; stderr follows:" >&2
  tail -80 "$STDERR_LOG" >&2 || true
  exit 1
fi

cat > "$MANIFEST" <<EOF
{
  "engine": "chrome-reference-v0",
  "url": "$URL",
  "browser_binary": "$CHROME",
  "viewport": {
    "width": $WIDTH,
    "height": $HEIGHT,
    "device_scale_factor": 1
  },
  "artifacts": {
    "page_screenshot": "chrome_page.png",
    "stderr_log": "chrome_stderr.log"
  },
  "note": "This is a Chrome-rendered page-content screenshot, not a browser-UI screenshot with URL bar."
}
EOF

echo "CHROME REFERENCE READY screenshot=$SCREENSHOT manifest=$MANIFEST"
echo "Use only with local fixtures or non-sensitive pages; screenshots capture visible page values."
