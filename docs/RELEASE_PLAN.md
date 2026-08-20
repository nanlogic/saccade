# Saccade 0.1.0 Developer Preview release plan

Status: release preparation only. No public artifact has been published.

## Ownership and release surface

Saccade is a Nanlogic product. `nanlogic/saccade` is the sole active source
repository, GitHub Actions publisher, and Runtime Release owner. The npm name
remains `@saccade/setup`, but the npm organization, trusted publisher,
recovery methods, and at least two administrators must be controlled by
Nanlogic. Wayne operates the Chrome Web Store submission through a
Nanlogic-controlled publisher identity.

The public product contains one browser-store Extension and the explicit
`npx -y @saccade/setup` command. The Extension package is shared across CPU
architectures. Setup selects one signed and notarized headless Runtime for
`darwin-arm64` or `darwin-x64`. Windows follows only after its setup and
lifecycle evidence exists. There is no GUI installer, Accessibility request,
Reference Actuator, Playwright/CDP route, selector, screenshot, or arbitrary
coordinate fallback in this release.

## Automated publication

1. Freeze an existing `v0.1.0` tag after the complete local and browser gates.
2. Manually dispatch `Prepare signed Runtime release` with that tag and the
   final store Extension ID. The workflow reruns repository gates, requires a
   production-named exact Extension candidate, builds on GitHub's arm64 and
   Intel macOS runners, signs and notarizes both Runtime artifacts, and creates
   one draft GitHub Release without overwriting an existing release.
3. The workflow assembles `release.json` only when both architecture drafts
   share the exact version, MCP contract, Extension candidate, signing status,
   and Nanlogic Release URL. It attaches the manifest, checksums, Runtime
   binaries, and Extension ZIP to the draft.
4. Wayne reviews and publishes the GitHub Release. That publication event is
   the only trigger for `Publish setup package`.
5. The npm workflow downloads the attached manifest, verifies the tag,
   company ownership, candidate, store origin, both signed artifacts and
   checksums, then publishes with GitHub OIDC trusted publishing and npm
   provenance. It has no long-lived npm token.
6. Wayne submits the exact attached Extension ZIP to the Nanlogic Chrome Web
   Store publisher. After approval, a clean user runs setup, doctor, open,
   Truth, action, browser restart, uninstall, and Profile-preservation smoke.

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

- Candidate `0.3.23`
  (`2d8a877e3dc1b5c9a003aa3662ea9ddad506a7033aba286e1c48e21fe8af2612`)
  is the verified development candidate, but its manifest name is still
  `Saccade Extension (Development)`. The release workflow intentionally
  refuses to package it for the store. A production name changes the candidate
  content and therefore requires a new version/identity and browser evidence.
- Nanlogic's Apple signing/notarization credentials, final store Extension ID,
  npm trusted-publisher binding, company recovery channels, and second npm
  administrator must exist before the workflows can publish.
- The x64 Runtime and setup lifecycle still need real Intel macOS evidence.
- GitHub repository archival is a separate owner-approved mutation; its
  read-only audit is recorded in the repository archive report.
