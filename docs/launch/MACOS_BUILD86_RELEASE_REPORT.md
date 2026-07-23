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
