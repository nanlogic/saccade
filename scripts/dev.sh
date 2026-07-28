#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DEV_ROOT="$HOME/Library/Application Support/Saccade Dev"
BIN_DIR="$DEV_ROOT/bin"
RUNTIME_APP="$HOME/Applications/Saccade Dev Runtime.app"
RUNTIME_MACOS="$RUNTIME_APP/Contents/MacOS"
RUNTIME_DIR="$DEV_ROOT/runtime"
STATE_DIR="$DEV_ROOT/state"
LOG_DIR="$DEV_ROOT/logs"
EVIDENCE_DIR="$DEV_ROOT/evidence"
CHROME_PROFILE="$DEV_ROOT/chrome-profile"
CHROME_CACHE="$HOME/Library/Caches/Saccade Dev/chrome-for-testing"
HOST_DIR="$HOME/Library/Application Support/Google/Chrome for Testing/NativeMessagingHosts"
HOST_DIR_COMPACT="$HOME/Library/Application Support/Google/ChromeForTesting/NativeMessagingHosts"
HOST_DIR_CHROME="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
SYSTEM_HOST_DIR="/Library/Google/ChromeForTesting/NativeMessagingHosts"
SYSTEM_HOST_MANIFEST="$SYSTEM_HOST_DIR/com.nanlogic.saccade.dev.json"
RUNTIME="$RUNTIME_MACOS/saccade-runtime"
FIXTURE_URL="http://127.0.0.1:8765/fixtures/controls/all.html"
CODEX_BACKUP="$STATE_DIR/codex-saccade-backup.json"
PROFILE_BACKUP="$STATE_DIR/profile-before-test.json"
PROFILE_MISSING="$STATE_DIR/profile-was-missing"

mkdirs() {
  mkdir -p "$BIN_DIR" "$RUNTIME_MACOS" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$CHROME_PROFILE"
  chmod 700 "$DEV_ROOT" "$BIN_DIR" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$CHROME_PROFILE"
  chmod 755 "$RUNTIME_APP" "$RUNTIME_APP/Contents" "$RUNTIME_MACOS"
}

pid_alive() {
  pid_file=$1
  [ -f "$pid_file" ] || return 1
  pid=$(sed -n '1p' "$pid_file")
  case "$pid" in
    ''|*[!0-9]*) return 1 ;;
  esac
  kill -0 "$pid" 2>/dev/null
}

