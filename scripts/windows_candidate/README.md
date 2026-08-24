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

5. Pin Saccade, open the tab to test, click **Share this tab**, and restart
   Codex or Claude so it reloads the MCP configuration.
6. Ask the agent to call `saccade.system.capabilities`, inspect the shared tab,
   perform one supported action, and observe its delta.

Windows SmartScreen may warn because this test Runtime is not yet signed. Check
the artifact came from the `nanlogic/saccade` Actions run before allowing it.
Do not redistribute this candidate.

To diagnose or remove it:

```powershell
node .\package\bin\saccade-setup.js doctor
node .\package\bin\saccade-setup.js uninstall --purge
```
