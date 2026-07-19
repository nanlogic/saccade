#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
STAMP=${SACCADE_RELEASE_STAMP:-$(date +%Y%m%d-%H%M%S)}
OUT=${SACCADE_RELEASE_OUT:-$REPO_ROOT/dist/saccade-cef-dogfood-$STAMP}
BUILD_DIR=${SACCADE_CEF_BUILD_DIR:-$REPO_ROOT/target/cef-release}
APP="$BUILD_DIR/Saccade.app"
MCP_BIN="$REPO_ROOT/target/release/saccade-mcp"

[ ! -e "$OUT" ] || { echo "Release path already exists: $OUT" >&2; exit 1; }
[ -f "$REPO_ROOT/LICENSE" ] || { echo "Missing Saccade LICENSE" >&2; exit 1; }
[ -f "$REPO_ROOT/NOTICE" ] || { echo "Missing Saccade NOTICE" >&2; exit 1; }
[ -f "$REPO_ROOT/TRADEMARKS.md" ] || { echo "Missing trademark policy" >&2; exit 1; }
cargo build --release -p saccade-mcp --manifest-path "$REPO_ROOT/Cargo.toml"
SACCADE_CODESIGN_IDENTITY=${SACCADE_CODESIGN_IDENTITY:-auto} \
SACCADE_CODESIGN_TIMESTAMP=${SACCADE_CODESIGN_TIMESTAMP:-none} \
  "$SCRIPT_DIR/build_macos.sh"
cp "$MCP_BIN" "$APP/Contents/MacOS/saccade-mcp"
cp "$REPO_ROOT/engines/cef/release/saccade-current-tab-mcp" \
  "$APP/Contents/MacOS/saccade-current-tab-mcp"
cp "$REPO_ROOT/engines/cef/release/saccade-connect-codex" \
  "$APP/Contents/MacOS/saccade-connect-codex"
cp "$REPO_ROOT/engines/cef/release/profile-status" \
  "$APP/Contents/MacOS/saccade-profile-status"
cp "$REPO_ROOT/engines/cef/release/clear-profile" \
  "$APP/Contents/MacOS/saccade-clear-profile"
chmod 755 "$APP/Contents/MacOS/saccade-mcp" \
  "$APP/Contents/MacOS/saccade-current-tab-mcp" \
  "$APP/Contents/MacOS/saccade-connect-codex" \
  "$APP/Contents/MacOS/saccade-profile-status" \
  "$APP/Contents/MacOS/saccade-clear-profile"
mkdir -p "$APP/Contents/Resources/Saccade/fixtures/mcp_installation"
ditto "$REPO_ROOT/test_pages/mcp_installation" \
  "$APP/Contents/Resources/Saccade/fixtures/mcp_installation"
mkdir -p "$APP/Contents/Resources/Saccade/fixtures/downloads"
ditto "$REPO_ROOT/test_pages/downloads" \
  "$APP/Contents/Resources/Saccade/fixtures/downloads"
cat > "$APP/Contents/Resources/Saccade/INSTALLATION.json" <<EOF
{
  "schema": "saccade-self-contained-installation-v1",
  "release_stamp": "$STAMP",
  "mcp_command": "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp",
  "publisher_name": "NaN Logic LLC",
  "publisher_url": "https://nanlogic.com/",
  "help_url": "https://nanlogic.com/",
  "repo_required": false
}
EOF
cp "$REPO_ROOT/docs/privacy_and_cookie_model.md" \
  "$APP/Contents/Resources/Saccade/PRIVACY_AND_COOKIE_MODEL.md"
cp "$REPO_ROOT/LICENSE" \
  "$APP/Contents/Resources/Saccade/SACCADE_LICENSE.txt"
cp "$REPO_ROOT/NOTICE" \
  "$APP/Contents/Resources/Saccade/SACCADE_NOTICE.txt"
cp "$REPO_ROOT/TRADEMARKS.md" \
  "$APP/Contents/Resources/Saccade/SACCADE_TRADEMARKS.md"