stop_pid() {
  pid_file=$1
  if pid_alive "$pid_file"; then
    pid=$(sed -n '1p' "$pid_file")
    kill "$pid"
    count=0
    while kill -0 "$pid" 2>/dev/null && [ "$count" -lt 50 ]; do
      sleep 0.1
      count=$((count + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid"
    fi
  fi
  rm -f "$pid_file"
}

find_codex() {
  for candidate in \
    "/Applications/ChatGPT.app/Contents/Resources/codex" \
    "/Applications/Codex.app/Contents/Resources/codex"
  do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  command -v codex
}

install_runtime() {
  signing_identity=${SACCADE_DEV_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning | sed -n 's/.*"\(Apple Development:[^"]*\)"/\1/p' | sed -n '1p')}
  if [ -z "$signing_identity" ]; then
    printf '%s\n' "Saccade Dev requires an Apple Development signing identity for stable Accessibility permission." >&2
    exit 1
  fi
  signing_version="3:$signing_identity"
  cargo build --release --manifest-path "$ROOT/Cargo.toml" -p saccade-runtime
  source_hash=$(shasum -a 256 "$ROOT/target/release/saccade-runtime" | awk '{print $1}')
  installed_source_hash=$(sed -n '1p' "$STATE_DIR/runtime-source.sha256" 2>/dev/null || true)
  installed_signing_version=$(sed -n '1p' "$STATE_DIR/runtime-signing.version" 2>/dev/null || true)
  runtime_changed=false
  if [ ! -x "$RUNTIME" ] || [ "$source_hash" != "$installed_source_hash" ]; then
    runtime_installing="$RUNTIME_MACOS/saccade-runtime.installing"
    cp "$ROOT/target/release/saccade-runtime" "$runtime_installing"
    chmod 700 "$runtime_installing"
    mv -f "$runtime_installing" "$RUNTIME"
    runtime_changed=true
  fi
  if ! cmp -s "$ROOT/scripts/dev/Info.plist" "$RUNTIME_APP/Contents/Info.plist"; then
    cp "$ROOT/scripts/dev/Info.plist" "$RUNTIME_APP/Contents/Info.plist.installing"
    chmod 644 "$RUNTIME_APP/Contents/Info.plist.installing"
    mv -f "$RUNTIME_APP/Contents/Info.plist.installing" "$RUNTIME_APP/Contents/Info.plist"
    runtime_changed=true
  fi
  if [ "$installed_signing_version" != "$signing_version" ]; then
    runtime_changed=true
  fi
  if [ "$runtime_changed" = true ]; then
    codesign --force --deep --options runtime --timestamp=none \
      --sign "$signing_identity" \
      --identifier com.nanlogic.saccade.dev.runtime \
      "$RUNTIME_APP"
    codesign --verify --deep --strict "$RUNTIME_APP"
    printf '%s\n' "$source_hash" > "$STATE_DIR/runtime-source.sha256"
    printf '%s\n' "$signing_version" > "$STATE_DIR/runtime-signing.version"
  fi
  if [ ! -f "$RUNTIME_DIR/profile.json" ]; then
    cp "$ROOT/profiles/default.json" "$RUNTIME_DIR/profile.json"
    chmod 600 "$RUNTIME_DIR/profile.json"
  fi
}

install_native_manifest() {
  for host_dir in "$HOST_DIR" "$HOST_DIR_COMPACT" "$HOST_DIR_CHROME"; do
    mkdir -p "$host_dir"
    host_manifest="$host_dir/com.nanlogic.saccade.dev.json"
    python3 - "$ROOT/scripts/dev/com.nanlogic.saccade.dev.json.in" "$host_manifest" "$RUNTIME" <<'PY'
import json
import sys
from pathlib import Path

source, target, launcher = map(Path, sys.argv[1:])
value = json.loads(source.read_text(encoding="utf-8").replace("@HOST_PATH@", str(launcher)))
target.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
PY
    chmod 600 "$host_manifest"
  done
  source_manifest="$HOST_DIR/com.nanlogic.saccade.dev.json"
  if ! cmp -s "$source_manifest" "$SYSTEM_HOST_MANIFEST"; then
    osascript -l JavaScript "$ROOT/scripts/dev/install_native_host.js" \
      "$source_manifest" "$SYSTEM_HOST_DIR"
  fi
  if ! cmp -s "$source_manifest" "$SYSTEM_HOST_MANIFEST"; then
    printf '%s\n' "Chrome for Testing Native Messaging manifest installation failed." >&2
    exit 1
  fi
}

ensure_chrome() {
  chrome=$(python3 "$ROOT/scripts/dev_chrome.py" --cache "$CHROME_CACHE")
  printf '%s\n' "$chrome" > "$STATE_DIR/chrome-path"
}

start_fixture() {
  if pid_alive "$STATE_DIR/fixture.pid"; then
    return
  fi
  nohup python3 -m http.server 8765 --bind 127.0.0.1 --directory "$ROOT" \
    >"$LOG_DIR/fixture.log" 2>&1 &
  printf '%s\n' "$!" > "$STATE_DIR/fixture.pid"
}

start_chrome() {
  if pid_alive "$STATE_DIR/chrome.pid"; then
    return
  fi
  chrome=$(sed -n '1p' "$STATE_DIR/chrome-path")
  nohup "$chrome" \
    --user-data-dir="$CHROME_PROFILE" \
    --load-extension="$ROOT/extension" \
    --disable-extensions-except="$ROOT/extension" \
    --enable-logging=stderr \
    '--vmodule=extensions*=1,native_message*=2' \
    --no-first-run \
    --no-default-browser-check \
    about:blank >"$LOG_DIR/chrome.log" 2>&1 &
  printf '%s\n' "$!" > "$STATE_DIR/chrome.pid"
}

install_codex_mcp() {
  codex=$(find_codex)
  printf '%s\n' "$codex" > "$STATE_DIR/codex-path"
  python3 "$ROOT/scripts/dev_codex_config.py" install \
    --codex "$codex" \
    --backup "$CODEX_BACKUP" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR"
}

up() {
  mkdirs
  install_runtime
  install_native_manifest
  install_codex_mcp
  ensure_chrome
  start_fixture
  start_chrome
  runtime_hash=$(shasum -a 256 "$RUNTIME" | awk '{print $1}')
  requested_hash=$(sed -n '1p' "$STATE_DIR/accessibility-requested" 2>/dev/null || true)
  if [ "$requested_hash" != "$runtime_hash" ]; then
    SACCADE_RUNTIME_DIR="$RUNTIME_DIR" "$RUNTIME" repair
    printf '%s\n' "$runtime_hash" > "$STATE_DIR/accessibility-requested"
  fi
  printf '%s\n' "Saccade Dev is starting. Run ./scripts/dev.sh status, then ./scripts/dev.sh test."
}

restore_profile() {
  if [ -f "$PROFILE_BACKUP" ]; then
    cp "$PROFILE_BACKUP" "$RUNTIME_DIR/profile.json"
    chmod 600 "$RUNTIME_DIR/profile.json"
    rm -f "$PROFILE_BACKUP"
  elif [ -f "$PROFILE_MISSING" ]; then
    rm -f "$RUNTIME_DIR/profile.json" "$PROFILE_MISSING"
  fi
}

write_test_profile() {
  if [ -f "$RUNTIME_DIR/profile.json" ]; then
    cp "$RUNTIME_DIR/profile.json" "$PROFILE_BACKUP"
    chmod 600 "$PROFILE_BACKUP"
  else
    : > "$PROFILE_MISSING"
  fi
  python3 - "$RUNTIME_DIR/profile.json" <<'PY'
import json
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "name": "integration",
    "behavior": "Saccade profile integration test.",
    "ban": [{"control": "Save"}],
}, indent=2) + "\n", encoding="utf-8")
PY
  chmod 600 "$RUNTIME_DIR/profile.json"
}

