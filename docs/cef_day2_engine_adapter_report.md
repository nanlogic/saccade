# CEF Day 2 Engine Adapter Report

Date: 2026-07-14
Result: `DAY2_ENGINE_ADAPTER_GATE=PASS`

## Delivered

- Added `saccade_engine_api`, a Rust boundary with contract `1.0` types for
  capabilities, opaque tab identity, origin, page revision, fact batches,
  action maps, input receipts, and typed errors.
- Added an engine-neutral control protocol to the CEF browser process. It uses
  a one-session Unix socket and bearer capability, not CDP, WebDriver, page
  JavaScript, or a browser extension.
- Updated `saccade-mcp` to attach by advertised capabilities. Existing Servo
  and Chrome-reference grants remain compatible; new host code does not need
  an engine name.
- Updated the Python and TypeScript integration examples with a lifecycle-only
  path: negotiate contract, attach the granted tab, navigate, inspect status,
  pause, and close.

The official `cefsimple` target and helper lifecycle remain intact. The build
applies three small, tracked source patches and adds one adapter source pair.

## Security Boundary

- The socket, grant, and parent session directory deny group/other access.
  The measured modes were `0600`, `0600`, and `0700` respectively.
- CEF generates a fresh 256-bit session capability inside the browser process.
  It is stored only in the owner grant and compared in constant time.
- Grant rewrites use a private temporary file plus atomic rename. The grant is
  not published until the main frame has a non-empty URL.
- Closing the granted tab removes the socket, grant, temporary file, bearer
  capability directory, and browser process.
- The adapter exposes no cookies, storage, form values, protected values,
  screenshots, or page-DOM binding. Its Day 2 capabilities are only `ping`,
  `shell_status`, `navigate`, `pause`, and `close`.

Engine-neutral grants outside the repository must be absolute, regular,
owner-only files. The same permission check applies if such a grant is placed
inside the repository. Legacy loopback TCP remains accepted only for existing
adapters.

## Gate Evidence

The release CEF app was rebuilt after the adapter changes. The automated gate
then launched a fresh CEF process for each host example and produced the same
result twice:

```text
contract=1.0
attached=true
navigate_status=ok
current_url=...cef_adapter_lifecycle.html?navigated=<host>
pause_status=ok
capabilities=ping,shell_status,navigate,pause,close,saccade.browser.navigate
```

Python and TypeScript completed together in 8.7 seconds. Both browser
processes exited cleanly, and both private session directories disappeared.
The gate runs hidden, incognito, and with Chromium's test-only mock keychain so
automated verification cannot touch a normal profile or create macOS Keychain
prompts. Product launchers never enable the mock keychain.

Relevant checks:

```sh
cargo test -p saccade_engine_api -p saccade-mcp
engines/cef/scripts/build_macos.sh
engines/cef/scripts/test_day2_macos.sh
cargo fmt --all -- --check
git diff --check
```

## Not Day 2

This adapter does not yet emit page truth or dispatch reflex input. Those are
Day 3 gates and must pass the existing 3x100 no-CDP test before the CEF path can
claim Saccade's millisecond truth/action loop.

The unchanged CEF sample still warns that `root_cache_path` uses its default
on ordinary normal-profile launches. Earlier attempts to set it caused a
macOS Keychain/shutdown hang, so concurrent named profiles are not claimed.
Final app identity, Developer ID signing, normal-profile Keychain behavior,
and that profile-root follow-up remain release hardening work.
