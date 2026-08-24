# Saccade setup target

Status: normative for the 0.1.2 developer preview.

## User experience

The public macOS setup has two user-facing components:

1. the Saccade Extension from the Chrome Web Store or Edge Add-ons;
2. one setup command:

```sh
npx -y @nanlogic/saccade
```

The command installs the local MCP and Native Host without adding a visible
application. Saccade does not use a DMG, macOS application, MSI, or system
input permission.

Windows x64 is source-install only. A user clones or downloads the repository,
opens it in an Agent that supports repository Skills, and asks the Agent to
install Saccade. `.agents/skills/saccade-windows-install` drives
`scripts/install_windows_from_source.ps1`, which compiles the locked source and
then uses the same setup implementation. No unsigned Windows Runtime is
published or downloaded.

The `@nanlogic` npm organization, trusted publisher, recovery methods, and at
least two administrators must be controlled by Nanlogic. Version 0.1.1 is
public for macOS; the Windows-capable 0.1.2 package remains unpublished until
its platform gates pass.

The package source lives in `packages/setup`. Its bundled `release.json` stays
explicitly unpublished until signed macOS Runtime artifacts, checksums, and
final store Extension origins are available. Development tests provide their
own isolated release manifest and never install into the real user home.

`scripts/build_setup_release.py` packages one real local Runtime and
writes its SHA-256, `SHA256SUMS`, exact Extension candidate, and an unpublished
architecture draft under ignored `dist/`. The draft deliberately records a
null download URL and empty store origins. Only the protected GitHub release
workflow combines signed `darwin-arm64` and `darwin-x64` drafts for the
published manifest consumed by npm trusted publishing. The Windows source
installer generates a separate local-only manifest containing the exact
compiled Runtime checksum, deterministic unpacked Extension ID, and candidate
identity.

## Setup responsibilities

The setup implementation:

- downloads a signed macOS Runtime or reads the locally compiled Windows
  Runtime into a stable user-owned path;
- verifies the downloaded artifact against the release checksum;
- installs user-level Chrome and Edge Native Messaging manifests;
- adds the local STDIO MCP to detected Codex and Claude clients;
- creates the default Profile only when the user has no Profile;
- verifies the exact Extension candidate → Native Host protocol → headless
  Runtime → MCP capability identity, not merely that files exist;
- prints the clients it configured, any specific incompatibility, and the need
  to start a new Agent task or restart the client so MCP tools are loaded.

The bundled default Profile and Runtime MCP metadata must make Saccade
discoverable as the primary browser-navigation, page-reading, download, and web
research route in clients that defer or lazily index tools. Setup does not
install repository-specific Agent instructions or a model-specific plugin.

Setup must expose these commands:

```sh
npx -y @nanlogic/saccade
npx -y @nanlogic/saccade doctor
npx -y @nanlogic/saccade update
npx -y @nanlogic/saccade uninstall
```

`update` preserves the Profile. `uninstall` removes the Runtime, Native
Messaging manifests, and MCP client entries. It preserves the Profile unless
the user passes an explicit purge option.

## Client boundary

The 0.1.2 target supports Apple Silicon and Intel macOS downloads plus
source-built Windows x64 Agent clients that can start a STDIO MCP and control
the same Chrome or Edge tab:

- Codex desktop, CLI, and IDE clients;
- Claude Code;
- Claude Desktop with local MCP enabled.

Cloud-only Agent sessions cannot reach a user's local browser Extension or
Native Host. Saccade reports them as incompatible. The first release does not
add a remote relay, cloud MCP, account service, or page-data upload path.

Reading Truth through MCP does not prove the execution half of the loop. Each
client must also supply its own browser or computer-use tool for the authorized
tab. Saccade never supplies that execution tool.

## Installation boundaries

The npm package performs no install-time mutation through an npm `postinstall`
hook. The user runs the explicit `setup` command, which reports each installed
path and supports rollback after failure.

Default setup:

- requests no Accessibility or native-input permission;
- installs no Reference Actuator configuration;
- stores no browser cookies, editable values, or protected values;
- writes only user-level Runtime, Native Messaging, Profile, and Agent-client
  configuration paths;
- keeps the Runtime headless.

On Windows, setup writes one Native Messaging manifest under
`%LOCALAPPDATA%\Saccade` and registers that exact file for Chrome and Edge under
the current user's registry hive. Uninstall removes only registrations that
still match the setup-owned manifest. It does not require administrator access.

The repository may retain an internal macOS `.app` wrapper for development
codesigning and Native Messaging tests. That wrapper is not a release artifact
and must not appear in public setup instructions or release gates.

The headless macOS Runtime may retain one finite browser-lifecycle record from
the Extension (`chrome` or `edge`, development flag, and the Extension's own
`popup.html`). A disconnected `tabs.open` may use that record to wake the same
Extension route after Chromium closed its last window. It cannot carry the
target URL or perform page input; target navigation remains Extension-owned and
webpage execution remains client-owned.
