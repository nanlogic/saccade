#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${1:-$ROOT/dist/saccade-dogfood-$STAMP}"
SERVOSHELL_BIN="${SACCADE_SERVOSHELL_BIN:-/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell}"
OWNED_DOMAINS="${SACCADE_OWNED_DOMAINS:-nanmesh.ai,mythcastera.com,mysterypartynow.com}"
INCLUDE_LEGACY_SHELL="${SACCADE_INCLUDE_LEGACY_SHELL:-0}"

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
exec "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
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
exec "$DIR/bin/saccade-servoshell" bridge --servoshell "$SACCADE_SERVOSHELL_BIN" "$@"
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
exec "$DIR/bin/saccade-servoshell" bridge \
  --servoshell "$SACCADE_SERVOSHELL_BIN" \
  --url "$URL" \
  --read-article \
  --article-max-chars "${SACCADE_ARTICLE_MAX_CHARS:-30000}" \
  --exit \
  --json \
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
exec "$DIR/bin/saccade-shell" browse \
  --url "$URL" \
  --width "${SACCADE_WIDTH:-1440}" \
  --height "${SACCADE_HEIGHT:-1000}" \
  --profile-dir "$DIR/profile/default"
SH
  chmod +x "$OUT/open-legacy-saccade"
fi

chmod +x "$OUT/open-saccade" "$OUT/servoshell-bridge" "$OUT/read-article" "$OUT/run-local-game-reflex"

cp "$ROOT/docs/dogfood_release_plan.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/SACCADE_DOGFOOD_HANDOFF.md" "$OUT/docs/"
cp "$ROOT/docs/site_policy_matrix.md" "$OUT/docs/"
cp "$ROOT/docs/servo_0_2_retirement_plan.md" "$OUT/docs/" 2>/dev/null || true

cat > "$OUT/README.md" <<README
# Saccade Dogfood Release

Kind: local dogfood kit, not public/notarized app distribution.

Open a page:

\`\`\`bash
$OUT/open-saccade https://example.com
\`\`\`

This uses the ServoShell 0.3 bridge by default and writes a current-tab grant:

\`\`\`text
$OUT/current_tab_grant.json
\`\`\`

Run a bridge smoke:

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
README

echo "SACCADE_DOGFOOD_RELEASE=$OUT"
