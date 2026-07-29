#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
EXTENSION_VERSION=$(sed -n 's/^[[:space:]]*"version": "\([0-9][0-9.]*\)",*$/\1/p' "$ROOT/extension/manifest.json" | sed -n '1p')
: "${EXTENSION_VERSION:?development Extension manifest has no version}"
DEV_ROOT="$HOME/Library/Application Support/Saccade Dev"
BIN_DIR="$DEV_ROOT/bin"
RUNTIME_APP="$HOME/Applications/Saccade Dev Runtime.app"
RUNTIME_MACOS="$RUNTIME_APP/Contents/MacOS"
RUNTIME_DIR="$DEV_ROOT/runtime"
STATE_DIR="$DEV_ROOT/state"
LOG_DIR="$DEV_ROOT/logs"
EVIDENCE_DIR="$DEV_ROOT/evidence"
FIXTURE_ROOT="$DEV_ROOT/fixture-root"
EXTENSION_ROOT="$DEV_ROOT/extension"
CHROME_PROFILE="$DEV_ROOT/chrome-profile-$EXTENSION_VERSION"
EDGE_PROFILE="$DEV_ROOT/edge-profile-$EXTENSION_VERSION"
CHROME_CACHE="$HOME/Library/Caches/Saccade Dev/chrome-for-testing"
HOST_DIR="$HOME/Library/Application Support/Google/Chrome for Testing/NativeMessagingHosts"
HOST_DIR_COMPACT="$HOME/Library/Application Support/Google/ChromeForTesting/NativeMessagingHosts"
HOST_DIR_CHROME="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
HOST_DIR_EDGE="$HOME/Library/Application Support/Microsoft Edge/NativeMessagingHosts"
SYSTEM_HOST_DIR="/Library/Google/ChromeForTesting/NativeMessagingHosts"
SYSTEM_HOST_MANIFEST="$SYSTEM_HOST_DIR/com.nanlogic.saccade.dev.json"
RUNTIME="$RUNTIME_MACOS/saccade-runtime"
FIXTURE_URL="http://127.0.0.1:8765/fixtures/controls/all.html"
MOUSE_ACCURACY_URL="http://127.0.0.1:8765/fixtures/conformance/mouse_accuracy.html"
MOUSE_ACCURACY_LAYOUT="${SACCADE_MOUSE_ACCURACY_LAYOUT:-buttons}"
MOUSE_ACCURACY_DIFFICULTY="${SACCADE_MOUSE_ACCURACY_DIFFICULTY:-ordinary}"
MOUSE_ACCURACY_BACKEND="${SACCADE_MOUSE_ACCURACY_BACKEND:-native}"
REFLEX_URL="${SACCADE_REFLEX_URL:-https://mouseaccuracy.com/game}"
REFLEX_MAX_ACTIONS="${SACCADE_REFLEX_MAX_ACTIONS:-500}"
REFLEX_TIMEOUT_MS="${SACCADE_REFLEX_TIMEOUT_MS:-30000}"
CODEX_BACKUP="$STATE_DIR/codex-saccade-backup.json"
PROFILE_BACKUP="$STATE_DIR/profile-before-test.json"
PROFILE_MISSING="$STATE_DIR/profile-was-missing"

mkdirs() {
  mkdir -p "$BIN_DIR" "$RUNTIME_MACOS" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$FIXTURE_ROOT" "$EXTENSION_ROOT" "$CHROME_PROFILE" "$EDGE_PROFILE"
  chmod 700 "$DEV_ROOT" "$BIN_DIR" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$FIXTURE_ROOT" "$EXTENSION_ROOT" "$CHROME_PROFILE" "$EDGE_PROFILE"
  chmod 755 "$RUNTIME_APP" "$RUNTIME_APP/Contents" "$RUNTIME_MACOS"
}

require_browser() {
  case "$1" in
    chrome|edge) ;;
    *) printf '%s\n' "browser must be chrome or edge" >&2; exit 2 ;;
  esac
}

browser_pid_file() {
  printf '%s/%s.pid\n' "$STATE_DIR" "$1"
}

browser_path_file() {
  printf '%s/%s-path\n' "$STATE_DIR" "$1"
}

