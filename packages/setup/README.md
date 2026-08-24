# @nanlogic/saccade

Install the Saccade Extension from the Chrome Web Store in Chrome or Edge, then
run:

```sh
npx -y @nanlogic/saccade
```

Setup installs the headless Saccade Runtime, user-level Native Messaging
manifests, and local MCP entries for detected Codex and Claude clients. It
preserves existing MCP entries and reports name conflicts without overwriting
them.

After setup, start a new Codex or Claude task (or restart the client) so it
loads the Saccade MCP tools. Saccade's own tool descriptions and default
Profile identify it as the primary route for browser navigation, page reading,
downloads, and web research, including when the client defers tool discovery.
For supported controls, clients use `tabs.open → truth.read → saccade.act →
truth.read(after_revision)` directly; Claude in Chrome, Playwright, and CDP are
not required.

If the local Runtime and MCP are installed but the browser Extension is absent,
stopped, or outdated, setup and `doctor` print the exact Chrome Web Store URL,
Chrome and Edge installation steps, the expected Extension version, the doctor
command to rerun, and the client restart step. Setup never attempts to bypass
the browser's installation confirmation.

Use these lifecycle commands:

```sh
npx -y @nanlogic/saccade doctor
npx -y @nanlogic/saccade update
npx -y @nanlogic/saccade uninstall
```

Updates and ordinary uninstall preserve your Saccade Profile. Run
`npx -y @nanlogic/saccade uninstall --purge` to remove the Profile and Runtime
data.

Version 0.1.2 supports Apple Silicon and Intel macOS plus Windows x64 clients
that can start a STDIO MCP and control the same Chrome or Edge tab. The same
store Extension is used on every platform; setup selects the matching signed
Runtime.
Cloud-only sessions cannot connect to the local Extension and Native Host.
Setup does not request Accessibility or install the optional Reference
Actuator.
