# Saccade Windows Dogfood Quickstart

Status: Build 78 installed-product dogfood. Windows x64 uses the same pinned CEF
150.0.11 / Chromium 150.0.7871.115 engine as the macOS build.

```powershell
engines\cef\scripts\fetch_windows.ps1
engines\cef\scripts\build_windows.ps1
engines\cef\scripts\install_windows.ps1
```

The package is written to `target/cef-windows64/Saccade` with a version manifest
and SHA-256 manifest covering every application file. The installer requests a
graceful shutdown, verifies installed processes exited, validates the source,
copies it to a same-volume versioned staging directory, validates the staged
copy, and swaps the complete application directory. It keeps the previous
directory until registration and launch smoke checks succeed, then removes it;
any failure rolls the application directory back.

The profile remains outside the application at
`%LOCALAPPDATA%\Saccade\CEF\Profiles\default` and is never replaced. After the
swap, the installer applies the LPAC ACL, registers Saccade as a browser,
installs the Agent native host, pins the Agent action, creates Desktop and Start
Menu shortcuts, registers the installed MCP for the current Codex user, and
launches Saccade. A normal installed first launch repeats MCP registration
idempotently. It adds a missing entry, repairs an entry already owned by the same
installed executable, and never overwrites a conflicting user-owned entry.

Codex loads MCP configuration when a task starts. After the first install, open a
new Codex task once. No workspace checkout, Python probe, manual control socket,
screenshot route, Playwright or CDP is part of installed operation.

Saccade is the default automatic browser route after registration. The installed
MCP tells every model to use Saccade for browser and website tasks even when the
user does not name Saccade. Registration disables the bundled Browser and Computer Use plugins so the
model's first browser tool call is Saccade MCP, with no competing automatic
route. The user can simply ask for a normal browser task; no routing prompt is
required.

For a Human-created current tab, turn that tab's Agent switch On and call
`saccade.tabs.grant_current`. For an LLM-requested new browser session, call
`saccade.tabs.open_agent`; it creates an Agent-owned On tab without taking over a
Human tab. All browser reads and actions then stay on the installed Saccade MCP.

Latency-sensitive benchmark loops are one MCP call:

```text
saccade.web.reflex_run(tab_id, auto_start=true, max_hits=1000, timeout_ms=30000)
```

The MCP process locally runs `next_fact -> native act -> next_receipt` with zero
LLM calls in the hot loop. It accepts only MouseAccuracy or local test URLs,
requires a matching verified receipt for every counted target, and fails closed
instead of falling back to screenshots or OS input. MouseAccuracy completion is
verified from the same-WebView results truth and cross-checked against the native
receipt count.

Normal profile state is stored under
`%LOCALAPPDATA%\Saccade\CEF\Profiles\default`. Incognito uses an isolated
temporary profile and removes it after the browser exits.

The two-upgrade, stale-file, profile-preservation, and rollback gate is
`engines\cef\scripts\test_windows_staged_upgrade.ps1`.

The package does not bundle Google API keys or OAuth client credentials; core
browsing and the Saccade MCP bridge do not depend on Chrome Sync or other
Google-proprietary services.

Build 78 is unsigned dogfood (`public_distribution_ready=false`). Smart App
Control may still block public downloads until the Windows signing/distribution
track is complete; this is separate from MCP setup and runtime behavior.
