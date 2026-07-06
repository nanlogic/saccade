#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${1:-$ROOT/dist/saccade-dogfood-$STAMP}"
SERVOSHELL_BIN="${SACCADE_SERVOSHELL_BIN:-/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell}"
OWNED_DOMAINS="${SACCADE_OWNED_DOMAINS:-nanmesh.ai,mythcastera.com,mysterypartynow.com}"
SERVOSHELL_USERSCRIPTS_DIR="${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}"
DEFAULT_PROFILE_DIR="${SACCADE_PROFILE_DIR:-$ROOT/runs/dogfood_profile/default}"
DEFAULT_PROFILE_ROOT="${SACCADE_PROFILE_ROOT:-$ROOT/runs/dogfood_profile}"
INCLUDE_LEGACY_SHELL="${SACCADE_INCLUDE_LEGACY_SHELL:-0}"
BUILD_TIME_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RELEASE_COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
RELEASE_BRANCH="$(git -C "$ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
mkdir -p "$OUT/bin" "$OUT/docs" "$OUT/lib" "$OUT/profile/default" "$OUT/userscripts"
mkdir -p "$DEFAULT_PROFILE_DIR"

packages=(-p saccade-mcp -p saccade-servoshell)
if [[ "$INCLUDE_LEGACY_SHELL" == "1" ]]; then
  packages+=(-p saccade-shell)
fi

cargo build --release "${packages[@]}"

cp "$ROOT/target/release/saccade-mcp" "$OUT/bin/"
cp "$ROOT/target/release/saccade-servoshell" "$OUT/bin/"
if [[ "$INCLUDE_LEGACY_SHELL" == "1" ]]; then
  cp "$ROOT/target/release/saccade-shell" "$OUT/bin/"
fi
cp "$ROOT/scripts/read_article_fallback.py" "$OUT/lib/"
cp "$ROOT/scripts/run_ai020_live_draft.py" "$OUT/lib/"

cat > "$OUT/saccade-dogfood.env" <<ENV
SACCADE_RELEASE_KIND=dogfood
SACCADE_ROOT=$ROOT
SACCADE_DOGFOOD_DIR=$OUT
SACCADE_RELEASE_COMMIT=$RELEASE_COMMIT
SACCADE_RELEASE_BRANCH=$RELEASE_BRANCH
SACCADE_RELEASE_BUILD_TIME_UTC=$BUILD_TIME_UTC
SACCADE_SERVOSHELL_BIN=$SERVOSHELL_BIN
SACCADE_SERVOSHELL_USERSCRIPTS_DIR=\${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-$SERVOSHELL_USERSCRIPTS_DIR}
SACCADE_PROFILE_ROOT=\${SACCADE_PROFILE_ROOT:-$DEFAULT_PROFILE_ROOT}
SACCADE_PROFILE_NAME=\${SACCADE_PROFILE_NAME:-default}
SACCADE_PROFILE_DIR=\${SACCADE_PROFILE_DIR:-\${SACCADE_PROFILE_ROOT}/\${SACCADE_PROFILE_NAME}}
SACCADE_PROFILE_MODE=\${SACCADE_PROFILE_MODE:-normal}
SACCADE_PROFILE_ACTIONS_DIR=\${SACCADE_PROFILE_ACTIONS_DIR:-$OUT/runs/profile_actions}
SACCADE_INCOGNITO=\${SACCADE_INCOGNITO:-0}
SACCADE_INCOGNITO_BASE_DIR=\${SACCADE_INCOGNITO_BASE_DIR:-$OUT/runs/incognito}
SACCADE_OWNED_DOMAINS=$OWNED_DOMAINS
SACCADE_INCLUDE_LEGACY_SHELL=$INCLUDE_LEGACY_SHELL
RUST_LOG=error
ENV