SACCADE_CODESIGN_IDENTITY=${SACCADE_CODESIGN_IDENTITY:-auto} \
SACCADE_CODESIGN_TIMESTAMP=${SACCADE_CODESIGN_TIMESTAMP:-none} \
  "$SCRIPT_DIR/sign_macos.sh" "$APP"
codesign --verify --strict --verbose=2 "$APP"

mkdir -p "$OUT/bin" "$OUT/docs" "$OUT/licenses" "$OUT/tools" "$OUT/fixtures"
ditto "$APP" "$OUT/Saccade.app"
cp "$MCP_BIN" "$OUT/bin/saccade-mcp"
cp "$REPO_ROOT/engines/cef/release/open-saccade" "$OUT/bin/open-saccade"
cp "$REPO_ROOT/engines/cef/release/current-agent-grant" \
  "$OUT/bin/current-agent-grant"
cp "$REPO_ROOT/engines/cef/release/saccade-current-tab-mcp" \
  "$OUT/bin/saccade-current-tab-mcp"
cp "$REPO_ROOT/engines/cef/release/saccade-connect-codex" \
  "$OUT/bin/saccade-connect-codex"
cp "$REPO_ROOT/engines/cef/release/profile-status" "$OUT/bin/profile-status"
cp "$REPO_ROOT/engines/cef/release/clear-profile" "$OUT/bin/clear-profile"
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
cp "$REPO_ROOT/scripts/probe_ai038_conversational_dogfood.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_downloads.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_profile_controls.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_layout_epoch.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_release_license.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_company_help.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/probe_cef_media_capabilities.py" "$OUT/tools/"
cp "$REPO_ROOT/scripts/generate_release_sbom.py" "$OUT/tools/"
ditto "$REPO_ROOT/test_pages/form_plan" "$OUT/fixtures/form_plan"
ditto "$REPO_ROOT/test_pages/ai038_conversational_dogfood" \
  "$OUT/fixtures/ai038_conversational_dogfood"
ditto "$REPO_ROOT/test_pages/downloads" "$OUT/fixtures/downloads"
ditto "$REPO_ROOT/test_pages/layout_epoch" "$OUT/fixtures/layout_epoch"
cp "$REPO_ROOT/engines/cef/cef.lock.json" "$OUT/docs/cef.lock.json"
cp "$REPO_ROOT/docs/integration_contract_v1.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/cef_day5_dogfood_release_report.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/ai038_conversational_dogfood.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/ai039_native_browser_chrome.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/ai040_per_tab_agent_consent.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/privacy_and_cookie_model.md" "$OUT/docs/"
cp "$REPO_ROOT/docs/public_release_licensing.md" "$OUT/docs/"
cp "$OUT/Saccade.app/Contents/Resources/CEF_LICENSE.txt" "$OUT/licenses/"
cp "$OUT/Saccade.app/Contents/Resources/CHROMIUM_CREDITS.html" "$OUT/licenses/"
cp "$REPO_ROOT/LICENSE" "$OUT/licenses/SACCADE_LICENSE.txt"
cp "$REPO_ROOT/NOTICE" "$OUT/licenses/SACCADE_NOTICE.txt"
cp "$REPO_ROOT/TRADEMARKS.md" "$OUT/licenses/SACCADE_TRADEMARKS.md"

COMMIT=$(git -C "$REPO_ROOT" rev-parse HEAD)
if [ -z "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
  SOURCE_DIRTY=false
  SOURCE_DESCRIPTION=$COMMIT
else
  SOURCE_DIRTY=true
  SOURCE_DESCRIPTION="$COMMIT-dirty"
fi
CEF_VERSION=$(jq -r .cef_version "$REPO_ROOT/engines/cef/cef.lock.json")
CHROMIUM_VERSION=$(jq -r .chromium_version "$REPO_ROOT/engines/cef/cef.lock.json")
APP_VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
  "$APP/Contents/Info.plist")
APP_BUILD=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
  "$APP/Contents/Info.plist")
