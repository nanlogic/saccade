#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
STAMP=${SACCADE_RELEASE_STAMP:-$(date +%Y%m%d-%H%M%S)}
OUT=${SACCADE_RELEASE_OUT:-$REPO_ROOT/dist/saccade-cef-dogfood-$STAMP}
APP="$REPO_ROOT/target/cef-release/Saccade.app"
MCP_BIN="$REPO_ROOT/target/release/saccade-mcp"

[ ! -e "$OUT" ] || { echo "Release path already exists: $OUT" >&2; exit 1; }
SACCADE_CODESIGN_IDENTITY=${SACCADE_CODESIGN_IDENTITY:-auto} \
SACCADE_CODESIGN_TIMESTAMP=${SACCADE_CODESIGN_TIMESTAMP:-none} \
  "$SCRIPT_DIR/build_macos.sh"
cargo build --release -p saccade-mcp --manifest-path "$REPO_ROOT/Cargo.toml"
codesign --verify --strict --verbose=2 "$APP"

mkdir -p "$OUT/bin" "$OUT/docs" "$OUT/licenses" "$OUT/tools" "$OUT/fixtures"
ditto "$APP" "$OUT/Saccade.app"
cp "$MCP_BIN" "$OUT/bin/saccade-mcp"
cp "$REPO_ROOT/engines/cef/release/open-saccade" "$OUT/bin/open-saccade"
cp "$REPO_ROOT/engines/cef/release/current-agent-grant" \
  "$OUT/bin/current-agent-grant"
cp "$REPO_ROOT/engines/cef/release/profile-status" "$OUT/bin/profile-status"
cp "$REPO_ROOT/engines/cef/release/run-local-game-gate" \
  "$OUT/bin/run-local-game-gate"
cp "$REPO_ROOT/engines/cef/release/run-form-gate" "$OUT/bin/run-form-gate"
cp "$REPO_ROOT/engines/cef/release/docmax" "$OUT/bin/docmax"
cp "$REPO_ROOT/engines/cef/release/run-docmax-gate" "$OUT/bin/run-docmax-gate"
cp "$REPO_ROOT/engines/cef/release/run-github-canary" "$OUT/bin/run-github-canary"
chmod 755 "$OUT/bin/"*
cp "$REPO_ROOT/scripts/probe_cef_local_game.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_truth_reflex.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_mcp_form_plan.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/docmax_pdf.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_docmax.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/formmax_pdf_feasibility.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_github_canary.py" "$OUT/tools/"
ditto "$REPO_ROOT/test_pages/form_plan" "$OUT/fixtures/form_plan"
cp "$REPO_ROOT/engines/cef/cef.lock.json" "$OUT/docs/cef.lock.json"
cp "$REPO_ROOT/docs/integration_contract_v1.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/cef_day5_dogfood_release_report.md" "$OUT/docs/"
cp "$OUT/Saccade.app/Contents/Resources/CEF_LICENSE.txt" "$OUT/licenses/"
cp "$OUT/Saccade.app/Contents/Resources/CHROMIUM_CREDITS.html" "$OUT/licenses/"

COMMIT=$(git -C "$REPO_ROOT" rev-parse HEAD)
CEF_VERSION=$(jq -r .cef_version "$REPO_ROOT/engines/cef/cef.lock.json")
CHROMIUM_VERSION=$(jq -r .chromium_version "$REPO_ROOT/engines/cef/cef.lock.json")
TEAM=$(codesign -dvv "$APP" 2>&1 | sed -n 's/^TeamIdentifier=//p')
cat > "$OUT/VERSION.json" <<EOF
{
  "schema": "saccade-cef-dogfood-release-v1",
  "channel": "local-macos-dogfood",
  "source_commit": "$COMMIT",
  "cef_version": "$CEF_VERSION",
  "chromium_version": "$CHROMIUM_VERSION",
  "bundle_identifier": "ai.saccade.browser",
  "codesign_team": "$TEAM",
  "notarized": false,
  "public_distribution_ready": false
}
EOF
cat > "$OUT/licenses/INVENTORY.json" <<EOF
{
  "schema": "saccade-license-inventory-v1",
  "cef": {"license": "BSD-3-Clause", "file": "CEF_LICENSE.txt"},
  "chromium": {"credits": "CHROMIUM_CREDITS.html"},
  "saccade": {"status": "license-decision-required-before-public-distribution"}
}
EOF
cat > "$OUT/README.txt" <<'EOF'
Saccade CEF macOS dogfood release

Open a saved normal profile:
  bin/open-saccade https://example.com

Open a disposable private profile:
  SACCADE_PROFILE_MODE=incognito bin/open-saccade https://example.com

Opening with bin/open-saccade explicitly grants the visible tab to the
owner-only bridge. To locate that grant, run:
  bin/current-agent-grant

Opening Saccade.app directly does not start or grant an agent session.

With the local Blend or Die server running, rerun the fact-bound Canvas motor
gate with:
  bin/run-local-game-gate http://127.0.0.1:4173/

Rerun the public MCP ordinary-field form gate with:
  bin/run-form-gate

Inspect or safely fill an AcroForm PDF with value-free evidence:
  bin/docmax inspect --input blank.pdf --report inventory.json --replay replay.jsonl

Rerun the local DOCMAX gate with:
  bin/run-docmax-gate

Rerun the no-write GitHub New Issue and account-menu canary with:
  bin/run-github-canary

The returned owner-only grant contains the engine-neutral endpoint and
capability. Do not copy it into chat or logs. The agent never receives raw
cookies, raw browser storage, passwords, SSNs, or payment values.

This is a signed local dogfood build. It is not notarized and is not a public
general-browser release. Provider anti-bot, DRM, proprietary codecs, and every
third-party custom editor are not claimed.

On the first saved-profile launch, macOS may ask Saccade to access "Chromium
Safe Storage". CEF uses that Keychain item to encrypt persistent cookies. Check
that the requesting app is Saccade, then choose Always Allow once. Saccade's
agent bridge never receives the Keychain secret or raw cookies. Repeated prompts
mean an unsigned/development build was launched against the saved profile.
EOF

(cd "$OUT" && find . -type f ! -name SHA256SUMS -print0 | sort -z | \
  xargs -0 shasum -a 256 > SHA256SUMS)
ln -sfn "$(basename "$OUT")" "$REPO_ROOT/dist/saccade-cef-dogfood-current"
printf '%s\n' "$OUT"