cat > "$OUT/lib/profile.sh" <<'SH'
# Shared dogfood profile resolver. Source after saccade-dogfood.env.
saccade_validate_profile_name() {
  local name="${SACCADE_PROFILE_NAME:-default}"
  case "$name" in
    ""|.|..|*/*|*\\*|*:*|*" "*)
      echo "invalid SACCADE_PROFILE_NAME=$name; use a short name like default, work, or test" >&2
      exit 2
      ;;
  esac
}

saccade_mark_profile() {
  local mode="$1"
  local persistent="$2"
  local dir="$3"
  mkdir -p "$dir"
  python3 - "$dir/.saccade-profile.json" "$mode" "$persistent" "${SACCADE_PROFILE_NAME:-default}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = {
    "kind": "saccade_browser_profile",
    "mode": sys.argv[2],
    "persistent": sys.argv[3] == "1",
    "name": sys.argv[4],
}
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

saccade_resolve_profile() {
  local requested="${SACCADE_PROFILE_MODE:-normal}"
  saccade_validate_profile_name
  case "${SACCADE_INCOGNITO:-0}" in
    1|true|TRUE|yes|YES|on|ON) requested="incognito" ;;
  esac

  case "$requested" in
    ""|normal|default)
      SACCADE_EFFECTIVE_PROFILE_MODE="normal"
      SACCADE_EFFECTIVE_PROFILE_DIR="$SACCADE_PROFILE_DIR"
      SACCADE_EFFECTIVE_PROFILE_PERSISTENT=1
      SACCADE_PROFILE_CLEANUP_DIR=""
      SACCADE_PROFILE_ACTIONS_PATH="$SACCADE_PROFILE_ACTIONS_DIR/$SACCADE_PROFILE_NAME.json"
      mkdir -p "$SACCADE_EFFECTIVE_PROFILE_DIR"
      mkdir -p "$SACCADE_PROFILE_ACTIONS_DIR"
      saccade_mark_profile "normal" "1" "$SACCADE_EFFECTIVE_PROFILE_DIR"
      ;;
    incognito|private|ephemeral)
      SACCADE_EFFECTIVE_PROFILE_MODE="incognito"
      SACCADE_EFFECTIVE_PROFILE_PERSISTENT=0
      mkdir -p "$SACCADE_INCOGNITO_BASE_DIR"
      SACCADE_EFFECTIVE_PROFILE_DIR="$(mktemp -d "$SACCADE_INCOGNITO_BASE_DIR/profile_XXXXXXXX")"
      SACCADE_PROFILE_CLEANUP_DIR="$SACCADE_EFFECTIVE_PROFILE_DIR"
      SACCADE_PROFILE_ACTIONS_PATH=""
      : > "$SACCADE_PROFILE_CLEANUP_DIR/.saccade-incognito-profile"
      saccade_mark_profile "incognito" "0" "$SACCADE_EFFECTIVE_PROFILE_DIR"
      ;;
    *)
      echo "unknown SACCADE_PROFILE_MODE=$requested; expected normal or incognito" >&2
      exit 2
      ;;
  esac

  export SACCADE_EFFECTIVE_PROFILE_MODE
  export SACCADE_EFFECTIVE_PROFILE_DIR
  export SACCADE_EFFECTIVE_PROFILE_PERSISTENT
  export SACCADE_PROFILE_CLEANUP_DIR
  export SACCADE_PROFILE_ACTIONS_PATH
}

saccade_cleanup_profile() {
  local dir="${SACCADE_PROFILE_CLEANUP_DIR:-}"
  if [[ -n "$dir" && -f "$dir/.saccade-incognito-profile" ]]; then
    rm -rf -- "$dir"
  fi
}

saccade_run_with_profile_cleanup() {
  local status=0
  set +e
  "$@"
  status=$?
  set -e
  saccade_apply_profile_action_requests
  saccade_cleanup_profile
  return "$status"
}

saccade_apply_profile_action_requests() {
  local request="${SACCADE_PROFILE_ACTIONS_PATH:-}"
  if [[ -z "$request" || ! -f "$request" ]]; then
    return 0
  fi
  if [[ "${SACCADE_EFFECTIVE_PROFILE_MODE:-}" != "normal" || "${SACCADE_EFFECTIVE_PROFILE_PERSISTENT:-0}" != "1" ]]; then
    echo "Ignoring Saccade profile action request outside normal persistent mode: $request" >&2
    rm -f -- "$request"
    return 0
  fi

  python3 - "$request" "$SACCADE_EFFECTIVE_PROFILE_DIR" "$SACCADE_PROFILE_ROOT" "$SACCADE_PROFILE_NAME" <<'PY'
import json
import shutil
import sys
from pathlib import Path

request_path = Path(sys.argv[1]).expanduser().resolve()
profile_dir = Path(sys.argv[2]).expanduser().resolve()
profile_root = Path(sys.argv[3]).expanduser().resolve()
profile_name = sys.argv[4]
result_path = request_path.with_suffix(".result.json")

def write_result(payload):
    result_path.parent.mkdir(parents=True, exist_ok=True)
    result_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")

try:
    request = json.loads(request_path.read_text(encoding="utf-8"))
except Exception as error:
    write_result({"ok": False, "action": "ignored_invalid_request", "error": str(error)})
    request_path.unlink(missing_ok=True)
    raise SystemExit(0)

if request.get("action") != "clear_profile_on_quit" or request.get("confirmed") is not True:
    write_result({"ok": False, "action": "ignored", "reason": "unsupported or unconfirmed request"})
    request_path.unlink(missing_ok=True)
    raise SystemExit(0)

home = Path.home().resolve()
cwd = Path.cwd().resolve()
default_child = profile_dir.parent == profile_root and profile_dir.name == profile_name
refuse_reasons = []
if profile_dir in (Path("/").resolve(), home, cwd):
    refuse_reasons.append("target is too broad")
if not default_child:
    refuse_reasons.append("target is not the named profile under SACCADE_PROFILE_ROOT")
if not profile_dir.exists():
    write_result({
        "ok": True,
        "action": "noop_missing_profile",
        "profile_dir": str(profile_dir),
        "profile_name": profile_name,
    })
    request_path.unlink(missing_ok=True)
    raise SystemExit(0)
if refuse_reasons:
    write_result({
        "ok": False,
        "action": "refused",
        "profile_dir": str(profile_dir),
        "profile_name": profile_name,
        "reasons": refuse_reasons,
    })
    request_path.unlink(missing_ok=True)
    raise SystemExit(0)

children = [child for child in profile_dir.iterdir()]
file_count = 0
size_bytes = 0
for path in profile_dir.rglob("*"):
    try:
        if path.is_file():
            file_count += 1
            size_bytes += path.stat().st_size
    except OSError:
        pass

for child in children:
    if child.is_dir() and not child.is_symlink():
        shutil.rmtree(child)
    else:
        child.unlink(missing_ok=True)
profile_dir.mkdir(parents=True, exist_ok=True)
(profile_dir / ".saccade-profile.json").write_text(
    json.dumps(
        {
            "kind": "saccade_browser_profile",
            "mode": "normal",
            "persistent": True,
            "name": profile_name,
            "cleared": True,
            "cleared_by": "saccade_clear_on_quit",
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
request_path.unlink(missing_ok=True)
write_result({
    "ok": True,
    "action": "clear_profile_on_quit",
    "profile_dir": str(profile_dir),
    "profile_name": profile_name,
    "removed_entries": len(children),
    "removed_files": file_count,
    "removed_bytes": size_bytes,
    "raw_cookies_printed": False,
    "raw_storage_printed": False,
})
PY
  local result="${request%.json}.result.json"
  if [[ -f "$result" ]]; then
    echo "Saccade profile action result: $result" >&2
  else
    echo "Saccade profile action request was processed: $request" >&2
  fi
}
SH

cat > "$OUT/open-saccade" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
URL="${1:-https://example.com}"
cd "$SACCADE_ROOT"
saccade_resolve_profile
echo "Opening Saccade dogfood browser..." >&2
echo "Target: $URL" >&2
echo "Profile: $SACCADE_EFFECTIVE_PROFILE_MODE ($SACCADE_EFFECTIVE_PROFILE_DIR)" >&2
echo "A local launch page appears first; the bridge will navigate to the target after it attaches." >&2
userscripts_extra=()
if [[ -n "${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}" ]]; then
  userscripts_extra+=(--userscripts-dir "$SACCADE_SERVOSHELL_USERSCRIPTS_DIR")
fi
saccade_run_with_profile_cleanup "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR" \
  ${userscripts_extra[@]+"${userscripts_extra[@]}"} \
  --no-headless \
  --output-dir "$DIR/runs/servoshell_bridge" \
  --grant-path "$DIR/current_tab_grant.json"
SH

cat > "$OUT/servoshell-bridge" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
cd "$SACCADE_ROOT"
extra=()
has_arg() {
  local name="$1"
  shift
  local arg
  for arg in "$@"; do
    case "$arg" in
      "$name"|"$name"=*) return 0 ;;
    esac
  done
  return 1
}
if has_arg --profile-dir "$@"; then
  SACCADE_EFFECTIVE_PROFILE_MODE="custom"
  SACCADE_EFFECTIVE_PROFILE_PERSISTENT=1
  SACCADE_PROFILE_CLEANUP_DIR=""
  SACCADE_PROFILE_ACTIONS_PATH=""
  export SACCADE_EFFECTIVE_PROFILE_MODE SACCADE_EFFECTIVE_PROFILE_PERSISTENT SACCADE_PROFILE_CLEANUP_DIR SACCADE_PROFILE_ACTIONS_PATH
else
  saccade_resolve_profile
  extra+=(--profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR")
fi
has_arg --grant-path "$@" || extra+=(--grant-path "$DIR/current_tab_grant.json")
has_arg --output-dir "$@" || extra+=(--output-dir "$DIR/runs/servoshell_bridge")
if [[ -n "${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}" ]] && ! has_arg --userscripts-dir "$@"; then
  extra+=(--userscripts-dir "$SACCADE_SERVOSHELL_USERSCRIPTS_DIR")
fi
saccade_run_with_profile_cleanup "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  "${extra[@]}" \
  "$@"
SH

cat > "$OUT/check-saccade" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"

if [[ ! -x "$DIR/bin/saccade-servoshell" ]]; then
  echo "missing bundled saccade-servoshell: $DIR/bin/saccade-servoshell" >&2
  exit 2
fi
if [[ ! -x "$SACCADE_SERVOSHELL_BIN" ]]; then
  echo "missing ServoShell binary: $SACCADE_SERVOSHELL_BIN" >&2
  echo "Set SACCADE_SERVOSHELL_BIN to a source-release ServoShell 0.3 binary and retry." >&2
  exit 2
fi

cd "$SACCADE_ROOT"
SMOKE_URL="file://$SACCADE_ROOT/test_pages/browser_session/index.html"
saccade_resolve_profile
echo "Saccade dogfood kit: $DIR" >&2
echo "Saccade commit: ${SACCADE_RELEASE_COMMIT:-unknown}" >&2
echo "ServoShell: $SACCADE_SERVOSHELL_BIN" >&2
echo "Profile: $SACCADE_EFFECTIVE_PROFILE_MODE ($SACCADE_EFFECTIVE_PROFILE_DIR)" >&2
userscripts_extra=()
if [[ -n "${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}" ]]; then
  userscripts_extra+=(--userscripts-dir "$SACCADE_SERVOSHELL_USERSCRIPTS_DIR")
fi
saccade_run_with_profile_cleanup "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$SMOKE_URL" \
  --profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR" \
  ${userscripts_extra[@]+"${userscripts_extra[@]}"} \
  --grant-path "$DIR/current_tab_grant.json" \
  --output-dir "$DIR/runs/check/bridge_smoke" \
  --smoke \
  --json \
  --timeout-sec "${SACCADE_CHECK_TIMEOUT_SEC:-35}"
SH

cat > "$OUT/read-article" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
URL="${1:?usage: read-article <url> [output_name]}"
NAME="${2:-article_$(date +%Y%m%d-%H%M%S)}"
cd "$SACCADE_ROOT"
saccade_resolve_profile
userscripts_extra=()
if [[ -n "${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}" ]]; then
  userscripts_extra+=(--userscripts-dir "$SACCADE_SERVOSHELL_USERSCRIPTS_DIR")
fi
saccade_run_with_profile_cleanup python3 "$DIR/lib/read_article_fallback.py" \
  --bin "$DIR/bin/saccade-servoshell" \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR" \
  ${userscripts_extra[@]+"${userscripts_extra[@]}"} \
  --article-max-chars "${SACCADE_ARTICLE_MAX_CHARS:-30000}" \
  --timeout-sec "${SACCADE_READ_ARTICLE_TIMEOUT_SEC:-35}" \
  --hard-timeout-sec "${SACCADE_READ_ARTICLE_HARD_TIMEOUT_SEC:-50}" \
  --http-timeout-sec "${SACCADE_READ_ARTICLE_HTTP_TIMEOUT_SEC:-20}" \
  --fallback "${SACCADE_READ_ARTICLE_FALLBACK:-auto}" \
  --cwd "$SACCADE_ROOT" \
  --grant-path "$DIR/runs/article/$NAME/current_tab_grant.json" \
  --output-dir "$DIR/runs/article/$NAME"
SH

cat > "$OUT/run-formmax" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
NAME="${1:-formmax_$(date +%Y%m%d-%H%M%S)}"
cd "$SACCADE_ROOT"
exec "$DIR/bin/saccade-servoshell" formmax-selftest \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --output-dir "$DIR/runs/formmax/$NAME" \
  --timeout-sec "${SACCADE_FORMMAX_TIMEOUT_SEC:-35}"
SH

cat > "$OUT/run-local-game-reflex" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
URL="${1:-http://127.0.0.1:4173/}"
NAME="${2:-dogfood_reflex_$(date +%Y%m%d-%H%M%S)}"
cd "$SACCADE_ROOT"
node "$SACCADE_ROOT/scripts/run_local_game_reflex_loop.js" \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --headless \
  --window-size "${SACCADE_REFLEX_WINDOW_SIZE:-1280x900}" \
  --duration-ms "${SACCADE_REFLEX_DURATION_MS:-15000}" \
  --policy visual \
  --visual-fact-interval-ms "${SACCADE_REFLEX_FACT_INTERVAL_MS:-1000}" \
  --output-dir "$SACCADE_ROOT/runs/local_game_reflex/$NAME"
SH

cat > "$OUT/run-ai020-live-draft" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
cd "$SACCADE_ROOT"
saccade_resolve_profile
userscripts_extra=()
if [[ -n "${SACCADE_SERVOSHELL_USERSCRIPTS_DIR:-}" ]]; then
  userscripts_extra+=(--userscripts-dir "$SACCADE_SERVOSHELL_USERSCRIPTS_DIR")
fi
saccade_run_with_profile_cleanup python3 "$DIR/lib/run_ai020_live_draft.py" \
  --bin "$DIR/bin/saccade-servoshell" \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR" \
  ${userscripts_extra[@]+"${userscripts_extra[@]}"} \
  "$@"
SH

cat > "$OUT/profile-status" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
saccade_resolve_profile
trap saccade_cleanup_profile EXIT
python3 - "$DIR" <<'PY'
import json
import os
import sys
from pathlib import Path

dogfood_dir = Path(sys.argv[1])
profile_dir = Path(os.environ["SACCADE_EFFECTIVE_PROFILE_DIR"]).expanduser()
profile_root = Path(os.environ.get("SACCADE_PROFILE_ROOT", "")).expanduser()
grant_path = dogfood_dir / "current_tab_grant.json"
marker_path = profile_dir / ".saccade-profile.json"
actions_path_raw = os.environ.get("SACCADE_PROFILE_ACTIONS_PATH", "")
actions_path = Path(actions_path_raw).expanduser() if actions_path_raw else None

file_count = 0
size_bytes = 0
if profile_dir.exists():
    for path in profile_dir.rglob("*"):
        try:
            if path.is_file():
                file_count += 1
                size_bytes += path.stat().st_size
        except OSError:
            pass

payload = {
    "ok": True,
    "dogfood_dir": str(dogfood_dir),
    "profile": {
        "mode": os.environ["SACCADE_EFFECTIVE_PROFILE_MODE"],
        "name": os.environ.get("SACCADE_PROFILE_NAME", "default"),
        "persistent": os.environ["SACCADE_EFFECTIVE_PROFILE_PERSISTENT"] in ("1", "true", "yes", "on"),
        "dir": str(profile_dir),
        "root": str(profile_root) if str(profile_root) else None,
        "marker_exists": marker_path.exists(),
        "cookie_jar_exists": (profile_dir / "cookie_jar.json").exists(),
        "file_count": file_count,
        "size_bytes": size_bytes,
        "clear_on_quit_request_path": str(actions_path) if actions_path else None,
        "clear_on_quit_pending": actions_path.exists() if actions_path else False,
    },
    "agent": {
        "grant_path": str(grant_path),
        "grant_exists": grant_path.exists(),
        "grants_are_session_scoped": True,
        "raw_cookies_exposed": False,
        "sensitive_values_exposed": False,
    },
    "commands": {
        "open": f"{dogfood_dir}/open-saccade https://example.com",
        "incognito": f"SACCADE_PROFILE_MODE=incognito {dogfood_dir}/open-saccade https://example.com",
        "clear_profile": f"{dogfood_dir}/clear-profile --yes",
    },
}
print(json.dumps(payload, indent=2, sort_keys=True))
PY
SH

cat > "$OUT/clear-profile" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"

YES=0
DRY_RUN=0
FORCE_CUSTOM=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes|-y)
      YES=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --force-custom)
      FORCE_CUSTOM=1
      shift
      ;;
    --help|-h)
      cat <<HELP
usage: clear-profile [--yes] [--dry-run] [--force-custom]

Deletes the current persistent Saccade browser profile contents after a safety
check. This signs sites out. It never prints cookies or storage values.

By default it clears the resolved normal profile under SACCADE_PROFILE_ROOT.
Custom profile dirs require --force-custom.
HELP
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

saccade_resolve_profile
if [[ "$SACCADE_EFFECTIVE_PROFILE_MODE" != "normal" ]]; then
  echo "clear-profile only clears persistent normal profiles; current mode is $SACCADE_EFFECTIVE_PROFILE_MODE" >&2
  exit 2
fi

python3 - "$SACCADE_EFFECTIVE_PROFILE_DIR" "$SACCADE_PROFILE_ROOT" "$SACCADE_PROFILE_NAME" "$YES" "$DRY_RUN" "$FORCE_CUSTOM" <<'PY'
import json
import os
import shutil
import sys
from pathlib import Path

profile_dir = Path(sys.argv[1]).expanduser().resolve()
profile_root = Path(sys.argv[2]).expanduser().resolve()
profile_name = sys.argv[3]
yes = sys.argv[4] == "1"
dry_run = sys.argv[5] == "1"
force_custom = sys.argv[6] == "1"

home = Path.home().resolve()
cwd = Path.cwd().resolve()
default_child = profile_dir.parent == profile_root and profile_dir.name == profile_name

refuse_reasons = []
if profile_dir in (Path("/").resolve(), home, cwd):
    refuse_reasons.append("target is too broad")
if not default_child and not force_custom:
    refuse_reasons.append("target is not the named profile under SACCADE_PROFILE_ROOT")
if not profile_dir.exists():
    payload = {
        "ok": True,
        "action": "noop_missing_profile",
        "profile_dir": str(profile_dir),
        "profile_name": profile_name,
        "persistent": True,
    }
    print(json.dumps(payload, indent=2, sort_keys=True))
    raise SystemExit(0)
if refuse_reasons:
    payload = {
        "ok": False,
        "action": "refused",
        "profile_dir": str(profile_dir),
        "profile_name": profile_name,
        "reasons": refuse_reasons,
        "hint": "Pass --force-custom only if this is intentionally a Saccade test profile.",
    }
    print(json.dumps(payload, indent=2, sort_keys=True))
    raise SystemExit(2)

children = [child for child in profile_dir.iterdir()]
file_count = 0
size_bytes = 0
for path in profile_dir.rglob("*"):
    try:
        if path.is_file():
            file_count += 1
            size_bytes += path.stat().st_size
    except OSError:
        pass

summary = {
    "ok": True,
    "action": "dry_run" if dry_run else "clear_profile",
    "profile_dir": str(profile_dir),
    "profile_name": profile_name,
    "persistent": True,
    "would_remove_entries": len(children),
    "would_remove_files": file_count,
    "would_remove_bytes": size_bytes,
    "raw_cookies_printed": False,
    "raw_storage_printed": False,
}

if dry_run:
    print(json.dumps(summary, indent=2, sort_keys=True))
    raise SystemExit(0)

if not yes:
    print(json.dumps({**summary, "ok": False, "action": "confirmation_required"}, indent=2, sort_keys=True))
    expected = f"CLEAR {profile_name}"
    typed = input(f"Type {expected!r} to clear this Saccade profile: ")
    if typed != expected:
        print(json.dumps({"ok": False, "action": "cancelled", "profile_dir": str(profile_dir)}, indent=2, sort_keys=True))
        raise SystemExit(3)

for child in children:
    if child.is_dir() and not child.is_symlink():
        shutil.rmtree(child)
    else:
        child.unlink(missing_ok=True)
profile_dir.mkdir(parents=True, exist_ok=True)
(profile_dir / ".saccade-profile.json").write_text(
    json.dumps(
        {
            "kind": "saccade_browser_profile",
            "mode": "normal",
            "persistent": True,
            "name": profile_name,
            "cleared": True,
        },
        indent=2,
        sort_keys=True,
    )
    + "\n",
    encoding="utf-8",
)
print(json.dumps(summary, indent=2, sort_keys=True))
PY
SH

if [[ "$INCLUDE_LEGACY_SHELL" == "1" ]]; then
  cat > "$OUT/open-legacy-saccade" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
source "$DIR/lib/profile.sh"
URL="${1:-https://example.com}"
cd "$SACCADE_ROOT"
saccade_resolve_profile
saccade_run_with_profile_cleanup "$DIR/bin/saccade-shell" browse \
  --url "$URL" \
  --width "${SACCADE_WIDTH:-1440}" \
  --height "${SACCADE_HEIGHT:-1000}" \
  --profile-dir "$SACCADE_EFFECTIVE_PROFILE_DIR"
SH
  chmod +x "$OUT/open-legacy-saccade"
fi

chmod +x "$OUT/open-saccade" "$OUT/servoshell-bridge" "$OUT/check-saccade" "$OUT/read-article" "$OUT/run-formmax" "$OUT/run-local-game-reflex" "$OUT/run-ai020-live-draft" "$OUT/profile-status" "$OUT/clear-profile"
chmod +x "$OUT/lib/read_article_fallback.py" "$OUT/lib/run_ai020_live_draft.py"

cp "$ROOT/docs/CURRENT_ACTION_ITEMS.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/CURRENT_PLAN.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/ai017_real_dogfood_flow_matrix.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/ai018_dogfood_launch_visibility.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/ai019_public_evidence_pack.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/ai020_human_in_loop_site_matrix.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/ai021_profile_productization_report.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/browser_compat_ledger.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/dogfood_browser_quickstart.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/dogfood_release_plan.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/github_dropdown_compat_shim_probe.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/SACCADE_DOGFOOD_HANDOFF.md" "$OUT/docs/"
cp "$ROOT/docs/profile_persistence_report.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/site_policy_matrix.md" "$OUT/docs/"
cp "$ROOT/docs/servo_0_2_retirement_plan.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT"/scripts/userscripts/*.js "$OUT/userscripts/" 2>/dev/null || true

cat > "$OUT/DOGFOOD_STATUS.md" <<STATUS
# Saccade Dogfood Status

Built: $BUILD_TIME_UTC
Saccade commit: $RELEASE_COMMIT
Branch: $RELEASE_BRANCH
ServoShell binary: $SERVOSHELL_BIN

## Use This First

\`\`\`bash
$OUT/check-saccade
$OUT/profile-status
$OUT/open-saccade https://example.com
\`\`\`

## Current Claims

- Default runtime is the ServoShell 0.3 bridge, not the legacy embedded Servo
  0.2 shell.
- The bridge uses clean Servo shutdown so local profile/cookie flush works for
  measured local profile flows.
- Wrappers default to the stable Saccade profile at \`$DEFAULT_PROFILE_DIR\`,
  not a per-build kit profile. Override with \`SACCADE_PROFILE_DIR=/path/to/profile\`.
- Named local profiles are available with \`SACCADE_PROFILE_NAME=work\`,
  resolving under \`$DEFAULT_PROFILE_ROOT/<name>\` unless \`SACCADE_PROFILE_DIR\`
  is explicitly set.
- \`profile-status\` prints a JSON profile/grant summary without cookie or
  storage values. \`clear-profile\` clears the current normal profile only after
  explicit confirmation or \`--yes\`; custom profile paths require
  \`--force-custom\`.
- Incognito/ephemeral dogfood is available with \`SACCADE_INCOGNITO=1\` or
  \`SACCADE_PROFILE_MODE=incognito\`; wrappers create a temporary marked profile
  under \`$OUT/runs/incognito\` and delete it when the command exits.
- Visible \`open-saccade\` launches show a local launch page first, print
  immediate terminal status, and then navigate that same bridge session to the
  target URL.
- Same-tab agent help is limited by the site policy docs copied into
  \`docs/\`.
- Public article/tutorial extraction is available through \`read-article\`.
  If the Saccade browser article path hangs or exits nonzero,
  \`read-article\` returns a bounded public HTTP fallback packet instead of
  hanging silently. Disable fallback with \`SACCADE_READ_ARTICLE_FALLBACK=off\`.
- Local long-form/table fill dogfood is available through \`run-formmax\`.
- Local game reflex dogfood is available through \`run-local-game-reflex\`.
- Real-site human-in-loop draft measurements are available through
  \`run-ai020-live-draft\`. It launches the visible ServoShell bridge, waits
  for the human when requested, calls \`inspect_editors\` and
  \`draft_editor_fill\`, writes a redacted AI-020 report, and verifies draft
  values do not leak into the report/replay artifacts.
- Optional ServoShell userscripts can be enabled by setting
  \`SACCADE_SERVOSHELL_USERSCRIPTS_DIR=$OUT/userscripts\` before running
  \`open-saccade\`, \`servoshell-bridge\`, \`check-saccade\`, or
  \`read-article\`. This is currently an experimental browser-compat layer,
  not a default claim.
- Public/demo claims, rerun commands, non-claims, and video/article shot list
  are frozen in \`docs/ai019_public_evidence_pack.md\`.
- The next real-site human-in-loop draft matrix is in
  \`docs/ai020_human_in_loop_site_matrix.md\`.

## Known Limits

- This is a local unsigned dogfood kit, not a notarized macOS app.
- GitHub account/logout menu visual parity is not claimed. Real GitHub/Primer
  menus currently hit Servo API gaps around \`IntersectionObserver\` and
  adopted stylesheets.
- Login, password, OTP, CAPTCHA, payment, release, signing, account recovery,
  and destructive actions remain human-only.
- Provider sessions may still require same-process login or fresh 2FA even
  though local profile flush is fixed.
STATUS

cat > "$OUT/README.md" <<README
# Saccade Dogfood Release

Kind: local dogfood kit, not public/notarized app distribution.

Built: $BUILD_TIME_UTC
Saccade commit: $RELEASE_COMMIT
Branch: $RELEASE_BRANCH

Check the kit first:

\`\`\`bash
$OUT/check-saccade
\`\`\`

See the active browser profile and current agent grant file:

\`\`\`bash
$OUT/profile-status
\`\`\`

Open a page:

\`\`\`bash
$OUT/open-saccade https://example.com
\`\`\`

The visible browser opens a local Saccade launch page first, then the bridge
navigates that same session to the requested URL. The wrapper prints launch
status to stderr immediately, so a slow provider page should not look like "no
window opened."

This uses the ServoShell 0.3 bridge by default and writes a current-tab grant:

\`\`\`text
$OUT/current_tab_grant.json
\`\`\`

Login/session persistence uses one stable local Saccade profile by default:

\`\`\`text
$DEFAULT_PROFILE_DIR
\`\`\`

That directory is under \`runs/\`, which is gitignored. It stores browser cookies
and local storage locally, similar to a Chrome profile, but Saccade control
artifacts still redact sensitive field values and do not print cookies. To use a
named profile under the Saccade profile root:

\`\`\`bash
SACCADE_PROFILE_NAME=work $OUT/open-saccade https://example.com
\`\`\`

To use a fully custom profile path:

\`\`\`bash
SACCADE_PROFILE_DIR=/path/to/another/profile $OUT/open-saccade https://example.com
\`\`\`

To clear the current normal Saccade profile and sign sites out:

\`\`\`bash
$OUT/clear-profile --dry-run
$OUT/clear-profile --yes
\`\`\`

\`clear-profile\` refuses custom profile paths unless \`--force-custom\` is
passed. It prints counts/bytes only; it never prints cookie or storage values.

For incognito/ephemeral browsing:

\`\`\`bash
SACCADE_INCOGNITO=1 $OUT/open-saccade https://example.com
SACCADE_PROFILE_MODE=incognito $OUT/check-saccade
\`\`\`

Incognito mode uses a temporary marked profile under:

\`\`\`text
$OUT/runs/incognito
\`\`\`

The wrapper removes that temporary profile after the command exits. Agent grants
inside the incognito session still use redacted truth/actions and do not expose
raw cookies, storage dumps, password data, or sensitive field values.

Run a bridge smoke manually:

\`\`\`bash
$OUT/servoshell-bridge --smoke
\`\`\`

Read a public article/tutorial page and exit with JSON:

\`\`\`bash
$OUT/read-article https://example.com/tutorial
\`\`\`

\`read-article\` first tries the live Saccade/ServoShell article path. If that
path exceeds \`SACCADE_READ_ARTICLE_HARD_TIMEOUT_SEC\` or exits nonzero, it kills
the browser process group and emits a public HTTP fallback packet with
\`route=http_article_fallback\`. The fallback sends no browser cookies or profile
data, and is only for public reference pages.

Run the local game reflex gate when the game server is up:

\`\`\`bash
$OUT/run-local-game-reflex http://127.0.0.1:4173/
\`\`\`

Run the local FORMMAX long-form gate:

\`\`\`bash
$OUT/run-formmax
\`\`\`

Run a real-site human-in-loop draft measurement:

\`\`\`bash
printf 'Saccade AI-020 draft rehearsal. Human will review and decide whether to submit.\\n' > /tmp/saccade-draft.txt
$OUT/run-ai020-live-draft \\
  --site hn_comment \\
  --url https://news.ycombinator.com/item?id=48706714 \\
  --body-file /tmp/saccade-draft.txt \\
  --manual-gate
\`\`\`

Login/password/OTP/CAPTCHA stay human-only. The agent writes only a draft and
does not click submit/publish.

Legacy embedded Servo 0.2 shell is not built by default. To include it for old
regression checks only:

\`\`\`bash
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
\`\`\`

Icon policy: use a distinct Saccade icon for Saccade builds. Do not reuse the
official Servo app icon unless the Servo project explicitly grants that use.

Safety: human-only for login, password, OTP, CAPTCHA, payment, release,
signing, account recovery, and destructive actions. For high-risk pages, use
redacted notes instead of live agent access.

Known browser compatibility limit: GitHub account/logout dropdown parity is
routed to Servo Web API compatibility. Source-release and official ServoShell
currently miss APIs GitHub/Primer uses, including IntersectionObserver and
Document/ShadowRoot adopted stylesheets.

Optional userscript compatibility layer:

\`\`\`bash
SACCADE_SERVOSHELL_USERSCRIPTS_DIR=$OUT/userscripts \\
  $OUT/open-saccade https://gist.github.com/starred
\`\`\`

This is experimental. It is useful for measuring whether ServoShell userscripts
can provide missing non-sensitive browser APIs on a target page. It is not a
claim that GitHub account-menu geometry is fixed.
README

CURRENT_LINK="$ROOT/dist/saccade-dogfood-current"
case "$OUT" in
  "$ROOT"/dist/*)
    ln -sfn "$(basename "$OUT")" "$CURRENT_LINK"
    ;;
esac

echo "SACCADE_DOGFOOD_RELEASE=$OUT"
if [[ -L "${CURRENT_LINK:-}" ]]; then
  echo "SACCADE_DOGFOOD_CURRENT=$CURRENT_LINK"
fi
