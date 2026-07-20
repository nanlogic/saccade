# AI-045A Build 66: Saccade New Tab Identity

Date: 2026-07-19

## Scope

Replace Chromium's new-tab favicon with the existing Saccade double-loop icon
without modifying the pinned CEF/Chromium engine or disabling the sandbox.

## Implementation

- Added a minimal Manifest V3 `chrome_url_overrides.newtab` extension.
- Bundled the extension beneath `extensions/saccade-new-tab` in the Windows
  package and per-user installation.
- Added a CEF `OnBeforeCommandLineProcessing` hook that resolves the extension
  relative to `Saccade.exe` and appends `--load-extension` for the browser
  process.
- Reused the seven-size `Saccade.ico` so the favicon remains crisp at tab size.
- Bumped Windows dogfood metadata from Build 65 to Build 66.

## Verification

- Release build completed with `USE_SANDBOX=ON`.
- `chrome://version` showed the automatically injected
  `--load-extension=...\\extensions\\saccade-new-tab` switch.
- A post-startup new tab in a clean profile displayed the Saccade loop favicon.
- A new tab in the installed daily profile displayed the same Saccade favicon.
- Installed extension manifest and icon are present.
- No installed Saccade process contained `--no-sandbox`.
- Installed `Saccade.dll` SHA-256:
  `BFE81E43BF9FDA2FB19DA5467EAFA51051231AAD899235DA8C1FEC41DE1D965D`.

## Claim boundary

This changes the new-tab page identity only. Native CEF Chrome Runtime product
strings may still say Chromium, and Build 66 remains unsigned and unsuitable
for public distribution.
