# Saccade public-release licensing

Date: 2026-07-18

Saccade is published by **NaN Logic LLC** at **https://nanlogic.com/**. Saccade
source code and the core browser/Agent runtime are licensed under the Apache
License 2.0. The goal is broad adoption, independent verification, integration
and reproducible comparison with Playwright.

The `Saccade` name, logo and designation of an official signed Saccade release
are not licensed as product identity. Apache License 2.0 section 6 already
excludes trademark rights; `TRADEMARKS.md` explains permitted nominative use and
the requirement that modified distributions use a distinct identity unless
written permission is granted.

Official binary packages include:

- `SACCADE_LICENSE.txt`, the Apache License 2.0 text for Saccade;
- `SACCADE_NOTICE.txt`, the Saccade attribution notice;
- `SACCADE_TRADEMARKS.md`, the product-identity policy;
- `CEF_LICENSE.txt`, the pinned CEF BSD-3-Clause license;
- `CHROMIUM_CREDITS.html`, Chromium third-party notices;
- `INVENTORY.json`, machine-readable license and identity metadata; and
- `SBOM.cdx.json`, a deterministic CycloneDX 1.6 inventory of the target
  Rust dependency graph plus the pinned CEF and Chromium engine versions; and
- `VERSION.json`, application, build, CEF, Chromium, source-commit and signing
  identity metadata.

The macOS About panel identifies NaN Logic LLC as the copyright owner. The Help
menu opens `https://nanlogic.com/` in a new Human-controlled Agent Off Saccade
tab; opening Help does not silently grant an Agent access to that tab.

Open-source licensing does not make every binary official. The official macOS
release remains identifiable by bundle ID `ai.saccade.browser` and the Team ID
recorded in its signed package manifest. Notarization and Gatekeeper acceptance
are separate release gates and are not implied by this license decision.