test_route() {
  mkdirs
  restore_profile
  up
  stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  run_dir="$EVIDENCE_DIR/$stamp"
  mkdir -p "$run_dir"
  chmod 700 "$run_dir"
  python3 "$ROOT/scripts/dev_probe.py" controls \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$FIXTURE_URL" \
    --output "$run_dir/controls.json"

  stop_pid "$STATE_DIR/chrome.pid"
  write_test_profile
  trap 'stop_pid "$STATE_DIR/chrome.pid"; restore_profile; start_chrome' EXIT
  start_chrome
  python3 "$ROOT/scripts/dev_probe.py" profile \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$FIXTURE_URL" \
    --output "$run_dir/profile.json"
  stop_pid "$STATE_DIR/chrome.pid"
  restore_profile
  start_chrome
  trap - EXIT HUP INT TERM
  printf '%s\n' "Four-control evidence: $run_dir"
}

status() {
  fixture=stopped
  chrome=stopped
  pid_alive "$STATE_DIR/fixture.pid" && fixture=running
  pid_alive "$STATE_DIR/chrome.pid" && chrome=running
  printf 'fixture=%s chrome=%s\n' "$fixture" "$chrome"
  if [ -x "$RUNTIME" ]; then
    SACCADE_RUNTIME_DIR="$RUNTIME_DIR" "$RUNTIME" doctor
  fi
}

down() {
  stop_pid "$STATE_DIR/chrome.pid"
  stop_pid "$STATE_DIR/fixture.pid"
  restore_profile
  if [ -f "$STATE_DIR/codex-path" ]; then
    codex=$(sed -n '1p' "$STATE_DIR/codex-path")
    python3 "$ROOT/scripts/dev_codex_config.py" restore \
      --codex "$codex" \
      --backup "$CODEX_BACKUP"
  fi
  printf '%s\n' "Saccade Dev processes stopped and the prior Codex MCP entry restored."
}

case "${1:-}" in
  up) up ;;
  test) test_route ;;
  status) status ;;
  down) down ;;
  *) printf '%s\n' "usage: ./scripts/dev.sh <up|test|status|down>" >&2; exit 2 ;;
esac