TEAM=$(codesign -dvv "$APP" 2>&1 | sed -n 's/^TeamIdentifier=//p')
APP_SIGNATURE=$(codesign -dvvv "$APP" 2>&1)
if echo "$APP_SIGNATURE" | grep -q 'flags=.*runtime'; then
  HARDENED_RUNTIME=true
else
  HARDENED_RUNTIME=false
fi
if echo "$APP_SIGNATURE" | grep -q '^Timestamp='; then
  SECURE_TIMESTAMP=true
else
  SECURE_TIMESTAMP=false
fi
cat > "$OUT/VERSION.json" <<EOF
{
  "schema": "saccade-cef-dogfood-release-v1",
  "channel": "local-macos-dogfood",
  "source_commit": "$COMMIT",
  "source_description": "$SOURCE_DESCRIPTION",
  "source_dirty": $SOURCE_DIRTY,
  "app_version": "$APP_VERSION",
  "app_build": "$APP_BUILD",
  "cef_version": "$CEF_VERSION",
  "chromium_version": "$CHROMIUM_VERSION",
  "bundle_identifier": "ai.saccade.browser",
  "codesign_team": "$TEAM",
  "hardened_runtime": $HARDENED_RUNTIME,
  "secure_timestamp": $SECURE_TIMESTAMP,
  "browser_chrome": "cef_native_chrome_ui",
  "browser_fallback_favicon": "Saccade-tab.png",
  "agent_consent": "per_tab_browser_owned_switch_v1",
  "llm_tab_launch": "open_or_reuse_agent_tab_v1",
  "cookie_model": "chromium_profile_agent_value_isolation_v1",
  "privacy_document": "docs/privacy_and_cookie_model.md",
  "layout_safety": "live_layout_epoch_semantic_rebase_receipt_v1",
  "source_license": "Apache-2.0",
  "source_license_file": "licenses/SACCADE_LICENSE.txt",
  "trademark_policy_file": "licenses/SACCADE_TRADEMARKS.md",
  "publisher_name": "NaN Logic LLC",
  "publisher_url": "https://nanlogic.com/",
  "help_url": "https://nanlogic.com/",
  "official_binary_identity": {
    "bundle_identifier": "ai.saccade.browser",
    "codesign_team": "$TEAM"
  },
  "notarized": false,
  "public_distribution_ready": false
}
EOF
cat > "$OUT/licenses/INVENTORY.json" <<EOF
{
  "schema": "saccade-license-inventory-v1",
  "cef": {"license": "BSD-3-Clause", "file": "CEF_LICENSE.txt"},
  "chromium": {"credits": "CHROMIUM_CREDITS.html"},
  "saccade": {
    "license": "Apache-2.0",
    "license_file": "SACCADE_LICENSE.txt",
    "notice_file": "SACCADE_NOTICE.txt",
    "trademark_policy_file": "SACCADE_TRADEMARKS.md",
    "copyright_owner": "NaN Logic LLC",
    "publisher_url": "https://nanlogic.com/",
    "official_bundle_identifier": "ai.saccade.browser",
    "official_codesign_team": "$TEAM"
  }
}
EOF
python3 "$REPO_ROOT/scripts/generate_release_sbom.py" \
  --package "$OUT" --output "$OUT/licenses/SBOM.cdx.json"
cat > "$OUT/MCP_CONFIG.toml" <<EOF
[mcp_servers.saccade]
command = "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp"
EOF
cat > "$OUT/README.txt" <<'EOF'
Saccade CEF macOS dogfood release

Licensing and official identity

Saccade source code and the core browser/Agent runtime are licensed under
Apache License 2.0. The Saccade name, logo and designation "official Saccade"
are not granted for modified distributions. The complete Saccade license,
notice, trademark policy, CEF license and Chromium credits are in licenses/;
the same Saccade license and identity documents are embedded inside the app.
See docs/public_release_licensing.md.

