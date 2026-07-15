# CEF Day 5 Dogfood Release Report

Date: 2026-07-15
Status: engineering gates passed; live Gist collaboration passed

## What shipped

- The official CEF Chrome Runtime remains the human browser surface, including
  its tabs, address bar, navigation controls, menus, and Chromium renderer.
  Chromium's BrowserView remains the direct macOS window child. The first
  trust-strip implementation was removed after a physical-input gate proved
  that both overlay and nested-panel variants swallowed page mouse events.
- `bin/open-saccade` is the explicit collaboration entry point and grants only
  its visible tab to the owner-only bridge. Opening `Saccade.app` directly does
  not start or grant an agent session.
- Agent grants follow the focused CEF tab. Opening a child tab creates a new
  tab identity; closing it returns control to the remaining visible tab without
  killing the bridge.
- Normal profiles persist outside the repository. Incognito profiles are
  disposable. The official CEF `root_cache_path` and `cache_path` are set from
  the selected user-data directory.
- `article_text` provides a bounded, redacted reading route without exposing
  cookies, storage, sensitive control values, or arbitrary JavaScript.
- The macOS dogfood builder creates a fixed-identity signed app, exact
  CEF/Chromium metadata, license inventory, portable SHA-256 checksums, and
  owner-only launch/grant tools.

## Measured gates

| Gate | Result | Evidence |
| --- | --- | --- |
| No-CDP reflex | PASS: 300/300, zero misses; run p95 3.1-3.4 ms | `runs/cef_day5/day3_3x100/aggregate.json` |
| Original MouseAccuracy | PASS: START plus 12/12 live targets; 10.6 ms p95 | `runs/cef_day5/mouseaccuracy/report.json` |
| FORMMAX | PASS: 672 verified fills, 3 protected fields blocked, 2 page receipts, no values logged | `runs/cef_day5/formmax/report.json` |
| Forms, safety, rich editor, screenshot, replay | PASS: hidden controls filtered, ordinary fields verified, protected writes rejected, rich editor visible/backing surfaces agreed, sensitive screenshot blocked, sentinel scan clean | `runs/cef_day5/form_safety_native_rich_editor_final/report.json` |
| Physical human input | PASS: macOS CoreGraphics HID click plus focused typing, with no browser input API | `runs/cef_day5/human_input_final/report.json` |
| Tabs and profile restart | PASS three consecutive times; child-tab focus, close recovery, and local state persistence | `runs/cef_day5/session_consistency_1/report.json` through `_3/report.json` |
| Public article | PASS: 9,360 redacted characters and headings, no CDP or screenshot | `runs/cef_day5/public_article/report.json` |
| Logged-in Gist collaboration | PASS: 25 DOM controls reduced to seven visible fields; description, filename, and rich-editor body verified; Wayne confirmed the same visible draft and retained submit control | `runs/cef_day5/gist_live_rich_editor_20260715/report.json` |

## Keychain behavior

CEF/Chromium uses the macOS `Chromium Safe Storage` Keychain item to encrypt
persistent cookies. A fixed Developer ID signature is therefore mandatory for
Normal profiles. The first signed launch may require one macOS `Always Allow`
authorization. Repeated prompts indicate that an ad-hoc/debug app or a changing
path was used. Saccade does not use CEF's test-only mock Keychain in saved-profile
product runs, and the agent bridge never receives the Keychain secret or raw
cookies.

Choosing a nearby passkey/security-key route may make Chromium enumerate
Bluetooth authenticators. The signed app now declares the required Bluetooth
usage descriptions so macOS asks for permission instead of terminating the
process. Touch ID passkey support is not claimed until the required WebAuthn
keychain-access-group entitlement is provisioned and retested.

## Remaining acceptance

1. Re-run the same saved GitHub/Gist product profile from one stable signed app
   path without the test-only mock Keychain flag, then confirm login persistence
   and zero repeated Keychain prompts.
2. Close OAuth/password child windows automatically after their flow completes;
   the live Gist test left stale `Discover gists` and password windows.
3. Start the local game at `http://127.0.0.1:4173/` and rerun the CEF reflex
   gate. The server was not running during this Day 5 closeout, so no new CEF
   game-action result is claimed here.

This is a signed local macOS dogfood build, not a notarized public release.
Public distribution still requires a project-license decision, hardened-runtime
and notarization policy, and a clean-machine installation test.
