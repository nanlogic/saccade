# Saccade Windows x64 test candidate

This bundle is for Wayne's explicit Windows-machine test. The Runtime is
unsigned and is not a public release.

1. Extract the entire Actions artifact. Do not run it from inside the ZIP.
2. Open `chrome://extensions` or `edge://extensions`, enable Developer mode,
   choose **Load unpacked**, and select this bundle's `extension` directory.
3. Copy the 32-letter Extension ID shown by the browser.
4. In PowerShell, from this directory, run:

   ```powershell
   Set-ExecutionPolicy -Scope Process Bypass
   .\install.ps1 -ExtensionId YOUR_32_LETTER_EXTENSION_ID
   ```

5. Restart Codex or Claude so it reloads the MCP configuration.
6. Ask the agent to call `saccade.system.capabilities`, open the test URL with
   `saccade.tabs.open`, perform one supported action, and observe its delta.

Sharing is optional and only exposes a specific pre-existing user tab. The
normal route opens an Agent-owned tab automatically and requires no manual tab
authorization.

Windows SmartScreen may warn because this test Runtime is not yet signed. Check
the artifact came from the `nanlogic/saccade` Actions run before allowing it.
Do not redistribute this candidate.

To diagnose or remove it:

```powershell
node .\package\bin\saccade-setup.js doctor
node .\package\bin\saccade-setup.js uninstall --purge
```
