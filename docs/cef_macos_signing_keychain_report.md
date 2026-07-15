# CEF macOS Signing and Keychain Report

Date: 2026-07-15
Status: local signed-profile gate passed; public notarization remains open

## Result

The CEF Release app is now signed with a stable Developer ID identity instead
of the linker-generated ad-hoc signature:

- bundle identifier: `ai.saccade.browser`
- team identifier: `48KK2UWXQM`
- authority: `Developer ID Application`
- main app, five helper apps, and the CEF framework pass strict code-signature
  verification

The old build's designated requirement was a changing `cdhash`. macOS therefore
treated rebuilt copies as different applications when they requested the
existing `Chromium Safe Storage` Keychain item. Chromium uses that item to
derive the encryption key for cookies, login sessions, and saved credentials.

Wayne approved the one-time migration from the previous ad-hoc identity with
`Always Allow`. Two subsequent launches of the same signed normal profile each
reported zero `SecurityAgent` windows and loaded `Example Domain` normally.
The test-only `--use-mock-keychain` switch was not used for this profile.

## Shutdown Regression

The signed-profile check exposed a separate CEF 150 macOS quit crash. The
official sample's `SimpleApplication::terminate` assumed the application
delegate implemented `tryToTerminateApplication:`. The Chrome Runtime supplied
an `AppController` that did not implement that selector.

Patch `0009-macos-quit-delegate-fallback.patch` keeps the official delegate
path when available and otherwise calls the existing
`SimpleHandler::CloseAllBrowsers(false)` path. The rebuilt and re-signed app
then passed an external quit regression with `exit_code=0`, no exception, and
normal `CefQuitMessageLoop`/shutdown behavior.

## Reproduce

```sh
SACCADE_CODESIGN_IDENTITY=auto engines/cef/scripts/build_macos.sh
SACCADE_PROFILE_NAME=cef-signed-keychain \
  engines/cef/scripts/run_macos.sh normal https://example.com
```

## Remaining Boundary

This closes the same-machine signed normal-profile and repeated-Keychain-prompt
gate. It does not claim a public distribution artifact. Hardened-runtime
entitlements, notarization, stapling, clean-machine install, and certificate
rotation tests remain required before public release.
