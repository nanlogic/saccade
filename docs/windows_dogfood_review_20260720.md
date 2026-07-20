# Windows CEF Dogfood Review

Date: 2026-07-20

Reviewed branch: `origin/agent/windows-cef-dogfood`

Reviewed commit: `aee5a22` (`add Windows CEF dogfood support`)

## Outcome

The Windows branch establishes the right product direction: native CEF Chrome
Runtime UI, a persistent normal profile, an owner-only control transport,
automatic installed MCP registration, fail-closed Saccade-first host
instructions, and a local `saccade.web.reflex_run` hot loop. The new Windows UI
should be preserved.

Do not merge the branch to `main` yet. Four P0 runtime issues need correction,
followed by one Windows-only SimpleMMO compatibility experiment. The current
change is also unusually large for one commit: 77 files and approximately 7,050
insertions. Split or stage the final integration so the transport, installer,
reflex loop, and UI can be reviewed independently.

## P0-1: MouseAccuracy deadline begins before the game

`web_reflex_run_tool` creates the overall deadline before searching for and
clicking START. The documented example uses `timeout_ms=30000` for a 30-second
game. START discovery, native input, the verified receipt, and navigation all
consume part of that same 30 seconds. The hot loop therefore stops before the
game ends and can miss the final target. This is a direct explanation for a
result near 95 percent.

Required change:

1. Use a separate bounded `start_timeout_ms` for START discovery and receipt.
2. Require a verified START receipt when `auto_start=true`.
3. Begin the game deadline only after that receipt and destination readiness.
4. Give result-page parsing a separate bounded settlement window.
5. Do not clamp result settlement to an already-expired game deadline.

Acceptance:

- Hard + Tiny completes through one MCP `reflex_run` call.
- The hot loop remains active through the complete game duration.
- The report records no screenshot, CDP, Playwright, WebDriver, OS-input, or
  external-browser fallback.

## P0-2: MouseAccuracy can report a false PASS

The current implementation treats any of the following as completion:

```text
results_passed || finished || verified_receipts >= max_hits
```

It then emits `verdict=PASS` for any completed result. Reaching a caller-supplied
`max_hits`, or observing a generic page `finished` receipt, is not proof of a
100-percent MouseAccuracy result.

For `mouseaccuracy.com`, PASS must require same-WebView result truth proving all
of the following:

```text
target_efficiency_pct == 100
click_accuracy_pct == 100
targets_hit == targets_total
clicks_hit == clicks_total
verified_receipt_count == targets_hit
```

Missing results, a parse failure, timeout, max-hits termination, or a receipt
count mismatch must return `FAIL` or `INCOMPLETE`, never PASS. Local fixtures may
retain a separate explicitly named completion policy.

## P0-3: Windows named-pipe calls have no effective read timeout

The Windows `call_windows_named_pipe` implementation accepts `read_timeout` as
`_read_timeout`, opens the pipe synchronously, and calls `transact` without
enforcing a read or write deadline. If the browser-side bridge stops responding,
an MCP tool call can block indefinitely. This can present as a frozen Codex task
or a website operation that never finishes.

Required change:

- use overlapped named-pipe I/O or another Windows API path with real connect,
  write, and read deadlines;
- call `CancelIoEx` when a deadline or user cancellation fires;
- map deadline expiry to the typed `SACCADE_TIMEOUT` error;
- add a Windows test server that accepts a connection but deliberately withholds
  a response, then prove the client returns within the configured deadline;
- do not silently reconnect and replay a mutation after an ambiguous timeout.

## P0-4: Installation overwrites a potentially running package in place

`install_windows.ps1` recursively copies the new package directly over
`%LOCALAPPDATA%\Programs\Saccade`. It does not first stop the old app, remove
obsolete package files, stage and validate the replacement, atomically swap the
install directory, or roll back on failure. Repeated dogfood installs can
therefore leave a mixture of old and new DLLs, resources, extensions, or MCP
binaries.

Required replacement flow:

```text
request graceful Saccade shutdown
verify package processes have exited
copy into a versioned staging directory
validate required files, version manifest, and checksums
preserve the profile outside the application directory
atomically replace the active application directory
restore the previous version on failure
re-register MCP/native host/default-browser entries
launch and run an installed-product smoke test
```

