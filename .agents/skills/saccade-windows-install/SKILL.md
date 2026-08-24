---
name: saccade-windows-install
description: Build and install Saccade from its checked-out source tree on Windows x64. Use when a user asks an AI to install, repair, verify, or uninstall Saccade on Windows without downloading an unsigned Runtime binary.
---

# Saccade Windows Install

Complete the current-user installation from source and leave a verified Runtime,
Native Messaging registration, and detected Codex/Claude MCP entries. Do not
publish or redistribute the locally compiled executable.

## Install

1. Resolve the repository root from this `SKILL.md`; do not assume the user's
   current directory. Require Windows x64 and a complete source tree.
2. Run `scripts/install_windows_from_source.ps1` from the repository root in
   Windows PowerShell or PowerShell. This builds the locked Rust release,
   derives the deterministic unpacked Extension ID, installs through Saccade
   Setup, and runs doctor.
3. If the script reports `PREREQUISITES_REQUIRED`, ask one yes/no question for
   permission to install exactly the missing Node.js, Rust MSVC, or Visual
   Studio C++ Build Tools packages. On yes, rerun with `-Bootstrap`. On no,
   stop with the missing list. Do not ask routine planning questions.
4. If it reports `SACCADE_EXTENSION_PENDING`, use an available computer/UI
   tool to open Chrome or Edge extensions, enable Developer mode, and load the
   exact `extension_path` as unpacked. Remove or replace only an older unpacked
   Extension clearly named Saccade; never remove unrelated extensions. Confirm
   the displayed ID and version equal the marker.
5. If no computer/UI tool can operate the browser, the script has already
   opened the extensions page. Ask only one question: whether the exact
   Extension shown in the marker is now loaded (`yes`/`no`). Continue on yes.
6. Run `node packages/setup/bin/saccade-setup.js doctor`. Success requires all
   local checks plus `exact Extension → Native Host → Runtime → MCP candidate
   and contract`. Preserve complete output for any failure.
7. Report the installed Runtime path and Extension ID. Explain that an already
   running Codex or Claude task cannot acquire a newly registered MCP; a new
   task or client restart is the only remaining step.

The user's install request authorizes the repository build, Saccade-owned files
under `%LOCALAPPDATA%\Saccade`, its two documented current-user Native Messaging
registry keys, and additive MCP configuration for detected clients. It does not
authorize changing Windows security policy or installing missing global build
tools without the single yes/no approval above.

## Repair and uninstall

- Repair by rerunning the source installer; it is idempotent and preserves the
  Profile.
- Uninstall only through
  `node packages/setup/bin/saccade-setup.js uninstall`; use `--purge` only when
  the user explicitly requests full data removal.
- Never manually delete registry content. If Windows Application Control blocks
  a locally compiled process, do not weaken the policy; report the exact blocked
  executable and stage.
