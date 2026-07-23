# macOS Build 86 Release Report

Date: 2026-07-23
Status: notarized local release candidate; not yet published

## Candidate

- source commit: `9338ab06703dc57ccffffd5d588acdcaf34c7f16`
- source tree at build: clean (`source_dirty=false`)
- app version/build: `0.1.0 (86)`
- bundle identifier: `ai.saccade.browser`
- signing identity: `Developer ID Application: NaN Logic LLC (W5D59P54A2)`
- team identifier: `W5D59P54A2`
- Hardened Runtime: enabled
- Apple secure timestamp: present

## Apple notarization

| Artifact | Submission ID | Result |
| --- | --- | --- |
| App archive | `44e1a5e9-526f-422e-83d4-01a7e841eb77` | Accepted |
| DMG | `49f5d93e-fc65-40e0-b4a7-80387a0ff614` | Accepted |

Both artifacts passed `xcrun stapler validate`. Gatekeeper accepted the App
and DMG with source `Notarized Developer ID`.

Final local artifact:

- path: `dist/notarization-build86/Saccade.dmg`
- SHA-256: `303149e1113785dbea608cc47795325b38ec2cabf630ba262e49730a07953f66`

## Candidate gates completed

- `cargo test -p saccade-mcp`: 38/38 passed;
- reflex evidence packager integration tests: 5/5 passed;
- strict nested code-signature verification: passed;
- notarization preflight: passed;
- App and DMG notarization, stapling, and local Gatekeeper assessment: passed;
- packaged DOCMAX/PDF smoke `release_pdf_smoke_build86`: passed.

## Remaining before a supported public download

- install the DMG on a second clean Mac without the development repository;
- confirm a new MCP host session connects to the installed embedded MCP;
- run Agent Off/On, article, nested iframe, protected-value, form, and reflex
  gates against that installed App;
- confirm profile preservation and uninstall behavior;
- publish the DMG and matching checksum only after those checks pass.

This report records a notarized candidate. It does not claim that the remaining
clean-machine and installed-product release gates have passed.

## Same-Mac uninstall and reinstall validation

The prior `/Applications/Saccade.app` Build 85 was moved to a recoverable
temporary backup and the notarized Build 86 App was installed from the final
DMG. This preserved the existing browser profile while replacing the installed
application bundle.

The installed `/Applications/Saccade.app` then passed:

- build number 86 and strict nested signature verification;
- stapled-ticket validation and Gatekeeper source `Notarized Developer ID`;
- exact embedded-MCP match with the release candidate, SHA-256
  `c68f9865366694066da687c533a1690b9b3116254d1d1437704e9eedc19890e1`;
- launch from `/Applications`, including GPU, Renderer, and utility helpers;
- installed-MCP cleanroom probe from a repo-free working directory and
  temporary HOME: Agent tab, same-WebView collector, article read, dynamic form
  inventory, installed-product tool surface, and `values_logged=false`;
- single cross-origin iframe form gate: 2/2 fields filled with two verified
  same-WebView native-input receipts;
- three-layer nested iframe form gate: 3/3 fields filled across depths 1, 2,
  and 3 with three verified same-WebView native-input receipts.

Both iframe gates used `form_plan_v2`, blocked the direct-type bypass, and did
not submit a form. This is strong installed-path evidence, but it remains a
same-machine reinstall rather than an independent clean-Mac result.