browser_job_label() {
  printf 'com.nanlogic.saccade.dev.%s\n' "$1"
}

browser_job_target() {
  printf 'gui/%s/%s\n' "$(id -u)" "$(browser_job_label "$1")"
}

browser_job_loaded() {
  launchctl print "$(browser_job_target "$1")" >/dev/null 2>&1
}

fixture_job_label() {
  printf '%s\n' 'com.nanlogic.saccade.dev.fixture'
}

fixture_job_target() {
  printf 'gui/%s/%s\n' "$(id -u)" "$(fixture_job_label)"
}

fixture_job_loaded() {
  launchctl print "$(fixture_job_target)" >/dev/null 2>&1
}

browser_profile() {
  case "$1" in
    chrome) printf '%s\n' "$CHROME_PROFILE" ;;
    edge) printf '%s\n' "$EDGE_PROFILE" ;;
  esac
}

other_browser() {
  case "$1" in
    chrome) printf '%s\n' edge ;;
    edge) printf '%s\n' chrome ;;
  esac
}

pid_alive() {
  pid_alive_file=$1
  [ -f "$pid_alive_file" ] || return 1
  pid_alive_pid=$(sed -n '1p' "$pid_alive_file")
  case "$pid_alive_pid" in
    ''|*[!0-9]*) return 1 ;;
  esac
  kill -0 "$pid_alive_pid" 2>/dev/null
}

stop_pid() {
  stop_pid_file=$1
  if pid_alive "$stop_pid_file"; then
    stop_pid_pid=$(sed -n '1p' "$stop_pid_file")
    kill "$stop_pid_pid" 2>/dev/null || true
    stop_pid_count=0
    while kill -0 "$stop_pid_pid" 2>/dev/null && [ "$stop_pid_count" -lt 50 ]; do
      sleep 0.1
      stop_pid_count=$((stop_pid_count + 1))
    done
    if kill -0 "$stop_pid_pid" 2>/dev/null; then
      kill -9 "$stop_pid_pid" 2>/dev/null || true
    fi
  fi
  rm -f "$stop_pid_file"
}

clear_browser_singletons() {
  singleton_browser=$1
  singleton_profile=$(browser_profile "$singleton_browser")
  for singleton_name in SingletonSocket SingletonCookie SingletonLock; do
    singleton_path="$singleton_profile/$singleton_name"
    if [ -L "$singleton_path" ]; then
      unlink "$singleton_path"
    fi
  done
}

browser_profile_owner() {
  owner_browser=$1
  owner_profile=$(browser_profile "$owner_browser")
  owner_lock="$owner_profile/SingletonLock"
  [ -L "$owner_lock" ] || return 1
  owner_target=$(readlink "$owner_lock")
  owner_pid=${owner_target##*-}
  case "$owner_pid" in
    ''|*[!0-9]*) return 1 ;;
  esac
  owner_command=$(ps -p "$owner_pid" -o command= 2>/dev/null || true)
  case "$owner_command" in
    *"--user-data-dir=$owner_profile"*) printf '%s\n' "$owner_pid" ;;
    *) return 1 ;;
  esac
}

mark_browser_profile_clean() {
  clean_browser=$1
  clean_preferences="$(browser_profile "$clean_browser")/Default/Preferences"
  [ -f "$clean_preferences" ] || return 0
  python3 - "$clean_preferences" <<'PY'
import json
import os
import sys
from pathlib import Path

preferences = Path(sys.argv[1])
data = json.loads(preferences.read_text(encoding="utf-8"))
profile = data.setdefault("profile", {})
profile["exit_type"] = "Normal"
profile["exited_cleanly"] = True
temporary = preferences.with_name(f"{preferences.name}.saccade-tmp")
temporary.write_text(json.dumps(data, separators=(",", ":")), encoding="utf-8")
os.chmod(temporary, preferences.stat().st_mode)
temporary.replace(preferences)
PY
}

