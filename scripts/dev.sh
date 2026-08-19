#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
EXTENSION_VERSION=$(sed -n 's/^[[:space:]]*"version": "\([0-9][0-9.]*\)",*$/\1/p' "$ROOT/extension/manifest.json" | sed -n '1p')
: "${EXTENSION_VERSION:?development Extension manifest has no version}"
BROWSER_PROFILE_GENERATION=17
DEV_ROOT="$HOME/Library/Application Support/Saccade Dev"
BIN_DIR="$DEV_ROOT/bin"
RUNTIME_APP="$HOME/Applications/Saccade Dev Runtime.app"
RUNTIME_MACOS="$RUNTIME_APP/Contents/MacOS"
RUNTIME_DIR="$DEV_ROOT/runtime"
STATE_DIR="$DEV_ROOT/state"
LOG_DIR="$DEV_ROOT/logs"
EVIDENCE_DIR="$DEV_ROOT/evidence"
FIXTURE_ROOT="$DEV_ROOT/fixture-root"
EXTENSION_ROOT="$DEV_ROOT/extension-$EXTENSION_VERSION"
CHROME_PROFILE="$DEV_ROOT/chrome-profile-v$BROWSER_PROFILE_GENERATION"
EDGE_PROFILE="$DEV_ROOT/edge-profile-v$BROWSER_PROFILE_GENERATION"
LEGACY_CHROME_PROFILE="$DEV_ROOT/chrome-profile-0.3.3"
LEGACY_EDGE_PROFILE="$DEV_ROOT/edge-profile-0.3.3"
CHROME_CACHE="$HOME/Library/Caches/Saccade Dev/chrome-for-testing"
HOST_DIR="$HOME/Library/Application Support/Google/Chrome for Testing/NativeMessagingHosts"
HOST_DIR_COMPACT="$HOME/Library/Application Support/Google/ChromeForTesting/NativeMessagingHosts"
HOST_DIR_CHROME="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
HOST_DIR_EDGE="$HOME/Library/Application Support/Microsoft Edge/NativeMessagingHosts"
SYSTEM_HOST_DIR="/Library/Google/ChromeForTesting/NativeMessagingHosts"
SYSTEM_HOST_MANIFEST="$SYSTEM_HOST_DIR/com.nanlogic.saccade.dev.json"
RUNTIME="$RUNTIME_MACOS/saccade-runtime"
FIXTURE_URL="http://127.0.0.1:8765/fixtures/controls/all.html"
EXTENSION_BOOTSTRAP_URL="chrome-extension://bobfbgjplflcigednmccmbhlgclomgod/popup.html"
STRUCTURAL_FIXTURE_URL="http://127.0.0.1:8765/fixtures/structural/frames_and_shadow.html"
PUSHED_DELTA_URL="http://127.0.0.1:8765/fixtures/structural/pushed_delta.html"
TRUTH_LATENCY_URL="http://127.0.0.1:8765/fixtures/structural/truth_latency.html"
LIFECYCLE_URL="http://127.0.0.1:8765/fixtures/structural/lifecycle_gauntlet.html"
MOUSE_ACCURACY_URL="http://127.0.0.1:8765/fixtures/conformance/mouse_accuracy.html"
MOUSE_ACCURACY_LAYOUT="${SACCADE_MOUSE_ACCURACY_LAYOUT:-buttons}"
MOUSE_ACCURACY_DIFFICULTY="${SACCADE_MOUSE_ACCURACY_DIFFICULTY:-ordinary}"
MOUSE_ACCURACY_BACKEND="${SACCADE_MOUSE_ACCURACY_BACKEND:-native}"
REFLEX_URL="${SACCADE_REFLEX_URL:-https://mouseaccuracy.com/game}"
REFLEX_MAX_ACTIONS="${SACCADE_REFLEX_MAX_ACTIONS:-500}"
REFLEX_TIMEOUT_MS="${SACCADE_REFLEX_TIMEOUT_MS:-30000}"
FAIR_MODEL="${SACCADE_FAIR_MODEL:-}"
FAIR_EFFORT="${SACCADE_FAIR_EFFORT:-low}"
CODEX_BACKUP="$STATE_DIR/codex-saccade-backup.json"
PROFILE_BACKUP="$STATE_DIR/profile-before-test.json"
PROFILE_MISSING="$STATE_DIR/profile-was-missing"
INPUT_POLICY_BACKUP="$STATE_DIR/input-policy-before-test.json"
INPUT_POLICY_MISSING="$STATE_DIR/input-policy-was-missing"