Never delete or overwrite `%LOCALAPPDATA%\Saccade\CEF\Profiles` during an app
replacement.

## SimpleMMO Windows bot classification

The reviewed product launch path does not add `--headless`,
`--enable-automation`, `--remote-debugging-port`, `--disable-gpu`, or incognito
by default. The Windows and macOS locks use the same CEF and Chromium versions,
and the Windows build requests `USE_SANDBOX=ON`. The internal `cef:*` tab id
proves Saccade attached to a CEF tab, but it is not a web-visible fingerprint.

The most important Windows-only browser-surface difference is the unpacked
extension loaded through:

```text
--load-extension=...\extensions\saccade-new-tab
```

That extension implements the improved New Tab identity and Agent toolbar
action. Preserve the UI, but isolate whether command-line unpacked-extension
loading contributes to the provider decision.

Run these tests on the same Windows machine, network, profile state, and site
URL, with Saccade fully closed between runs:

1. Normal installed Saccade.
2. The same Saccade executable and profile with extensions disabled, for a
   human-only compatibility observation. Do not claim an Agent test in this
   mode.
3. Normal Chrome on the same machine and network.

Record only non-secret evidence:

- exact visible rejection message;
- top-level HTTP status and provider name where available;
- `navigator.webdriver`, user agent, platform, languages, time zone,
  device-pixel ratio, and WebGL vendor/renderer;
- Saccade `shell_status` human-verification fields;
- profile mode, sandbox status, GPU status, and launch switches.

Do not bypass, solve, spoof, or automate a human-verification challenge. If
disabling the extension changes the result, retain the visual design but move
to a packaged/controlled extension-loading mechanism rather than a command-line
unpacked extension.

## Additional required cleanup

### Incognito truth is mislabeled

`SaccadeDirectSessionWin` sets `SACCADE_PROFILE_MODE=normal` and
`SACCADE_PROFILE_NAME=default` unconditionally. An incognito launch may use
isolated storage correctly while reporting the wrong profile truth. Derive the
reported mode from the actual command line/session configuration.

### Owner-only pipe ACL must fail closed

If construction of the current-user security descriptor fails, named-pipe
creation currently falls back to default security attributes. The advertised
`owner_only_windows_pipe_v1` contract must instead refuse to start the bridge.
The capability token remains required but is not a substitute for the transport
ACL guarantee.

### Build portability

Remove the hard-coded `C:\Users\wayne\...\cmake.exe` fallback. Discover tools
through PATH or explicit parameters and report a clear missing-tool error.

The current package combines Release CEF with a Debug MCP artifact because of a
local WDAC issue. Keep this dogfood exception explicit; it is not a public
release configuration.

### Branding should target known resource ids

The Windows locale branding script replaces every occurrence of `Chromium`
across every locale pack. Keep the improved Saccade UI, but target reviewed
resource ids so legal, diagnostic, accessibility, and unrelated strings are not
changed accidentally.

### Build preparation is too mutation-heavy

The branch uses many sequential PowerShell source-rewrite and repair scripts.
Collapse the accepted result into authoritative cross-platform source plus a
small deterministic patch set. A clean checkout should produce the same source
tree and package without depending on historical repair order.

## Validation completed during this review

The following passed from a detached checkout of commit `aee5a22` on macOS:

```text
cargo fmt --all -- --check                         PASS
cargo check -p saccade-mcp                         PASS
cargo test -p saccade-mcp                          PASS (27/27)
cargo test -p saccade_engine_api --lib             PASS (5/5)
git diff --check main...aee5a22                    PASS
```

These checks validate formatting and platform-neutral Rust behavior. They do
not validate Windows compilation, named-pipe cancellation, installed-package
replacement, Windows UI behavior, MouseAccuracy completion, or SimpleMMO
compatibility.

## Merge gate

Keep the new Windows UI. Merge only after:

- P0-1 through P0-4 are fixed and regression-tested;
- MouseAccuracy Hard + Tiny produces verified 100-percent result truth through
  one public MCP call;
- named-pipe timeout and cancellation tests pass on Windows;
- two consecutive staged upgrades preserve the profile and leave no stale app
  files;
- the SimpleMMO A/B result is recorded without anti-bot bypass;
- the final Windows commit set is reviewable and reproducible from a clean
  checkout.