stop_browser() {
  stop_browser_name=$1
  if browser_job_loaded "$stop_browser_name"; then
    launchctl remove "$(browser_job_label "$stop_browser_name")"
  fi
  stop_pid "$(browser_pid_file "$stop_browser_name")"
  stop_browser_owner=$(browser_profile_owner "$stop_browser_name" || true)
  if [ -z "$stop_browser_owner" ]; then
    clear_browser_singletons "$stop_browser_name"
  fi
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
  for host_dir in "$HOST_DIR" "$HOST_DIR_COMPACT" "$HOST_DIR_CHROME" "$HOST_DIR_EDGE"; do
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

ensure_edge() {
  edge=${SACCADE_EDGE_PATH:-}
  if [ -z "$edge" ]; then
    for candidate in \
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge" \
      "$HOME/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
    do
      if [ -x "$candidate" ]; then
        edge=$candidate
        break
      fi
    done
  fi
  if [ -z "$edge" ] || [ ! -x "$edge" ]; then
    printf '%s\n' "Microsoft Edge is not installed. Install the stable macOS app or set SACCADE_EDGE_PATH." >&2
    exit 1
  fi
  printf '%s\n' "$edge" > "$STATE_DIR/edge-path"
}

ensure_browser() {
  require_browser "$1"
  case "$1" in
    chrome) ensure_chrome ;;
    edge) ensure_edge ;;
  esac
}

start_fixture() {
  if pid_alive "$STATE_DIR/fixture.pid"; then
    return
  fi
  if fixture_job_loaded; then
    launchctl remove "$(fixture_job_label)"
  fi
  launchctl submit \
    -l "$(fixture_job_label)" \
    -o "$LOG_DIR/fixture.stdout.log" \
    -e "$LOG_DIR/fixture.log" \
    -- /usr/bin/python3 -m http.server 8765 --bind 127.0.0.1 --directory "$FIXTURE_ROOT"
  fixture_count=0
  fixture_pid=
  while [ "$fixture_count" -lt 50 ]; do
    fixture_pid=$(launchctl print "$(fixture_job_target)" 2>/dev/null \
      | sed -n 's/^[[:space:]]*pid = \([0-9][0-9]*\)$/\1/p' | sed -n '1p')
    [ -n "$fixture_pid" ] && break
    sleep 0.1
    fixture_count=$((fixture_count + 1))
  done
  if [ -z "$fixture_pid" ]; then
    printf '%s\n' 'launchd did not report a PID for the fixture server' >&2
    exit 1
  fi
  printf '%s\n' "$fixture_pid" > "$STATE_DIR/fixture.pid"
  fixture_ready_count=0
  while [ "$fixture_ready_count" -lt 50 ]; do
    if curl -fsS -o /dev/null "$FIXTURE_URL" 2>/dev/null; then
      return
    fi
    sleep 0.1
    fixture_ready_count=$((fixture_ready_count + 1))
  done
  printf '%s\n' 'fixture server did not become ready' >&2
  exit 1
}

install_fixtures() {
  mkdir -p "$FIXTURE_ROOT/fixtures"
  cp -R "$ROOT/fixtures/." "$FIXTURE_ROOT/fixtures/"
  chmod -R u=rwX,go= "$FIXTURE_ROOT/fixtures"
}

install_extension() {
  cp -R "$ROOT/extension/." "$EXTENSION_ROOT/"
  chmod -R u=rwX,go= "$EXTENSION_ROOT"
}

stop_fixture() {
  if fixture_job_loaded; then
    launchctl remove "$(fixture_job_label)"
  fi
  stop_pid "$STATE_DIR/fixture.pid"
}