mkdirs() {
  mkdir -p "$BIN_DIR" "$RUNTIME_MACOS" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$FIXTURE_ROOT" "$EXTENSION_ROOT"
  chmod 700 "$DEV_ROOT" "$BIN_DIR" "$RUNTIME_DIR" "$STATE_DIR" "$LOG_DIR" "$EVIDENCE_DIR" "$FIXTURE_ROOT" "$EXTENSION_ROOT"
  chmod 755 "$RUNTIME_APP" "$RUNTIME_APP/Contents" "$RUNTIME_MACOS"
}

migrate_browser_profiles() {
  if [ ! -e "$CHROME_PROFILE" ] && [ -d "$LEGACY_CHROME_PROFILE" ]; then
    mv "$LEGACY_CHROME_PROFILE" "$CHROME_PROFILE"
  fi
  if [ ! -e "$EDGE_PROFILE" ] && [ -d "$LEGACY_EDGE_PROFILE" ]; then
    mv "$LEGACY_EDGE_PROFILE" "$EDGE_PROFILE"
  fi
  mkdir -p "$CHROME_PROFILE" "$EDGE_PROFILE"
  chmod 700 "$CHROME_PROFILE" "$EDGE_PROFILE"
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
data.setdefault("extensions", {}).setdefault("ui", {})["developer_mode"] = True
safebrowsing = data.setdefault("safebrowsing", {})
safebrowsing["enabled"] = True
safebrowsing["enhanced"] = False
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
  signing_identity=${SACCADE_DEV_SIGNING_IDENTITY:--}
  signing_version="4:truth-layer:$signing_identity"
  cargo build --release --manifest-path "$ROOT/Cargo.toml" -p saccade-runtime
  source_hash=$(shasum -a 256 "$ROOT/target/release/saccade-runtime" | awk '{print $1}')
  installed_source_hash=$(sed -n '1p' "$STATE_DIR/runtime-source.sha256" 2>/dev/null || true)
  recorded_runtime_hash=$(sed -n '1p' "$STATE_DIR/runtime-installed.sha256" 2>/dev/null || true)
  actual_runtime_hash=
  if [ -x "$RUNTIME" ]; then
    actual_runtime_hash=$(shasum -a 256 "$RUNTIME" | awk '{print $1}')
  fi
  installed_signing_version=$(sed -n '1p' "$STATE_DIR/runtime-signing.version" 2>/dev/null || true)
  runtime_changed=false
  if [ ! -x "$RUNTIME" ] \
    || [ "$source_hash" != "$installed_source_hash" ] \
    || [ -z "$recorded_runtime_hash" ] \
    || [ "$actual_runtime_hash" != "$recorded_runtime_hash" ]; then
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
    shasum -a 256 "$RUNTIME" | awk '{print $1}' > "$STATE_DIR/runtime-installed.sha256"
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
    -- /usr/bin/python3 "$FIXTURE_ROOT/fixture_server.py" --port 8765 --bind 127.0.0.1 --directory "$FIXTURE_ROOT"
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
  cp "$ROOT/scripts/fixture_server.py" "$FIXTURE_ROOT/fixture_server.py"
  chmod -R u=rwX,go= "$FIXTURE_ROOT/fixtures"
  chmod 700 "$FIXTURE_ROOT/fixture_server.py"
}

install_extension() {
  source_expected="$STATE_DIR/source-extension-candidate.json"
  python3 "$ROOT/scripts/write_extension_candidate.py" \
    --extension-root "$ROOT/extension" \
    --expected "$source_expected"
  cp -R "$ROOT/extension/." "$EXTENSION_ROOT/"
  python3 "$ROOT/scripts/write_extension_candidate.py" \
    --extension-root "$EXTENSION_ROOT" \
    --expected "$RUNTIME_DIR/expected-extension-candidate.json"
  if ! cmp -s "$source_expected" "$RUNTIME_DIR/expected-extension-candidate.json"; then
    printf '%s\n' 'source and installed Extension candidates diverged' >&2
    exit 1
  fi
  chmod -R u=rwX,go= "$EXTENSION_ROOT"
}

