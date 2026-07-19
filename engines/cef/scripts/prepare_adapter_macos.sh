#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
CEF_ROOT=${CEF_ROOT:-$($SCRIPT_DIR/fetch_macos.sh)}
SIMPLE_ROOT="$CEF_ROOT/tests/cefsimple"
cp "$REPO_ROOT/engines/cef/host/saccade_adapter.h" "$SIMPLE_ROOT/saccade_adapter.h"
cp "$REPO_ROOT/engines/cef/host/saccade_adapter.cc" "$SIMPLE_ROOT/saccade_adapter.cc"
cp "$REPO_ROOT/engines/cef/host/saccade_renderer.h" "$SIMPLE_ROOT/saccade_renderer.h"
cp "$REPO_ROOT/engines/cef/host/saccade_renderer.cc" "$SIMPLE_ROOT/saccade_renderer.cc"
cp "$REPO_ROOT/engines/cef/host/saccade_brand_resources.h" \
  "$SIMPLE_ROOT/saccade_brand_resources.h"
cp "$REPO_ROOT/engines/cef/host/saccade_brand_resources.cc" \
  "$SIMPLE_ROOT/saccade_brand_resources.cc"
cp "$REPO_ROOT/engines/cef/host/saccade_form_script.h" "$SIMPLE_ROOT/saccade_form_script.h"
cp "$REPO_ROOT/engines/cef/host/saccade_direct_session_mac.h" \
  "$SIMPLE_ROOT/saccade_direct_session_mac.h"
cp "$REPO_ROOT/engines/cef/host/saccade_agent_switch_mac.h" \
  "$SIMPLE_ROOT/saccade_agent_switch_mac.h"
cp "$REPO_ROOT/engines/cef/host/saccade_agent_switch_mac.mm" \
  "$SIMPLE_ROOT/saccade_agent_switch_mac.mm"

if ! grep -q 'saccade_adapter.cc' "$SIMPLE_ROOT/CMakeLists.txt"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0001-engine-adapter-cmake.patch"
fi
if ! grep -q 'OnAddressChange' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0002-engine-adapter-header.patch"
fi
if ! grep -q 'saccade_adapter.h' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0003-engine-adapter-handler.patch"
fi
if ! grep -q 'saccade_renderer.cc' "$SIMPLE_ROOT/CMakeLists.txt"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0004-renderer-helper-cmake.patch"
fi
if ! grep -q 'SaccadeRendererApp' "$SIMPLE_ROOT/process_helper_mac.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0005-renderer-helper-entry.patch"
fi
if ! grep -q 'OnProcessMessageReceived' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0006-renderer-message-header.patch"
fi
if ! grep -q 'OnRendererMessage' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0007-renderer-message-handler.patch"
fi
if ! grep -q 'GetSwitchValue("window-size")' "$SIMPLE_ROOT/simple_app.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0008-window-size-views.patch"
fi
if ! grep -q 'respondsToSelector:@selector(tryToTerminateApplication:)' "$SIMPLE_ROOT/cefsimple_mac.mm"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0009-macos-quit-delegate-fallback.patch"
fi
if ! grep -q 'OnLoadingStateChange' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0010-load-complete-adapter.patch"
fi
if ! grep -q 'OnGotFocus' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0011-visible-tab-focus.patch"
fi
if ! grep -q 'Keep BrowserView as the direct window child' "$SIMPLE_ROOT/simple_app.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0012-browser-view-direct-child.patch"
fi
if ! grep -q 'settings.root_cache_path' "$SIMPLE_ROOT/cefsimple_mac.mm"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0013-profile-root-cache.patch"
fi
if ! grep -q 'Saccade is a human-facing browser' "$SIMPLE_ROOT/simple_app.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0014-native-chrome-ui-default.patch"
fi
if ! grep -q 'SaccadeDirectSession direct_session' "$SIMPLE_ROOT/cefsimple_mac.mm"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0015-direct-saccade-session.patch"
fi
if grep -q 'title="cefsimple"' "$SIMPLE_ROOT/mac/English.lproj/MainMenu.xib"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0016-saccade-menu-branding.patch"
fi
if ! grep -q 'saccade_agent_switch_mac.mm' "$SIMPLE_ROOT/CMakeLists.txt"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0017-agent-switch-macos.patch"
fi
if ! grep -q 'Based on Chromium' \
  "$SIMPLE_ROOT/mac/English.lproj/InfoPlist.strings"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0018-saccade-about-copyright.patch"
fi
if grep -q 'title="TestShell"' "$SIMPLE_ROOT/mac/English.lproj/MainMenu.xib"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0019-saccade-menu-title.patch"
fi
if grep -q 'title="Preferences…"' "$SIMPLE_ROOT/mac/English.lproj/MainMenu.xib"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0020-saccade-settings-label.patch"
fi
# The follow-up routed-tab patch replaces OpenUserTabOnUi, so detect the
# structural helper introduced by 0021 instead of the call that 0025 changes.
if ! grep -q 'IsSaccadeTabDisposition' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0021-normal-links-open-tabs.patch"
fi
if ! grep -q 'GetDownloadHandler' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0022-chrome-downloads.patch"
fi
if ! grep -q 'showSaccadeHelp' "$SIMPLE_ROOT/cefsimple_mac.mm"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0023-nan-logic-company-help.patch"
fi
if ! grep -q 'saccade_brand_resources.cc' "$SIMPLE_ROOT/CMakeLists.txt"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0024-saccade-brand-resources.patch"
fi
if ! grep -q 'OpenRoutedTabOnUi' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0025-agent-child-tab-routing.patch"
fi
if ! grep -q 'connectSaccadeToCodex' "$SIMPLE_ROOT/cefsimple_mac.mm"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0026-codex-mcp-registration.patch"
fi
if ! grep -q 'HumanVerificationProviderForRequest' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0027-human-verification-failure.patch"
fi

# Fail before compilation if an old mutable CEF cache contains a patch twice.
# A clean pinned archive plus one ordered patch pass must yield one definition.
for SYMBOL in IsSaccadeTabDisposition 'SimpleHandler::OnBeforePopup' \
  'SimpleHandler::OnOpenURLFromTab'; do
  COUNT=$(grep -c "^bool $SYMBOL" "$SIMPLE_ROOT/simple_handler.cc" || true)
  [ "$COUNT" -eq 1 ] || {
    echo "CEF cache patch multiplicity error: $SYMBOL count=$COUNT" >&2
    exit 1
  }
done