start_browser() {
  start_browser_name=$1
  require_browser "$start_browser_name"
  start_browser_pid_file=$(browser_pid_file "$start_browser_name")
  start_browser_inactive=$(other_browser "$start_browser_name")
  stop_browser "$start_browser_inactive"
  if pid_alive "$start_browser_pid_file"; then
    return
  fi
  if browser_job_loaded "$start_browser_name"; then
    launchctl remove "$(browser_job_label "$start_browser_name")"
  fi
  start_browser_owner=$(browser_profile_owner "$start_browser_name" || true)
  if [ -n "$start_browser_owner" ]; then
    printf '%s\n' "$start_browser_name profile is already owned by unrecorded PID $start_browser_owner" >&2
    exit 1
  fi
  clear_browser_singletons "$start_browser_name"
  mark_browser_profile_clean "$start_browser_name"
  start_browser_executable=$(sed -n '1p' "$(browser_path_file "$start_browser_name")")
  start_browser_profile=$(browser_profile "$start_browser_name")
  start_browser_label=$(browser_job_label "$start_browser_name")
  launchctl submit \
    -l "$start_browser_label" \
    -o "$LOG_DIR/$start_browser_name.stdout.log" \
    -e "$LOG_DIR/$start_browser_name.log" \
    -- /usr/bin/env \
    "HOME=$HOME" \
    "USER=${USER:-$(id -un)}" \
    "LOGNAME=${LOGNAME:-$(id -un)}" \
    "TMPDIR=${TMPDIR:-/tmp}" \
    'PATH=/usr/bin:/bin:/usr/sbin:/sbin' \
    "$start_browser_executable" \
    --user-data-dir="$start_browser_profile" \
    --load-extension="$EXTENSION_ROOT" \
    --disable-extensions-except="$EXTENSION_ROOT" \
    --enable-logging=stderr \
    '--vmodule=extensions*=1,native_message*=2' \
    --no-first-run \
    --no-default-browser-check \
    --disable-session-crashed-bubble \
    --window-position=24,52 \
    --window-size=800,747 \
    about:blank
  start_browser_count=0
  start_browser_pid=
  while [ "$start_browser_count" -lt 50 ]; do
    start_browser_pid=$(launchctl print "$(browser_job_target "$start_browser_name")" 2>/dev/null \
      | sed -n 's/^[[:space:]]*pid = \([0-9][0-9]*\)$/\1/p' | sed -n '1p')
    [ -n "$start_browser_pid" ] && break
    sleep 0.1
    start_browser_count=$((start_browser_count + 1))
  done
  if [ -z "$start_browser_pid" ]; then
    printf '%s\n' "launchd did not report a PID for $start_browser_name" >&2
    exit 1
  fi
  printf '%s\n' "$start_browser_pid" > "$start_browser_pid_file"
  printf '%s\n' "$start_browser_name" > "$STATE_DIR/active-browser"
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
  up_browser=${1:-chrome}
  require_browser "$up_browser"
  mkdirs
  install_runtime
  install_native_manifest
  install_codex_mcp
  install_fixtures
  install_extension
  ensure_browser "$up_browser"
  start_fixture
  stop_browser "$up_browser"
  start_browser "$up_browser"
  runtime_hash=$(shasum -a 256 "$RUNTIME" | awk '{print $1}')
  requested_hash=$(sed -n '1p' "$STATE_DIR/accessibility-requested" 2>/dev/null || true)
  if [ "$requested_hash" != "$runtime_hash" ]; then
    SACCADE_RUNTIME_DIR="$RUNTIME_DIR" "$RUNTIME" repair
    printf '%s\n' "$runtime_hash" > "$STATE_DIR/accessibility-requested"
  fi
  printf '%s\n' "Saccade Dev $up_browser is starting. Run ./scripts/dev.sh status, then ./scripts/dev.sh test $up_browser."
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
  test_browser=$1
  test_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$test_browser"
  mkdirs
  restore_profile
  up "$test_browser"
  test_run_dir="$EVIDENCE_DIR/$test_stamp/$test_browser"
  mkdir -p "$test_run_dir"
  chmod 700 "$test_run_dir"
  python3 "$ROOT/scripts/dev_probe.py" controls \
    --browser "$test_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$FIXTURE_URL" \
    --output "$test_run_dir/controls.json"

  stop_browser "$test_browser"
  write_test_profile
  trap 'stop_browser "$test_browser"; restore_profile; start_browser "$test_browser"' EXIT
  start_browser "$test_browser"
  python3 "$ROOT/scripts/dev_probe.py" profile \
    --browser "$test_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$FIXTURE_URL" \
    --output "$test_run_dir/profile.json"
  stop_browser "$test_browser"
  restore_profile
  start_browser "$test_browser"
  trap - EXIT HUP INT TERM
  printf '%s\n' "Cataloged-control $test_browser evidence: $test_run_dir"
}

test_all() {
  all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  test_route chrome "$all_stamp"
  test_route edge "$all_stamp"
  printf '%s\n' "Chrome and Edge evidence: $EVIDENCE_DIR/$all_stamp"
}