verify_attached_extension_candidate() {
  python3 "$ROOT/scripts/verify_extension_candidate.py" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --expected "$RUNTIME_DIR/expected-extension-candidate.json"
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
  case "$start_browser_executable" in
    */Contents/MacOS/*) start_browser_app=${start_browser_executable%/Contents/MacOS/*} ;;
    *) printf '%s\n' "browser executable is not inside a macOS app: $start_browser_executable" >&2; exit 1 ;;
  esac
  /usr/bin/open -na "$start_browser_app" "$EXTENSION_BOOTSTRAP_URL" --args \
    --user-data-dir="$start_browser_profile" \
    --load-extension="$EXTENSION_ROOT" \
    --disable-extensions-except="$EXTENSION_ROOT" \
    --enable-logging \
    --log-file="$LOG_DIR/$start_browser_name.log" \
    '--vmodule=extensions*=1,native_message*=2' \
    --no-first-run \
    --no-default-browser-check \
    --disable-session-crashed-bubble \
    --window-position=24,52 \
    --window-size=800,747 \
    --new-window
  start_browser_count=0
  start_browser_pid=
  while [ "$start_browser_count" -lt 100 ]; do
    start_browser_pid=$(browser_profile_owner "$start_browser_name" || true)
    [ -n "$start_browser_pid" ] && break
    sleep 0.1
    start_browser_count=$((start_browser_count + 1))
  done
  if [ -z "$start_browser_pid" ]; then
    printf '%s\n' "browser profile did not report a PID for $start_browser_name" >&2
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

refresh_attached_native_hosts() {
  pgrep -f "$RUNTIME chrome-extension://" 2>/dev/null | while IFS= read -r attached_host_pid; do
    case "$attached_host_pid" in
      ''|*[!0-9]*) continue ;;
    esac
    attached_host_command=$(ps -p "$attached_host_pid" -o command= 2>/dev/null || true)
    case "$attached_host_command" in
      "$RUNTIME chrome-extension://"*) kill "$attached_host_pid" 2>/dev/null || true ;;
    esac
  done
}

suspend_ordinary_chrome_native_host() {
  suspended_manifest="$STATE_DIR/com.nanlogic.saccade.dev.chrome.suspended.json"
  chrome_manifest="$HOST_DIR_CHROME/com.nanlogic.saccade.dev.json"
  if [ -f "$chrome_manifest" ]; then
    if [ -f "$suspended_manifest" ]; then
      # install_native_manifest recreates every browser manifest on each up.
      # A prior suspension must remain effective across repeat test/up calls,
      # otherwise ordinary Chrome can reconnect and steal the single Host.
      rm -f "$chrome_manifest"
    else
      mv "$chrome_manifest" "$suspended_manifest"
    fi
  fi
  refresh_attached_native_hosts
}

restore_ordinary_chrome_native_host() {
  suspended_manifest="$STATE_DIR/com.nanlogic.saccade.dev.chrome.suspended.json"
  chrome_manifest="$HOST_DIR_CHROME/com.nanlogic.saccade.dev.json"
  if [ -f "$suspended_manifest" ]; then
    mkdir -p "$HOST_DIR_CHROME"
    mv "$suspended_manifest" "$chrome_manifest"
    chmod 600 "$chrome_manifest"
  fi
}

up() {
  up_browser=${1:-chrome}
  require_browser "$up_browser"
  mkdirs
  stop_browser "$up_browser"
  migrate_browser_profiles
  install_runtime
  install_native_manifest
  if [ "${SACCADE_SUSPEND_ORDINARY_CHROME_HOST:-0}" = 1 ] \
    || [ -f "$STATE_DIR/com.nanlogic.saccade.dev.chrome.suspended.json" ]; then
    suspend_ordinary_chrome_native_host
  fi
  install_fixtures
  install_extension
  ensure_browser "$up_browser"
  start_fixture
  stop_browser "$up_browser"
  start_browser "$up_browser"
  printf '%s\n' "Saccade Dev $up_browser is starting. Run ./scripts/dev.sh status, then ./scripts/dev.sh test $up_browser."
}

attach_existing_chrome() {
  mkdirs
  install_runtime
  install_native_manifest
  install_fixtures
  install_extension
  start_fixture
  refresh_attached_native_hosts
  verify_attached_extension_candidate
  printf '%s\n' "Saccade host is prepared for ordinary Chrome."
  printf '%s\n' "Agents should use saccade.tabs.open for known URLs; the new tab is Agent On automatically."
  printf '%s\n' "Existing Agent-Off tabs remain private unless the user explicitly shares that exact tab."
  printf '%s\n' "Do not start ./scripts/dev.sh up while testing Codex same-tab execution; it launches a separate managed browser instance."
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

restore_input_policy() {
  if [ -f "$INPUT_POLICY_BACKUP" ]; then
    cp "$INPUT_POLICY_BACKUP" "$RUNTIME_DIR/input-policy.json"
    chmod 600 "$RUNTIME_DIR/input-policy.json"
    rm -f "$INPUT_POLICY_BACKUP"
  elif [ -f "$INPUT_POLICY_MISSING" ]; then
    rm -f "$RUNTIME_DIR/input-policy.json" "$INPUT_POLICY_MISSING"
  fi
}

isolate_input_policy() {
  if [ -f "$RUNTIME_DIR/input-policy.json" ]; then
    cp "$RUNTIME_DIR/input-policy.json" "$INPUT_POLICY_BACKUP"
    chmod 600 "$INPUT_POLICY_BACKUP"
  else
    : > "$INPUT_POLICY_MISSING"
  fi
  rm -f "$RUNTIME_DIR/input-policy.json"
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
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$test_browser"; restore_profile; restore_input_policy; start_browser "$test_browser"' EXIT
  up "$test_browser"
  SACCADE_RUNTIME_DIR="$RUNTIME_DIR" "$RUNTIME" reference-actuator-repair
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
  start_browser "$test_browser"
  python3 "$ROOT/scripts/dev_probe.py" profile \
    --browser "$test_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$FIXTURE_URL" \
    --output "$test_run_dir/profile.json"
  stop_browser "$test_browser"
  restore_profile
  restore_input_policy
  start_browser "$test_browser"
  trap - EXIT HUP INT TERM
  printf '%s\n' "Cataloged-control $test_browser evidence: $test_run_dir"
}

truth_test_route() {
  truth_browser=$1
  truth_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$truth_browser"
  mkdirs
  trap 'stop_browser "$truth_browser"; restore_ordinary_chrome_native_host' EXIT
  SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$truth_browser"
  truth_run_dir="$EVIDENCE_DIR/$truth_stamp/$truth_browser/truth"
  mkdir -p "$truth_run_dir"
  chmod 700 "$truth_run_dir"
  python3 "$ROOT/scripts/probe_pushed_delta.py" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --url "$PUSHED_DELTA_URL" --output "$truth_run_dir/pushed-delta.json"
  python3 "$ROOT/scripts/probe_resource_subscription.py" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --url "$PUSHED_DELTA_URL" --output "$truth_run_dir/resource-subscription.json"
  python3 "$ROOT/scripts/probe_truth_latency.py" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --url "$TRUTH_LATENCY_URL" --single-p95-limit-ms 150 \
    --output "$truth_run_dir/latency.json"
  python3 "$ROOT/scripts/probe_control_truth.py" \
    --browser "$truth_browser" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --catalog "$ROOT/catalog/controls.json" \
    --url "$FIXTURE_URL" --output "$truth_run_dir/controls.json"
  python3 "$ROOT/scripts/probe_semantic_truth.py" \
    --browser "$truth_browser" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --inventory "$ROOT/catalog/truth_inventory.json" \
    --url "$FIXTURE_URL" --structure-url "$STRUCTURAL_FIXTURE_URL" \
    --output "$truth_run_dir/semantics.json"
  restore_ordinary_chrome_native_host
  trap - EXIT HUP INT TERM
  printf '%s\n' "Truth Layer $truth_browser evidence: $truth_run_dir"
}

truth_test_all() {
  truth_all_stamp=${1:-$(date -u '+%Y%m%dT%H%M%SZ')}
  truth_all_tmp=$(mktemp -d "${TMPDIR:-/tmp}/saccade-truth-all.XXXXXX")
  truth_all_cleanup() {
    down
    case "$truth_all_tmp" in "${TMPDIR:-/tmp}"/saccade-truth-all.*) rm -rf "$truth_all_tmp" ;; esac
  }
  trap truth_all_cleanup EXIT HUP INT TERM
  down
  mkdirs
  ensure_browser chrome
  ensure_browser edge
  python3 "$ROOT/scripts/write_candidate_manifest.py" \
    --chrome "$(sed -n '1p' "$(browser_path_file chrome)")" \
    --edge "$(sed -n '1p' "$(browser_path_file edge)")" \
    --output "$EVIDENCE_DIR/$truth_all_stamp/candidate.json"
  CHROME_PROFILE="$truth_all_tmp/chrome"
  EDGE_PROFILE="$truth_all_tmp/edge"
  mkdir -p "$CHROME_PROFILE" "$EDGE_PROFILE"
  truth_test_route chrome "$truth_all_stamp"
  truth_test_route edge "$truth_all_stamp"
  trap - EXIT HUP INT TERM
  truth_all_cleanup
  printf '%s\n' "Chrome and Edge clean-profile Truth evidence: $EVIDENCE_DIR/$truth_all_stamp"
}

latency_matrix() {
  latency_iterations=${1:-10}
  case "$latency_iterations" in
    ''|*[!0-9]*) printf '%s\n' 'latency matrix iterations must be an integer from 1 to 20' >&2; exit 2 ;;
  esac
  if [ "$latency_iterations" -lt 1 ] || [ "$latency_iterations" -gt 20 ]; then
    printf '%s\n' 'latency matrix iterations must be from 1 to 20' >&2
    exit 2
  fi
  latency_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  latency_run_dir="$EVIDENCE_DIR/$latency_stamp/latency-matrix"
  latency_tmp=$(mktemp -d "${TMPDIR:-/tmp}/saccade-latency-matrix.XXXXXX")
  latency_cleanup() {
    down
    case "$latency_tmp" in "${TMPDIR:-/tmp}"/saccade-latency-matrix.*) rm -rf "$latency_tmp" ;; esac
  }
  trap latency_cleanup EXIT HUP INT TERM
  mkdirs
  mkdir -p "$latency_run_dir"
  install_runtime
  install_native_manifest
  install_codex_mcp
  install_fixtures
  install_extension
  ensure_browser chrome
  ensure_browser edge
  start_fixture
  latency_round=1
  while [ "$latency_round" -le "$latency_iterations" ]; do
    if [ $((latency_round % 2)) -eq 1 ]; then latency_first=chrome; latency_second=edge
    else latency_first=edge; latency_second=chrome
    fi
    latency_position=first
    for latency_browser in "$latency_first" "$latency_second"; do
      latency_profile="$latency_tmp/round-$latency_round-$latency_browser"
      mkdir -p "$latency_profile"
      case "$latency_browser" in
        chrome) CHROME_PROFILE=$latency_profile ;;
        edge) EDGE_PROFILE=$latency_profile ;;
      esac
      start_browser "$latency_browser"
      python3 "$ROOT/scripts/wait_for_mcp.py" --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" --timeout 30
      python3 "$ROOT/scripts/probe_truth_latency.py" \
        --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" --url "$TRUTH_LATENCY_URL" \
        --output "$latency_run_dir/round-$(printf '%02d' "$latency_round")-$latency_position-$latency_browser.json"
      stop_browser "$latency_browser"
      latency_position=second
    done
    latency_round=$((latency_round + 1))
  done
  python3 "$ROOT/scripts/summarize_truth_latency_matrix.py" \
    --input "$latency_run_dir" --output "$latency_run_dir/report.json" --iterations "$latency_iterations"
  trap - EXIT HUP INT TERM
  latency_cleanup
  printf '%s\n' "Truth latency matrix evidence: $latency_run_dir/report.json"
}

test_all() {
  all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  test_route chrome "$all_stamp"
  test_route edge "$all_stamp"
  printf '%s\n' "Chrome and Edge evidence: $EVIDENCE_DIR/$all_stamp"
}

frames_route() {
  frames_browser=$1
  frames_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$frames_browser"
  mkdirs
  up "$frames_browser"
  frames_run_dir="$EVIDENCE_DIR/$frames_stamp/$frames_browser"
  mkdir -p "$frames_run_dir"
  chmod 700 "$frames_run_dir"
  python3 "$ROOT/scripts/dev_probe.py" frames \
    --browser "$frames_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --url "$STRUCTURAL_FIXTURE_URL" \
    --output "$frames_run_dir/frames_and_shadow.json"
  printf '%s\n' "Frame and shadow $frames_browser evidence: $frames_run_dir"
}

frames_all() {
  frames_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  frames_route chrome "$frames_stamp"
  frames_route edge "$frames_stamp"
}

lifecycle_route() {
  lifecycle_browser=$1
  lifecycle_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$lifecycle_browser"
  mkdirs
  trap 'stop_browser "$lifecycle_browser"; restore_ordinary_chrome_native_host' EXIT
  SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$lifecycle_browser"
  lifecycle_run_dir="$EVIDENCE_DIR/$lifecycle_stamp/$lifecycle_browser/truth"
  mkdir -p "$lifecycle_run_dir"
  chmod 700 "$lifecycle_run_dir"
  python3 "$ROOT/scripts/probe_lifecycle_truth.py" \
    --browser "$lifecycle_browser" \
    --runtime "$RUNTIME" --runtime-dir "$RUNTIME_DIR" \
    --url "$LIFECYCLE_URL" --output "$lifecycle_run_dir/lifecycle.json"
  restore_ordinary_chrome_native_host
  trap - EXIT HUP INT TERM
  printf '%s\n' "Lifecycle Truth $lifecycle_browser evidence: $lifecycle_run_dir/lifecycle.json"
}

lifecycle_all() {
  lifecycle_stamp=${1:-$(date -u '+%Y%m%dT%H%M%SZ')}
  trap down EXIT HUP INT TERM
  lifecycle_route chrome "$lifecycle_stamp"
  lifecycle_route edge "$lifecycle_stamp"
  trap - EXIT HUP INT TERM
  down
  printf '%s\n' "Chrome and Edge lifecycle evidence: $EVIDENCE_DIR/$lifecycle_stamp"
}

denominator_all() {
  denominator_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  truth_test_all "$denominator_stamp"
  lifecycle_all "$denominator_stamp"
  python3 "$ROOT/scripts/summarize_denominator_evidence.py" \
    --denominator "$ROOT/catalog/public_truth_cases.json" \
    --truth-root "$EVIDENCE_DIR/$denominator_stamp" \
    --lifecycle-root "$EVIDENCE_DIR/$denominator_stamp" \
    --output "$EVIDENCE_DIR/$denominator_stamp/denominator-63.json"
  printf '%s\n' "Chrome and Edge 63-row denominator evidence: $EVIDENCE_DIR/$denominator_stamp/denominator-63.json"
}

compare_route() {
  compare_browser=$1
  compare_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$compare_browser"
  mkdirs
  restore_profile
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$compare_browser"; restore_input_policy; start_browser "$compare_browser"' EXIT
  up "$compare_browser"
  compare_run_dir="$EVIDENCE_DIR/$compare_stamp/$compare_browser/external"
  compare_oracle_dir="$compare_run_dir/playwright"
  mkdir -p "$compare_run_dir" "$compare_oracle_dir"
  chmod 700 "$compare_run_dir" "$compare_oracle_dir"
  python3 "$ROOT/scripts/external_dogfood.py" \
    --browser "$compare_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --case w3c-radio --case w3c-switch --case w3c-tab --case w3c-menu-item \
    --output "$compare_run_dir/saccade.json"
  compare_package_dir="$ROOT/tests/reference/playwright"
  if [ ! -d "$compare_package_dir/node_modules/playwright" ]; then
    npm --prefix "$compare_package_dir" ci
  fi
  compare_executable=$(sed -n '1p' "$(browser_path_file "$compare_browser")")
  node "$compare_package_dir/oracle.cjs" \
    --browser "$compare_browser" \
    --executable "$compare_executable" \
    --output "$compare_oracle_dir"
  python3 "$ROOT/scripts/compare_external_evidence.py" \
    --saccade "$compare_run_dir/saccade.json" \
    --playwright "$compare_oracle_dir/oracle.json" \
    --output "$compare_run_dir/comparison.json"
  stop_browser "$compare_browser"
  restore_input_policy
  start_browser "$compare_browser"
  trap - EXIT HUP INT TERM
  printf '%s\n' "External Saccade/Playwright $compare_browser comparison: $compare_run_dir"
}

external_route() {
  external_browser=$1
  external_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$external_browser"
  mkdirs
  restore_profile
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$external_browser"; restore_input_policy; start_browser "$external_browser"' EXIT
  up "$external_browser"
  external_run_dir="$EVIDENCE_DIR/$external_stamp/$external_browser/public-suite"
  mkdir -p "$external_run_dir"
  chmod 700 "$external_run_dir"
  python3 "$ROOT/scripts/external_dogfood.py" \
    --browser "$external_browser" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --cases "$ROOT/catalog/external_cases.json" \
    --output "$external_run_dir/saccade.json"
  stop_browser "$external_browser"
  restore_input_policy
  start_browser "$external_browser"
  trap - EXIT HUP INT TERM
  printf '%s\n' "Public cross-site $external_browser evidence: $external_run_dir/saccade.json"
}

external_all() {
  external_all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  external_route chrome "$external_all_stamp"
  external_route edge "$external_all_stamp"
  printf '%s\n' "Chrome and Edge public cross-site evidence: $EVIDENCE_DIR/$external_all_stamp"
}

public_truth_route() {
  public_truth_browser=$1
  public_truth_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$public_truth_browser"
  mkdirs
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$public_truth_browser"; restore_input_policy; restore_ordinary_chrome_native_host; start_browser "$public_truth_browser"' EXIT
  SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$public_truth_browser"
  public_truth_run_dir="$EVIDENCE_DIR/$public_truth_stamp/$public_truth_browser/public-truth"
  mkdir -p "$public_truth_run_dir"
  chmod 700 "$public_truth_run_dir"
  python3 "$ROOT/scripts/probe_public_truth.py" \
    --browser "$public_truth_browser" \
    --browser-version "$("$(sed -n '1p' "$(browser_path_file "$public_truth_browser")")" --version)" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --cases "$ROOT/catalog/external_cases.json" \
    --extra-cases "$ROOT/catalog/public_truth_extra_cases.json" \
    --output "$public_truth_run_dir/saccade.json"
  stop_browser "$public_truth_browser"
  restore_input_policy
  restore_ordinary_chrome_native_host
  start_browser "$public_truth_browser"
  trap - EXIT HUP INT TERM
  printf '%s\n' "Default public Truth $public_truth_browser evidence: $public_truth_run_dir/saccade.json"
}

public_truth_all() {
  public_truth_all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  public_truth_route chrome "$public_truth_all_stamp"
  public_truth_route edge "$public_truth_all_stamp"
  printf '%s\n' "Chrome and Edge default public Truth evidence: $EVIDENCE_DIR/$public_truth_all_stamp"
}

fair_task_path() {
  case "$1" in
    selenium) printf '%s\n' "$ROOT/benchmarks/tasks/selenium_web_form.json" ;;
    demoqa) printf '%s\n' "$ROOT/benchmarks/tasks/demoqa_react_practice_form.json" ;;
    angular) printf '%s\n' "$ROOT/benchmarks/tasks/angular_material_select.json" ;;
    *) printf '%s\n' "fair task must be selenium, demoqa, or angular" >&2; return 2 ;;
  esac
}

fair_route() {
  fair_task=$1
  fair_order=$2
  fair_stamp=${3:-$(date -u '+%Y%m%dT%H%M%SZ')}
  fair_task_file=$(fair_task_path "$fair_task")
  mkdirs
  restore_profile
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser chrome; restore_input_policy; start_browser chrome' EXIT
  stop_browser edge
  up chrome
  python3 "$ROOT/scripts/wait_for_mcp.py" \
    --runtime "$RUNTIME" \
    --runtime-dir "$RUNTIME_DIR" \
    --timeout 30
  fair_run_dir="$EVIDENCE_DIR/$fair_stamp/fair-$fair_task-$fair_order"
  mkdir -p "$fair_run_dir"
  chmod 700 "$fair_run_dir"
  fair_status=0
  set -- python3 "$ROOT/scripts/benchmark_agent_fair.py" \
      --task "$fair_task_file" \
      --runtime "$RUNTIME" \
      --runtime-dir "$RUNTIME_DIR" \
      --effort "$FAIR_EFFORT" \
      --output "$fair_run_dir" \
      --order "$fair_order"
  [ -z "$FAIR_MODEL" ] || set -- "$@" --model "$FAIR_MODEL"
  "$@" || fair_status=$?
  stop_browser chrome
  restore_input_policy
  start_browser chrome
  trap - EXIT HUP INT TERM
  printf '%s\n' "Same-Codex fair $fair_task/$fair_order (${FAIR_MODEL:-default}/$FAIR_EFFORT) evidence: $fair_run_dir/report.json"
  return "$fair_status"
}

fair_both() {
  fair_both_task=$1
  fair_both_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  fair_both_status=0
  fair_route "$fair_both_task" saccade-first "$fair_both_stamp" || fair_both_status=1
  fair_route "$fair_both_task" playwright-first "$fair_both_stamp" || fair_both_status=1
  printf '%s\n' "Order-reversed fair $fair_both_task evidence: $EVIDENCE_DIR/$fair_both_stamp"
  return "$fair_both_status"
}

compare_all() {
  compare_all_stamp=$(date -u '+%Y%m%dT%H%M%SZ')
  compare_route chrome "$compare_all_stamp"
  compare_route edge "$compare_all_stamp"
  printf '%s\n' "Chrome and Edge external comparisons: $EVIDENCE_DIR/$compare_all_stamp"
}

accuracy_route() {
  accuracy_browser=$1
  accuracy_stamp=${2:-$(date -u '+%Y%m%dT%H%M%SZ')}
  require_browser "$accuracy_browser"
  mkdirs
  restore_profile
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$accuracy_browser"; restore_input_policy; start_browser "$accuracy_browser"' EXIT
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
  stop_browser "$accuracy_browser"
  restore_input_policy
  start_browser "$accuracy_browser"
  trap - EXIT HUP INT TERM
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
  restore_input_policy
  isolate_input_policy
  trap 'stop_browser "$reflex_browser"; restore_input_policy; start_browser "$reflex_browser"' EXIT
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
  stop_browser "$reflex_browser"
  restore_input_policy
  start_browser "$reflex_browser"
  trap - EXIT HUP INT TERM
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
  restore_ordinary_chrome_native_host
  rm -f "$STATE_DIR/active-browser"
  restore_profile
  restore_input_policy
  printf '%s\n' "Saccade Dev processes stopped. Codex MCP configuration was left unchanged."
}

mcp_command() {
  mcp_action=${2:-install}
  case "$mcp_action" in
    install) mkdirs; install_runtime; install_codex_mcp ;;
    restore)
      [ -f "$STATE_DIR/codex-path" ] || { printf '%s\n' 'no recorded Codex MCP installation to restore' >&2; exit 2; }
      codex=$(sed -n '1p' "$STATE_DIR/codex-path")
      python3 "$ROOT/scripts/dev_codex_config.py" restore \
        --codex "$codex" \
        --backup "$CODEX_BACKUP"
      ;;
    *) printf '%s\n' 'mcp action must be install or restore' >&2; exit 2 ;;
  esac
}

profile_command() {
  profile_action=${2:-show}
  case "$profile_action" in
    show)
      python3 "$ROOT/scripts/dev_profile.py" show --runtime-dir "$RUNTIME_DIR" --profiles-dir "$ROOT/profiles"
      ;;
    set)
      [ -n "${3:-}" ] || { printf '%s\n' 'profile set requires a profile name or JSON path' >&2; exit 2; }
      python3 "$ROOT/scripts/dev_profile.py" set --runtime-dir "$RUNTIME_DIR" --profiles-dir "$ROOT/profiles" --profile "$3"
      profile_active=$(sed -n '1p' "$STATE_DIR/active-browser" 2>/dev/null || true)
      if [ -n "$profile_active" ]; then stop_browser "$profile_active"; start_browser "$profile_active"; fi
      ;;
    reset)
      python3 "$ROOT/scripts/dev_profile.py" reset --runtime-dir "$RUNTIME_DIR" --profiles-dir "$ROOT/profiles"
      profile_active=$(sed -n '1p' "$STATE_DIR/active-browser" 2>/dev/null || true)
      if [ -n "$profile_active" ]; then stop_browser "$profile_active"; start_browser "$profile_active"; fi
      ;;
    *) printf '%s\n' 'profile action must be show, set, or reset' >&2; exit 2 ;;
  esac
}

case "${1:-}" in
  up) up "${2:-chrome}" ;;
  attach) attach_existing_chrome ;;
  test)
    case "${2:-chrome}" in
      all) truth_test_all ;;
      chrome|edge) truth_test_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  test-actuator)
    case "${2:-chrome}" in
      all) test_all ;;
      chrome|edge) test_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  frames)
    case "${2:-chrome}" in
      all) frames_all ;;
      chrome|edge) frames_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  compare)
    case "${2:-chrome}" in
      all) compare_all ;;
      chrome|edge) compare_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  external)
    case "${2:-chrome}" in
      all) external_all ;;
      chrome|edge) external_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  public-truth)
    case "${2:-chrome}" in
      all) public_truth_all ;;
      chrome|edge) public_truth_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  lifecycle)
    case "${2:-all}" in
      chrome|edge) lifecycle_route "$2" ;;
      all) lifecycle_all ;;
      *) printf '%s\n' 'lifecycle browser must be chrome, edge, or all' >&2; exit 2 ;;
    esac
    ;;
  denominator) denominator_all ;;
  fair)
    case "${3:-both}" in
      both) fair_both "${2:-selenium}" ;;
      saccade-first|playwright-first) fair_route "${2:-selenium}" "$3" ;;
      *) printf '%s\n' "fair order must be both, saccade-first, or playwright-first" >&2; exit 2 ;;
    esac
    ;;
  latency-matrix) latency_matrix "${2:-10}" ;;
  accuracy)
    case "${2:-chrome}" in
      all) accuracy_all ;;
      chrome|edge) accuracy_route "${2:-chrome}" ;;
      *) printf '%s\n' "browser must be chrome, edge, or all" >&2; exit 2 ;;
    esac
    ;;
  reflex) reflex_route "${2:-chrome}" "${3:-soft}" ;;
  mcp) mcp_command "$@" ;;
  profile) profile_command "$@" ;;
  status) status ;;
  down) down ;;
  *) printf '%s\n' "usage: ./scripts/dev.sh <up [chrome|edge]|attach|mcp <install|restore>|test [chrome|edge|all]|lifecycle [chrome|edge|all]|latency-matrix [1-20]|test-actuator [chrome|edge|all]|frames [chrome|edge|all]|external [chrome|edge|all]|public-truth [chrome|edge|all]|compare [chrome|edge|all]|fair [selenium|demoqa|angular] [both|saccade-first|playwright-first]|accuracy [chrome|edge|all]|reflex [chrome|edge] [native|soft]|profile <show|set NAME_OR_PATH|reset>|status|down>" >&2; exit 2 ;;
esac
