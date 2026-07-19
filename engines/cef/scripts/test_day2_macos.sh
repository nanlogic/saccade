#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
APP=${SACCADE_CEF_APP:-$REPO_ROOT/target/cef-release/Saccade.app}
MCP="$REPO_ROOT/target/debug/saccade-mcp"
FIXTURE="file://$REPO_ROOT/fixtures/cef_adapter_lifecycle.html"

[ -x "$APP/Contents/MacOS/Saccade" ] || {
  echo "Missing Day 2 CEF app; run engines/cef/scripts/build_macos.sh" >&2
  exit 1
}

cargo build -q -p saccade-mcp

run_host() {
  host=$1
  session=$(mktemp -d "/tmp/saccade-day2-${host}.XXXXXX")
  log="$REPO_ROOT/target/cef-release/day2-${host}.log"
  SACCADE_ENGINE_SESSION_DIR="$session" \
    SACCADE_PROFILE_NAME="day2-${host}" \
    "$SCRIPT_DIR/run_adapter_macos.sh" incognito "$FIXTURE" \
      --use-mock-keychain --use-views --initial-show-state=hidden >"$log" 2>&1 &
  browser_pid=$!

  ready=false
  for _ in $(seq 1 200); do
    if [ -s "$session/grant.json" ] && \
       jq -e '.url != ""' "$session/grant.json" >/dev/null 2>&1; then
      ready=true
      break
    fi
    if ! kill -0 "$browser_pid" 2>/dev/null; then
      cat "$log" >&2
      echo "CEF exited before writing a usable grant" >&2
      exit 1
    fi
    sleep 0.1
  done
  [ "$ready" = true ] || {
    kill "$browser_pid" 2>/dev/null || true
    echo "Timed out waiting for CEF owner grant" >&2
    exit 1
  }

  case "$host" in
    python)
      SACCADE_GRANT_PATH="$session/grant.json" \
        SACCADE_LIFECYCLE_ONLY=1 \
        SACCADE_NAVIGATE_URL="$FIXTURE?navigated=$host" \
        SACCADE_MCP_COMMAND="$MCP serve-stdio" \
        python3 "$REPO_ROOT/docs/integration_examples/python-host/main.py"
      ;;
    typescript)
      SACCADE_GRANT_PATH="$session/grant.json" \
        SACCADE_LIFECYCLE_ONLY=1 \
        SACCADE_NAVIGATE_URL="$FIXTURE?navigated=$host" \
        SACCADE_MCP_COMMAND="$MCP serve-stdio" \
        npx --yes tsx "$REPO_ROOT/docs/integration_examples/typescript-host/index.ts"
      ;;
  esac

  if ! wait "$browser_pid"; then
    cat "$log" >&2
    echo "CEF adapter host exited unsuccessfully for $host" >&2
    exit 1
  fi
  [ ! -e "$session" ] || {
    echo "Private adapter session was not removed: $session" >&2
    exit 1
  }
}

run_host python
run_host typescript
printf 'DAY2_ENGINE_ADAPTER_GATE=PASS\n'
