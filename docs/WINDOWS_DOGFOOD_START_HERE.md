# Saccade Windows dogfood: start here

Date: 2026-07-21

## Readiness verdict

Windows implementation and installed-product dogfood are complete through
Build 78. The Build 76 final live gate passed MouseAccuracy Hard + Tiny and a
reversible SimpleMMO A/B through the installed MCP with same-WebView native-input receipts
and no browser fallback. The adapter and Rust engine contract use an owner-only
Windows named pipe; loopback TCP remains prohibited.

Build 77 added cross-origin embedded-iframe form inventory, planning, and fill.
Build 78 binds explicit field inspection to the fresh inventory revision and
adds the iframe inspection regression before compile/fill.

Build 78 is unsigned dogfood. Public distribution is not ready until the
Windows signing and reputation track is complete. See
`docs/windows_dogfood_quickstart.md`, `docs/work_ledger.md`,
`runs/windows_dogfood/build76_final_live_gate/report.json`, and
`runs/windows_dogfood/build78_installed_iframe_inspect/report.json` for current
status and evidence.

The Windows CEF archive is pinned to the same CEF and Chromium revisions as the
macOS build in `engines/cef/cef.windows64.lock.json`.

## Machine prerequisites

- Windows 10 or 11 x64
- Visual Studio 2022 with **Desktop development with C++** and a Windows SDK
- CMake
- Python 3
- Git
- Rust stable with host `x86_64-pc-windows-msvc`
- Codex installed and logged in for the final MCP onboarding gate

No Administrator access is required for the first portable dogfood package.

## First commands

```powershell
git clone https://github.com/nanlogic/saccade.git
cd saccade
powershell -ExecutionPolicy Bypass -File .\engines\cef\scripts\preflight_windows.ps1 -FetchCef
cargo test
```

The preflight downloads the pinned CEF archive into the per-user Saccade CEF
cache, checks its SHA-1 and SHA-256, and prints the extracted CEF root.

## Windows milestone order

W0-W4 are complete for Build 78. Keep these gates for clean rebuilds and
regressions.

### W0 - pinned CEF and toolchain

Gate:

```powershell
powershell -ExecutionPolicy Bypass -File .\engines\cef\scripts\preflight_windows.ps1 -FetchCef
```

Expected: `WINDOWS_PREFLIGHT PASS`.

### W1 - owner-only named-pipe transport

- Add `owner_only_windows_pipe_v1` to the C++ adapter.
- Generate a random `\\.\pipe\saccade-...` name per session.
- Restrict the pipe DACL to the current user SID.
- Add a Windows named-pipe address to `saccade_engine_api`.
- Validate grant and pointer file ownership/DACL; the non-Unix `is_file()`
  fallback is not a production security boundary.
- Keep TCP, CDP, WebDriver and remote debugging disabled.

Gate: Rust transport tests plus a Windows browser-process `ping` over the named
pipe, including denial from a second Standard user.

### W2 - native browser parity

- Build the Chrome Runtime UI with normal tabs, address bar, Back/Forward and
  downloads.
- Implement the browser-owned Win32 Agent On/Off control for the selected tab.
- Direct human tabs default Off; MCP-created tabs default On.
- Implement the protected passport/driver-document prompt; password, OTP and
  CVV remain unreadable and non-fillable.

Gate: two Agent On tabs plus one Off tab; MCP discovers only the On tabs and
cannot read the Off tab even when asked directly.

### W3 - stable portable package and Codex onboarding

- Package `Saccade.exe`, CEF runtime files, `saccade-mcp.exe` and
  `saccade-current-tab-mcp.exe` together.
- Install per user under `%LOCALAPPDATA%\Programs\Saccade` for dogfood.
- Store profiles and grants under `%LOCALAPPDATA%\Saccade\CEF`.
- First launch idempotently runs the installed Codex CLI equivalent of:
  `codex mcp add saccade -- <stable absolute MCP launcher>`.
- Preserve conflicting user-owned entries; expose explicit Connect/Repair.

Gate: a clean Standard user needs no source checkout and no manual MCP config.
Replacing the package does not change the configured MCP command.

### W4 - dogfood parity

Run the article, multi-tab consent, ordinary form, protected local fill,
download, media, resize/Canvas rebase, profile persistence and receipt gates.
Record Defender and SmartScreen behavior instead of inferring it.

## Historical bootstrap prompt

The prompt below is retained as implementation history. Do not treat its W0/W1
scope as current project status.

```text
Read AGENTS.md, SACCADE_BUILD_SPEC_v4.md, docs/windows_dogfood_plan.md and
docs/WINDOWS_DOGFOOD_START_HERE.md. Work on W0 then W1 only. Run the Windows
preflight first. Implement owner_only_windows_pipe_v1 end to end in the C++
browser adapter and Rust saccade_engine_api; never add TCP, CDP or WebDriver.
Use a random named pipe with a DACL limited to the current user SID. Finish with
literal test commands and evidence. Do not begin W2 until W1 passes.
```
