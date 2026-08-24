# Saccade 0.1.2 Windows x64 release plan

Status: candidate implementation; public release is blocked on Windows proof
and code signing.

## Ownership and release surface

Saccade is a Nanlogic product. `nanlogic/saccade` is the sole active source
repository, GitHub Actions publisher, and Runtime Release owner. The npm name
is `@nanlogic/saccade`; its organization, trusted publisher, recovery methods,
and at least two administrators must be controlled by Nanlogic. Wayne operates
the Chrome Web Store submission through a
Nanlogic-controlled publisher identity.

The public product contains one browser-store Extension and the explicit
`npx -y @nanlogic/saccade` command. The Extension package is shared across CPU
architectures. Version 0.1.2 keeps Extension candidate 0.3.24 and targets signed
headless Runtimes for `darwin-arm64`, `darwin-x64`, and `win32-x64`. Windows
uses current-user files and Native Messaging registry entries; it adds no MSI,
GUI installer, administrator requirement, Accessibility request, Reference
Actuator, Playwright/CDP route, screenshot, or arbitrary-coordinate fallback.

## Automated publication

1. Run the Windows candidate workflow and its isolated install, MCP tool-list,
   registry, and uninstall smoke.
2. Wayne downloads the unsigned seven-day artifact on a real Windows x64
   machine, loads the unpacked Extension, and verifies capabilities, Truth,
   action, delta, restart recovery, doctor, and uninstall.
3. After that proof, submit the GitHub-hosted Windows build to the free
   SignPath Foundation open-source program. The protected release workflow
   must use SignPath origin verification and manual signing approval. The
   assembler and verifier must require all three Runtime platforms before
   publication.
4. Freeze an existing `v0.1.2` tag after the complete local and browser gates.
5. Manually dispatch `Prepare signed Runtime release` with that tag and the
   final store Extension ID. The workflow reruns repository gates, requires a
   production-named exact Extension candidate, builds on GitHub's arm64 and
   Intel macOS and Windows runners, verifies platform signatures, and creates
   one draft GitHub Release without overwriting an existing release.
6. The workflow assembles `release.json` only when all architecture drafts
   share the exact version, MCP contract, Extension candidate, signing status,
   and Nanlogic Release URL. It attaches the manifest, checksums, Runtime
   binaries, and Extension ZIP to the draft.
7. Wayne reviews and publishes the GitHub Release. That publication event is
   the only trigger for `Publish setup package`.
8. The npm workflow downloads the attached manifest, verifies the tag,
   company ownership, candidate, store origin, all signed artifacts and
   checksums, then publishes with GitHub OIDC trusted publishing and npm
   provenance. It has no long-lived npm token.
9. The Chrome Web Store submission remains Extension 0.3.24. After approval, a
   clean user runs setup, doctor, open, Truth, action, browser restart,
   uninstall, and Profile-preservation smoke.

The tracked `packages/setup/release.json` remains an unpublished template.
Final URLs and checksums are injected into the package only inside the npm
publication job, after the GitHub Release is public.

## Product and evidence gates

- The same candidate and frozen commit pass Chrome and Edge full→delta,
  common controls, dynamic replacement, continuous movement, Profile,
  protected fields, frames/shadow boundaries, lifecycle recovery, and public
  site reading.
- Intel macOS additionally passes install, checksum, Native Messaging, MCP
  start, doctor, browser restart, update, rollback, uninstall, and purge.
- Windows x64 passes the same lifecycle on a real machine, including Chrome and
  Edge current-user registry ownership and Authenticode verification.
- Codex and Claude each act with their own tool in the same authorized tab
  while Saccade reports the semantic transition.
- Fair Playwright comparisons retain identical URL/task/model/order controls
  and separate control-plane, discovery, steady-state, infrastructure, and
  model-usage accounting. Reference Actuator reports are not a release gate
  and cannot support a public `saccade.act` superiority claim.
- A failure after publication pauses the store rollout, marks the GitHub
  Release prerelease when appropriate, and ships a new npm patch version.
  Published npm version numbers are never reused.

## Current blockers

- Production candidate `0.3.24` has a store-safe `Saccade` manifest and is in
  Chrome Web Store review. Local development installs derive a separately
  identified development candidate and continue to use
  `com.nanlogic.saccade.dev`.
- Company recovery channels and a second npm administrator remain required.
- The Windows Actions candidate, real-machine route, and Authenticode signing
  still need to pass before 0.1.2 can be public.
- GitHub Release `v0.1.0` is a prerelease because its npm package name used a
  scope Nanlogic does not own. Its published tag and artifacts are not reused.
- The owner-approved repository archival is complete and recorded in the
  repository archive report.
