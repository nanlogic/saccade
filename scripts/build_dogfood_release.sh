#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${1:-$ROOT/dist/saccade-dogfood-$STAMP}"
SERVOSHELL_BIN="${SACCADE_SERVOSHELL_BIN:-/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell}"
OWNED_DOMAINS="${SACCADE_OWNED_DOMAINS:-nanmesh.ai,mythcastera.com,mysterypartynow.com}"
INCLUDE_LEGACY_SHELL="${SACCADE_INCLUDE_LEGACY_SHELL:-0}"
BUILD_TIME_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RELEASE_COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
RELEASE_BRANCH="$(git -C "$ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
mkdir -p "$OUT/bin" "$OUT/docs" "$OUT/profile/default"

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

cat > "$OUT/saccade-dogfood.env" <<ENV
SACCADE_RELEASE_KIND=dogfood
SACCADE_ROOT=$ROOT
SACCADE_DOGFOOD_DIR=$OUT
SACCADE_RELEASE_COMMIT=$RELEASE_COMMIT
SACCADE_RELEASE_BRANCH=$RELEASE_BRANCH
SACCADE_RELEASE_BUILD_TIME_UTC=$BUILD_TIME_UTC
SACCADE_SERVOSHELL_BIN=$SERVOSHELL_BIN
SACCADE_OWNED_DOMAINS=$OWNED_DOMAINS
SACCADE_INCLUDE_LEGACY_SHELL=$INCLUDE_LEGACY_SHELL
RUST_LOG=error
ENV

cat > "$OUT/open-saccade" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
URL="${1:-https://example.com}"
cd "$SACCADE_ROOT"
exec "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --profile-dir "$DIR/profile/default" \
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
has_arg --profile-dir "$@" || extra+=(--profile-dir "$DIR/profile/default")
has_arg --grant-path "$@" || extra+=(--grant-path "$DIR/current_tab_grant.json")
has_arg --output-dir "$@" || extra+=(--output-dir "$DIR/runs/servoshell_bridge")
exec "$DIR/bin/saccade-servoshell" bridge \
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
echo "Saccade dogfood kit: $DIR" >&2
echo "Saccade commit: ${SACCADE_RELEASE_COMMIT:-unknown}" >&2
echo "ServoShell: $SACCADE_SERVOSHELL_BIN" >&2
exec "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$SMOKE_URL" \
  --profile-dir "$DIR/profile/default" \
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
URL="${1:?usage: read-article <url> [output_name]}"
NAME="${2:-article_$(date +%Y%m%d-%H%M%S)}"
cd "$SACCADE_ROOT"
exec "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --read-article \
  --article-max-chars "${SACCADE_ARTICLE_MAX_CHARS:-30000}" \
  --exit \
  --json \
  --grant-path "$DIR/runs/article/$NAME/current_tab_grant.json" \
  --output-dir "$DIR/runs/article/$NAME"
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

if [[ "$INCLUDE_LEGACY_SHELL" == "1" ]]; then
  cat > "$OUT/open-legacy-saccade" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
set -a
source "$DIR/saccade-dogfood.env"
set +a
URL="${1:-https://example.com}"
cd "$SACCADE_ROOT"
exec "$DIR/bin/saccade-shell" browse \
  --url "$URL" \
  --width "${SACCADE_WIDTH:-1440}" \
  --height "${SACCADE_HEIGHT:-1000}" \
  --profile-dir "$DIR/profile/default"
SH
  chmod +x "$OUT/open-legacy-saccade"
fi

chmod +x "$OUT/open-saccade" "$OUT/servoshell-bridge" "$OUT/check-saccade" "$OUT/read-article" "$OUT/run-local-game-reflex"

cp "$ROOT/docs/CURRENT_ACTION_ITEMS.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/CURRENT_PLAN.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/browser_compat_ledger.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/dogfood_browser_quickstart.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/dogfood_release_plan.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/SACCADE_DOGFOOD_HANDOFF.md" "$OUT/docs/"
cp "$ROOT/docs/profile_persistence_report.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/site_policy_matrix.md" "$OUT/docs/"
cp "$ROOT/docs/servo_0_2_retirement_plan.md" "$OUT/docs/" 2>/dev/null || true

cat > "$OUT/DOGFOOD_STATUS.md" <<STATUS
# Saccade Dogfood Status

Built: $BUILD_TIME_UTC
Saccade commit: $RELEASE_COMMIT
Branch: $RELEASE_BRANCH
ServoShell binary: $SERVOSHELL_BIN

## Use This First

\`\`\`bash
$OUT/check-saccade
$OUT/open-saccade https://example.com
\`\`\`

## Current Claims

- Default runtime is the ServoShell 0.3 bridge, not the legacy embedded Servo
  0.2 shell.
- The bridge uses clean Servo shutdown so local profile/cookie flush works for
  measured local profile flows.
- Same-tab agent help is limited by the site policy docs copied into
  \`docs/\`.
- Public article/tutorial extraction is available through \`read-article\`.
- Local game reflex dogfood is available through \`run-local-game-reflex\`.

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

Open a page:

\`\`\`bash
$OUT/open-saccade https://example.com
\`\`\`

This uses the ServoShell 0.3 bridge by default and writes a current-tab grant:

\`\`\`text
$OUT/current_tab_grant.json
\`\`\`

Run a bridge smoke manually:

\`\`\`bash
$OUT/servoshell-bridge --smoke
\`\`\`

Read a public article/tutorial page and exit with JSON:

\`\`\`bash
$OUT/read-article https://example.com/tutorial
\`\`\`

Run the local game reflex gate when the game server is up:

\`\`\`bash
$OUT/run-local-game-reflex http://127.0.0.1:4173/
\`\`\`

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