Saccade is published by NaN Logic LLC. Choose Help > Saccade Help —
nanlogic.com to open https://nanlogic.com/ in a new Human-controlled Agent Off
tab inside Saccade.

Saccade opens with CEF's standard Chrome-style browser UI: tabs, new tab,
address bar, Back, Forward, and Reload/Stop are visible. Click the address bar
or press Command-L, type a URL, and press Return.

Drag Saccade.app into /Applications and open it once. For each macOS user with
Codex installed, Saccade automatically registers the stable signed MCP command;
open a new Codex task once. Help > Connect Saccade to Codex repairs a missing
or stale entry. Other MCP-capable LLMs can use MCP_CONFIG.toml manually. The
command keeps working when Saccade.app is replaced.

Human-created tabs start with Agent Off. Use the browser-owned Agent Off/On
button for the visible tab when you want the LLM to read or control it.

When the LLM starts browsing, it calls saccade.tabs.open_agent. Saccade opens
a dedicated Agent On tab in the existing process, or starts the signed app if
needed. It never silently enables a Human tab.

The compatibility launcher also starts one Agent On tab:

  bin/open-saccade https://example.com

You can now ask the LLM in normal language:

  Read the current Saccade article and tell me whether it is useful.
  Research this page and compare it with my current project.
  I filled the SSN. Fill the remaining ordinary fields, preserve my values,
  and do not submit.

The MCP server discovers the owner-only broker published by the signed app.
The broker is not itself a read grant, and its capability is never printed or
copied into chat.

Resize, scroll, zoom, and responsive-layout changes invalidate coordinate-
bearing actions inside the browser. Saccade refreshes the current geometry
without a screenshot, locally rebases only the same stable semantic action,
and reports success only after a native input receipt. A disappeared or
ambiguous target is rejected before input.

Downloads use CEF's Chrome-style download handling. An Agent may trigger a
verified page download while its tab is On, then query metadata-only receipts
through saccade.downloads.list. Receipts expose no full local path or file
contents, and Saccade never auto-executes a downloaded file.

Open a saved normal profile:
  bin/open-saccade https://example.com

Open a disposable private profile:
  SACCADE_PROFILE_MODE=incognito bin/open-saccade https://example.com

Inspect or clear a persistent profile without printing Cookie/storage values:
  bin/profile-status
  bin/clear-profile --dry-run
  bin/clear-profile --yes

After installing only Saccade.app, the same controls remain available at:
  /Applications/Saccade.app/Contents/MacOS/saccade-profile-status
  /Applications/Saccade.app/Contents/MacOS/saccade-clear-profile --dry-run
  /Applications/Saccade.app/Contents/MacOS/saccade-clear-profile --yes

Read docs/privacy_and_cookie_model.md for the complete Cookie boundary. The
browser may use a site's Cookie to preserve an authenticated session, but the
Agent bridge never receives raw Cookies, browser storage, or Keychain secrets.

Opening with bin/open-saccade creates an Agent-owned On tab. Opening
Saccade.app directly creates a Human Off tab while keeping the broker ready.
For diagnostics
only, locate the grant path with:
  bin/current-agent-grant

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

This is a signed Hardened Runtime local dogfood build. It is not notarized and
is not a public general-browser release. Provider anti-bot, DRM, proprietary
codecs, and every third-party custom editor are not claimed.

On the first saved-profile launch, macOS may ask Saccade to access "Chromium
Safe Storage". CEF uses that Keychain item to encrypt persistent cookies. Check
that the requesting app is Saccade, then choose Always Allow once. Saccade's
agent bridge never receives the Keychain secret or raw cookies. A later prompt
can also mean the login Keychain was explicitly locked; unlock it in Keychain
Access before treating the event as a signing-identity regression.
EOF

(cd "$OUT" && find . -type f ! -name SHA256SUMS -print0 | sort -z | \
  xargs -0 shasum -a 256 > SHA256SUMS)
ln -sfn "$(basename "$OUT")" "$REPO_ROOT/dist/saccade-cef-dogfood-current"
printf '%s\n' "$OUT"
