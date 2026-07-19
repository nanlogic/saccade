# Saccade Dogfood Browser Quickstart

Date: 2026-07-17
Status: installed self-contained MCP handoff is the default

The current package opens as a normal Chrome-style browser window. It includes
a tab strip, `+` new tab, address bar, Back, Forward, and Reload/Stop. Click the
address bar or press Command-L, type a URL, and press Return.

## Automatic Codex setup

Drag `Saccade.app` into `/Applications` and open it once from each macOS user
account. If Codex is installed, Saccade idempotently registers its signed MCP
launcher for that user without replacing a conflicting user-owned entry. Open
a new Codex task or restart Codex once after the first registration.

Help -> Connect Saccade to Codex repairs a missing or stale `saccade` entry.
For another MCP-capable LLM, use the same client-neutral server entry:

```toml
[mcp_servers.saccade]
command = "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp"
```

The signed MCP server is inside the app. It does not require Rust, Python, a
CEF cache, or the source repository at runtime. Replacing `Saccade.app` with a
new build keeps the command unchanged.

The installed MCP advertises only its self-contained product tools. Source-only
developer audits, report validators and Cargo-backed fixtures are intentionally
absent from the installed tool list instead of failing with developer paths.

The configured command is:

```text
/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp
```

This is client configuration only. Do not copy a grant file or capability into
the configuration or chat. When `saccade` is available to an agent session, it
is the required first browser route; clients must not silently fall back to a
different browser when Saccade needs repair or reconnection.

## Start dogfood

Open `Saccade.app` normally. Human-created tabs begin with `Agent Off`. Turn
the browser-owned switch On when you want the LLM to use the visible tab.

If the LLM starts the browsing session, it calls `saccade.tabs.open_agent`.
Saccade opens a new Agent On tab in the existing process, or starts the signed
installed app and opens that tab if Saccade is not running. It never silently
turns a Human tab On.

## Talk normally

Examples:

```text
Read the current Saccade article. Summarize it and tell me whether it helps our project.
Research the site I have open and compare its interaction loop with our game.
I entered the SSN myself. Fill the remaining ordinary fields, preserve what I typed, and do not submit.
Download the free sound asset from this page and tell me when it finishes.
```

The LLM first discovers the running grant with zero-argument
`saccade.tabs.grant_current`. It can then read bounded article text, inspect
redacted truth/actions, or compile and execute a revision-bound ordinary-field
plan. SSNs, passwords, OTPs, payment fields, signatures, and legal attestations
remain human-only.

Downloads use the normal Chromium download UI. On an Agent On tab, the LLM may
trigger a verified page download and query `saccade.downloads.list` for its
file name, MIME type, source origin, size, progress and final status. Saccade
does not expose the full local path or file contents and never auto-executes a
downloaded file.

## Developer diagnostics

Source checkouts still produce `dist/saccade-cef-dogfood-current` with the
compatibility launchers and probes. Installed product use does not depend on
that directory.

## Clean macOS user validation (checklist 14)

1. In System Settings -> Users & Groups, create a new Standard user and log in
   to that account. Do not copy the source repository or the original user's
   Saccade Application Support directory.
2. Confirm the App is available at `/Applications/Saccade.app`.
3. Open Saccade once. For Codex, confirm that Saccade registered this command
   automatically, then open a new Codex task. For another MCP-capable LLM, add
   the command manually and restart the LLM:

   ```toml
   [mcp_servers.saccade]
   command = "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp"
   ```

4. Open Saccade manually. Create two tabs, leave one Agent Off, turn the other
   Agent On, and ask the LLM to list Agent tabs and read the On tab. Exactly one
   tab must be discoverable; the Off tab must remain absent.
5. Quit Saccade. Ask the LLM to open a URL in Saccade. The installed MCP must
   start the App and create one new Agent On tab without using a repo path.
6. Have the administrator replace `/Applications/Saccade.app` with the next
   build. Do not edit the new user's MCP configuration. Repeat steps 4 and 5;
   both must still pass with the same command.

Gatekeeper acceptance of a downloaded DMG without a workaround is checklist 15,
not checklist 14.

On the first saved-profile launch, macOS may ask Saccade to access Chromium Safe
Storage. Verify that the requesting app is signed Saccade and choose Always
Allow once. Repeated prompts indicate that an unsigned/ad-hoc build touched the
saved profile; the release launcher rejects such builds.

## Current evidence

- Product flow: `docs/ai038_conversational_dogfood.md`
- Packaged gate: `runs/dogfood/ai038_packaged_gate_final_20260715/report.json`
- Agent safety: `docs/ai033_agent_safety.md`
- Human/agent agreement: `docs/ai034_github_human_agent_agreement.md`
- Installed MCP clean-room Build A:
  `runs/dogfood/df_r14_installed_build_a3_20260717/report.json`
- Installed MCP clean-room Build B upgrade:
  `runs/dogfood/df_r14_installed_build_b_20260717/report.json`
- Installed build 44 download gate:
  `runs/dogfood/df_downloads_installed_20260717/report.json`
