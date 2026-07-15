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
