# Saccade Windows Dogfood Plan

Date: 2026-07-18
Status: next-platform implementation contract
Target: Windows 10/11 x64 first; ARM64 only after x64 parity

## Product rule

Windows is the same Saccade product, not a reduced compatibility build. It must
ship the same engine contract, MCP tool names, per-tab Human/Agent permission,
redacted renderer truth, revision/layout binding, receipts, protected-value
isolation, downloads, Chromium profile behavior and per-user Codex onboarding
as macOS Build 64.

## Reuse

- Pin the Windows x64 CEF binary matching the current CEF/Chromium revision.
- Reuse `saccade_adapter`, renderer collector, form script, brand resources,
  action/receipt logic and Rust MCP implementation.
- Reuse the Saccade `.ico`, Apache/NOTICE/trademark files, CEF license,
  Chromium credits, SBOM generator and acceptance fixtures.
- Keep Chrome Runtime UI, CEF sandbox, GPU process separation and remote
  debugging disabled.

## Windows-native work

1. Add a pinned Windows CEF fetch/build/package path using Visual Studio 2022
   and CMake. Package `Saccade.exe`, CEF DLLs/resources/locales and
   `saccade-current-tab-mcp.exe` in the official Windows layout.
2. Extract the POSIX socket/file implementation from `saccade_adapter.cc`.
   Implement `owner_only_windows_pipe_v1` with a random named pipe and a DACL
   restricted to the current Windows user SID. Do not permit loopback TCP.
3. Extend `saccade_engine_api` with a Windows named-pipe transport and validate
   the grant/pointer files' owner and DACL. The current non-Unix `is_file()`
   fallback is not sufficient for production.
4. Store profiles under `%LOCALAPPDATA%\Saccade\CEF\Profiles\default` and
   Agent grants/replay under `%LOCALAPPDATA%\Saccade\CEF\Agent`. Session pipe
   names include the user SID plus random entropy; values never enter them.
5. Add the visible browser-owned Agent On/Off control as a Win32 owned titlebar
   accessory anchored to the active Chrome Runtime window. It must remain
   outside page DOM, track the selected tab and be human-clickable only.
6. Add a Win32 modal protected-value prompt. Passport and driver-document
   values go directly to the selected field and are cleared from temporary
   buffers; password, OTP and CVV remain unreadable and non-fillable.
7. Make installed MCP launch/reuse `Saccade.exe`, create one Agent On tab, and
   preserve Human Off tabs exactly as Build 64 does.
8. On each Windows user's first launch, detect Codex and idempotently register
   the installed `saccade-current-tab-mcp.exe`. Never replace a conflicting
   user-owned MCP entry without an explicit Connect/Repair action.

## Dogfood packaging

The first internal package is a portable Windows x64 ZIP plus an installer
script that chooses a stable per-user path. It must not require Administrator
access. Codex registration is automatic per user; the installer must also print
the exact absolute MCP command for other LLM hosts. Replacement builds keep
that command and profile location.

For public distribution, Authenticode-sign Saccade-owned executables and the
installer, timestamp them, verify with `signtool verify /pa`, and publish the
SBOM/checksums. Microsoft Artifact Signing Public Trust is the preferred
non-Store signing route; internal dogfood may remain explicitly unsigned and
will therefore encounter Windows reputation warnings.

## Acceptance gate

- Direct human launch creates Agent Off; MCP cold start creates Agent On.
- Running-process reuse, Agent child tabs, two-On/one-Off discovery and Off
  physical unreadability match Build 64.
- Article, form, protected local fill, download, resize/Canvas rebase and
  receipt gates pass with no Cookie/storage/protected-value exposure.
- Normal profile survives replacement; private/incognito state does not.
- Clean Standard Windows user installs outside the repository and uses only
  the stable packaged MCP command.
- Windows Defender/SmartScreen behavior and every executable signature are
  recorded rather than inferred.

## Build-machine requirement

The native CEF host, Win32 control, named-pipe ACL and installer must be built
and exercised on the Windows machine. macOS can prepare shared source changes,
but cannot certify Windows UI, sandbox, ACL, Defender, SmartScreen or signing
behavior.
