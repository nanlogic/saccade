# CEF macOS Signing and Keychain Report

Date: 2026-07-15
Status: local signed-profile and notarization-preflight gates passed; Apple
submission remains intentionally deferred

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

## Hardened Runtime preparation

Build 62 enables Hardened Runtime and secure timestamps for the main app,
embedded MCP binaries, CEF framework and helper executables. The helpers used
for Renderer/GPU work receive only `com.apple.security.cs.allow-jit`, matching
their Chromium execution role. The no-upload preflight verifies Developer ID,
runtime flags, timestamps, the absence of `get-task-allow` and every nested
Mach-O signature.

`engines/cef/scripts/notarize_macos.sh submit` is the explicit release-owner
path. It notarizes and staples the App before building, signing, notarizing and
stapling the DMG, then runs App and DMG Gatekeeper assessment. It requires a
Keychain-stored `notarytool` profile and is never called by dogfood builds.

## Remaining Boundary

This closes the same-machine signed normal-profile and repeated-Keychain-prompt
gate. It does not claim a public distribution artifact. Apple submission,
stapling, offline clean-machine install and certificate-rotation tests remain
required before public release.

Build 62 evidence: `runs/dogfood/df_build62_release_complete_20260718/report.json`.

## 2026-07-18 locked-Keychain recurrence

A Keychain prompt recurred even though installed Build 65 matched the packaged
app and Builds 64/65 had the same bundle ID, Team ID and designated
requirement. At recurrence, strict verification reported
`CSSMERR_TP_NOT_TRUSTED` and the login Keychain exposed no valid signing
identity, while the extracted certificate chain itself still verified.

The user authenticated through the macOS Keychain UI using the system passcode.
The unchanged installed app then passed three complete quit/relaunch cycles:
one Saccade main process and zero `SecurityAgent` processes on every run. The
measured cause was the locked login Keychain, not bundle, Team ID, designated-
requirement, or package drift.

Saccade keeps the real Chromium Safe Storage backend and does not patch the CEF
binary or enable `--use-mock-keychain`. macOS authentication after the user
explicitly locks the login Keychain is an OS security boundary, not a prompt
that the browser should bypass. Sleep/wake remains a separate release check.
