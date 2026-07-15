#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
CEF_ROOT=${CEF_ROOT:-$($SCRIPT_DIR/fetch_macos.sh)}
SIMPLE_ROOT="$CEF_ROOT/tests/cefsimple"
cp "$REPO_ROOT/engines/cef/host/saccade_adapter.h" "$SIMPLE_ROOT/saccade_adapter.h"
cp "$REPO_ROOT/engines/cef/host/saccade_adapter.cc" "$SIMPLE_ROOT/saccade_adapter.cc"

if ! grep -q 'saccade_adapter.cc' "$SIMPLE_ROOT/CMakeLists.txt"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0001-engine-adapter-cmake.patch"
fi
if ! grep -q 'OnAddressChange' "$SIMPLE_ROOT/simple_handler.h"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0002-engine-adapter-header.patch"
fi
if ! grep -q 'saccade_adapter.h' "$SIMPLE_ROOT/simple_handler.cc"; then
  patch -d "$CEF_ROOT" -p1 < "$REPO_ROOT/engines/cef/patches/0003-engine-adapter-handler.patch"
fi
