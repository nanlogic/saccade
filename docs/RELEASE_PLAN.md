# Saccade 0.1.2 Windows x64 release plan

Status: candidate implementation; public release is blocked on final store and
release gates. Windows is source-install only.

## Ownership and release surface

Saccade is a Nanlogic product. `nanlogic/saccade` is the sole active source
repository, GitHub Actions publisher, and Runtime Release owner. The npm name
is `@nanlogic/saccade`; its organization, trusted publisher, recovery methods,
and at least two administrators must be controlled by Nanlogic. Wayne operates
the Chrome Web Store submission through a
Nanlogic-controlled publisher identity.

The public product contains one browser Extension, the explicit
`npx -y @nanlogic/saccade` macOS command, and a repository-level Windows source
install Skill. Version 0.1.2 keeps Extension candidate 0.3.24 and publishes
signed headless Runtimes for `darwin-arm64` and `darwin-x64`. Windows x64 builds
the locked source locally, uses current-user files and Native Messaging
registry entries, and publishes no unsigned executable. It adds no MSI, GUI
installer, Accessibility request, Reference Actuator, Playwright/CDP route,
screenshot, or arbitrary-coordinate fallback.

## Automated publication

1. Run the Windows workflow and its isolated build, install, MCP tool-list,
   registry, and uninstall smoke. Its unsigned artifact remains a seven-day
   test artifact and is never attached to a public Release.
2. On a real Windows x64 machine, run the repository source-install Skill,
   load the deterministic unpacked Extension, and verify capabilities, Truth,
   action, delta, restart recovery, doctor, repair, and uninstall.
3. Freeze an existing `v0.1.2` tag after the complete local and browser gates.
4. Manually dispatch `Prepare signed Runtime release` with that tag and the
   final store Extension ID. The workflow reruns repository gates, requires a
   production-named exact Extension candidate, builds on GitHub's arm64 and
   Intel macOS runners, verifies both signatures, and creates one draft GitHub
   Release without overwriting an existing release.
5. The workflow assembles `release.json` only when both macOS architecture
   drafts share the exact version, MCP contract, Extension candidate, signing
   status, and Nanlogic Release URL. It attaches the manifest, checksums,
   signed macOS Runtime binaries, and Extension ZIP to the draft.
6. Wayne reviews and publishes the GitHub Release. That publication event is
   the only trigger for `Publish setup package`.
7. The npm workflow downloads the attached manifest, verifies the tag,
   company ownership, candidate, store origin, signed macOS artifacts and
   checksums, then publishes with GitHub OIDC trusted publishing and npm
   provenance. It has no long-lived npm token.
8. The Chrome Web Store submission remains Extension 0.3.24. After approval, a
   clean macOS user runs the npm setup smoke and a clean Windows user runs the
   source-install Skill smoke through uninstall and Profile preservation.

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
- Windows x64 passes the same lifecycle from a clean source checkout on a real
  machine, including Chrome and Edge current-user registry ownership. No
  unsigned Windows executable is published.
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
- The Windows source-install Actions gate and real-machine route still need to
  pass before 0.1.2 can be public. Windows executables are built locally and
  are not attached to the public release.
- GitHub Release `v0.1.0` is a prerelease because its npm package name used a
  scope Nanlogic does not own. Its published tag and artifacts are not reused.
- The owner-approved repository archival is complete and recorded in the
  repository archive report.