accuracy_route() {
  accuracy_browser=$1
  accuracy_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$accuracy_browser"
  mkdirs
  restore_profile
  up "$accuracy_browser"
  accuracy_run_dir="$EVIDENCE_DIR/$accuracy_stamp/$accuracy_browser"
  accuracy_browser_pid=$(sed -n '1p' "$(browser_pid_file "$accuracy_browser")")
  mkdir -p "$accuracy_run_dir"
  chmod 700 "$accuracy_run_dir"
  python3 "$ROOT/scripts/dev_probe.py" mouse_accuracy \
    --browser "$accuracy_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --window-pid "$accuracy_browser_pid" \
    --mouse-backend "$MOUSE_ACCURACY_BACKEND" \
    --accuracy-layout "$MOUSE_ACCURACY_LAYOUT" \
    --accuracy-difficulty "$MOUSE_ACCURACY_DIFFICULTY" \
    --url "$MOUSE_ACCURACY_URL" \
    --output "$accuracy_run_dir/mouse_accuracy.json"
  printf '%s\n' "Mouse-accuracy $accuracy_browser evidence: layout=$MOUSE_ACCURACY_LAYOUT difficulty=$MOUSE_ACCURACY_DIFFICULTY backend=$MOUSE_ACCURACY_BACKEND evidence: $accuracy_run_dir/mouse_accuracy.json"
}

accuracy_all() {
  accuracy_all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  accuracy_route chrome "$accuracy_all_stamp"
  accuracy_route edge "$accuracy_all_stamp"
  printf '%s\n' "Chrome and Edge ordinary mouse-accuracy evidence: $EVIDENCE_DIR/$accuracy_all_stamp"
}

reflex_route() {
  reflex_browser=$1
  reflex_backend=${2:-soft}
  require_browser "$reflex_browser"
  case "$reflex_backend" in
    native|soft) ;;
    *) printf '%s\n' "input backend must be native or soft" >&2; exit 2 ;;
  esac
  mkdirs
  restore_profile
  up "$reflex_browser"
  reflex_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  reflex_run_dir="$EVIDENCE_DIR/$reflex_stamp/$reflex_browser"
  mkdir -p "$reflex_run_dir"
  chmod 700 "$reflex_run_dir"
  python3 "$ROOT/scripts/dev_probe.py" reflex \
    --browser "$reflex_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --mouse-backend "$reflex_backend" \
    --max-actions "$REFLEX_MAX_ACTIONS" \
    --timeout-ms "$REFLEX_TIMEOUT_MS" \
    --url "$REFLEX_URL" \
    --output "$reflex_run_dir/reflex.json"
  printf '%s\n' "Reflex $reflex_browser/$reflex_backend evidence: $reflex_run_dir/reflex.json"
}

status() {
  fixture=stopped
  chrome=stopped
  edge=stopped
  pid_alive "$STATE_DIR/fixture.pid" && fixture=running
  pid_alive "$STATE_DIR/chrome.pid" && chrome=running
  pid_alive "$STATE_DIR/edge.pid" && edge=running
  active=$(sed -n '1p' "$STATE_DIR/active-browser" 2>/dev/null || true)
  printf 'fixture=%s chrome=%s edge=%s active=%s\n' "$fixture" "$chrome" "$edge" "${active:-none}"
  if [ -x "$RUNTIME" ]; then
    SACCADE_RUNTIME_DIR="$RUNTIME_DIR" "$RUNTIME" doctor
  fi
}

down() {
  stop_browser chrome
  stop_browser edge
  stop_fixture
  rm -f "$STATE_DIR/active-browser"
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
  up) up "${2:-chrome}" ;;
  test)
    case "${2:-chrome}" in
      all) test_all ;;
      chrome|edge) test_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  accuracy)
    case "${2:-chrome}" in
      all) accuracy_all ;;
      chrome|edge) accuracy_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  reflex) reflex_route "${2:-chrome}" "${3:-soft}" ;;
  status) status ;;
  down) down ;;
  *) printf '%s\n' "usage: ./scripts/dev.sh <up [chrome|edge]|test [chrome|edge|all]|accuracy [chrome|edge|all]|reflex [chrome|edge] [native|soft]|status|down>" >&2; exit 2 ;;
esac
