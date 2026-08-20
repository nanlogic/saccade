# Saccade 0.1.0 Developer Preview release plan

Status: release preparation only. No public artifact has been published.

## Ownership and release surface

Saccade is a Nanlogic product. `nanlogic/saccade` is the sole active source
repository, GitHub Actions publisher, and Runtime Release owner. The npm package
is `@nanlogic/saccade`; its organization, trusted publisher, recovery methods,
and at least two administrators must be controlled by Nanlogic. Wayne operates
the Chrome Web Store submission through a
Nanlogic-controlled publisher identity.

The public product contains one browser-store Extension and the explicit
`npx -y @nanlogic/saccade` command. The Extension package is shared across CPU
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
   checksums, then publishes with npm provenance. Because npm cannot configure
   trusted publishing or staged publishing for a brand-new package, `0.1.0`
   uses a one-day organization-scoped `NPM_BOOTSTRAP_TOKEN`. Immediately after
   that first publish, bind `@nanlogic/saccade` to repository
   `nanlogic/saccade`, workflow `publish-npm.yml`, environment `npm-release`,
   delete the GitHub secret, and revoke the token. Every later version uses
   GitHub OIDC trusted publishing with no npm token.
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

- Production candidate `0.3.24` has a store-safe `Saccade` manifest. Local
  development installs derive a separately identified development candidate
  so they continue to use `com.nanlogic.saccade.dev`. The production candidate
  still requires exact Chrome and Edge browser evidence before store upload.
- Nanlogic's Apple signing/notarization credentials, final store Extension ID,
  one-day npm bootstrap token, company recovery channels, and second npm
  administrator must exist before the workflows can publish. The trusted
  publisher is bound and the bootstrap token is deleted immediately after the
  first package publish.
- The x64 Runtime and setup lifecycle still need real Intel macOS evidence.
- The owner-approved repository archival is complete and recorded in the
  repository archive report.
