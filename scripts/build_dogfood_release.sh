#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${1:-$ROOT/dist/saccade-dogfood-$STAMP}"
SERVOSHELL_BIN="${SACCADE_SERVOSHELL_BIN:-/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell}"
OWNED_DOMAINS="${SACCADE_OWNED_DOMAINS:-nanmesh.ai,mythcastera.com,mysterypartynow.com}"

mkdir -p "$OUT/bin" "$OUT/docs" "$OUT/profile/default"

cargo build --release -p saccade-shell -p saccade-mcp -p saccade-servoshell

cp "$ROOT/target/release/saccade-shell" "$OUT/bin/"
cp "$ROOT/target/release/saccade-mcp" "$OUT/bin/"
cp "$ROOT/target/release/saccade-servoshell" "$OUT/bin/"

cat > "$OUT/saccade-dogfood.env" <<ENV
SACCADE_RELEASE_KIND=dogfood
SACCADE_ROOT=$ROOT
SACCADE_SERVOSHELL_BIN=$SERVOSHELL_BIN
SACCADE_OWNED_DOMAINS=$OWNED_DOMAINS
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
exec "$DIR/bin/saccade-shell" browse \
  --url "$URL" \
  --width "${SACCADE_WIDTH:-1440}" \
  --height "${SACCADE_HEIGHT:-1000}" \
  --profile-dir "$DIR/profile/default"
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

chmod +x "$OUT/open-saccade" "$OUT/servoshell-bridge"

cp "$ROOT/docs/dogfood_release_plan.md" "$OUT/docs/" 2>/dev/null || true
cp "$ROOT/docs/SACCADE_DOGFOOD_HANDOFF.md" "$OUT/docs/"
cp "$ROOT/docs/site_policy_matrix.md" "$OUT/docs/"

cat > "$OUT/README.md" <<README
# Saccade Dogfood Release

Kind: local dogfood kit, not public/notarized app distribution.

Open a page:

\`\`\`bash
$OUT/open-saccade https://example.com
\`\`\`

Run the official ServoShell bridge:

\`\`\`bash
$OUT/servoshell-bridge --smoke
\`\`\`

Icon policy: use a distinct Saccade icon for Saccade builds. Do not reuse the
official Servo app icon unless the Servo project explicitly grants that use.
README

echo "SACCADE_DOGFOOD_RELEASE=$OUT"
