# Saccade CEF Migration and Cleanup Plan

Date: 2026-07-13
Status: canonical engine migration plan
Target: bounded macOS dogfood build in 5 focused days, with 2 contingency days

## Product Definition

Saccade is a secure agent browser runtime. It ships with CEF/Chromium as its
default human-facing engine and exposes browser-integrated, redacted facts,
revision-bound actions, verification, and value-free replay through a
versioned engine-neutral contract.

CEF is the first product engine, not the product boundary. Servo remains a
supported research adapter. A vendor may use Saccade's CEF runtime, attach the
Saccade adapter to its own Chromium host, or implement the same adapter
contract for another engine.

We will consume an official CEF binary distribution directly. We will not
build Chromium from source or make a community Rust CEF wrapper a critical
dependency. The CEF host is a thin C++/Objective-C++ layer around the official
C/C++ API; Saccade policy and protocol remain Rust.

Official basis:

- https://chromiumembedded.github.io/cef/general_usage.html
- https://github.com/chromiumembedded/cef-project
- https://cef-builds.spotifycdn.com/index.html

CEF provides the required multi-process browser/renderer architecture, IPC,
windowed browser hosting, GPU acceleration, profiles, and browser input APIs.
Its macOS application requires the official framework and helper-app bundle
layout, so packaging is part of the first gate rather than deferred cleanup.

## Architecture

```text
Host agent / MCP client
          |
          v
Saccade contract, policy, grants, verification, replay       Rust
          |
          v
Versioned Engine Adapter Protocol                            Rust schema
          |
          v
Saccade CEF browser process <---- CEF IPC ----> renderer     C++ / ObjC++
          |
          v
Visible Chromium page + trusted Saccade browser chrome
```

The engine adapter protocol owns capabilities, not product policy. The adapter
may report facts and accept validated primitive actions; it cannot grant itself
authority, unredact data, confirm a side effect, or change replay policy.

Initial layout:

```text
crates/saccade_engine_api/       engine-neutral request/response types
engines/cef/                     direct official CEF host and helper apps
engines/cef/cef.lock.json        exact CEF/Chromium version, URL, SHA-256
bins/saccade-mcp/                unchanged public host contract
crates/saccade_core/             policy, ownership, redaction classifications
```

Large CEF archives and build outputs live outside git under the user cache.
Only the version lock, checksums, build scripts, source, licenses, and manifests
belong in the repository.

## Security Contract

These rules block the migration if violated:

1. **Redact before export.** Password, OTP, identity, payment, signature, and
   user-owned values are never serialized out of the renderer/engine boundary.
   Completion state may leave; the value may not.
2. **Page content is untrusted.** Labels, article text, DOM facts, and renderer
   observations carry provenance and cannot grant authority or approve an
   action. The first CEF collector is explicitly labeled
   `cef_renderer_observed`, not `engine_native`.
3. **Trusted grants live outside page DOM.** A grant binds tab, main-frame
   origin, page revision, scopes, expiry, and profile. Trusted browser chrome
   creates it; page JavaScript cannot see or manufacture it.
4. **Local transport is owner-only.** Prefer a Unix domain socket with `0600`
   permissions. Use an ephemeral 256-bit capability stored in an owner-only
   grant file. Never put it in a URL, page, screenshot, log, or replay.
5. **Actions are map- and revision-bound.** The host accepts an action ID from
   the current map, checks policy and ownership, dispatches browser input, and
   verifies a postcondition. Stale basis means re-read and re-plan, not retry.
6. **Protected work remains human-owned.** Login, credentials, OTP, payment,
   signing, identity, account-security changes, and consequential final
   actions require a trusted human step.
7. **Screenshots are optional evidence.** They are off by default for logged-in
   or user-filled pages, never the primary truth channel, and require an
   explicit guarded capture path.
8. **Production has no DevTools backdoor.** Remote debugging is disabled in
   release builds. CDP remains a reference/test adapter only.
9. **CEF sandbox and process separation stay enabled.** A test-only bypass may
   not enter a release manifest.
10. **Supply chain is pinned.** Record CEF and Chromium revisions, SHA-256,
    licenses/notices, build tool versions, SBOM, and reproducible commands.

The renderer collector must capture needed DOM intrinsics at context creation
and serialize from a fixed allowlist. A hostile page may still attempt semantic
deception, so fast facts are candidates, not authority. A separate structural
preflight and optional visual agreement gate remain available for consequential
or inconsistent surfaces. We do not claim public CEF APIs are an unspoofable
Blink-internal truth source.

## Five-Day Build

### Day 1: Direct CEF boot

Status: complete on 2026-07-13. Evidence: `docs/cef_day1_report.md`.

- Pin one official macOS CEF binary distribution and checksum.
- Start from the official standard sample target and complete macOS bundle;
  defer minimal/custom host packaging until its lifecycle matches upstream.
- Build `Saccade.app` plus the required renderer/GPU helper app bundles.
- Open local fixtures, GitHub public, the local game, and one WebGL page in a
  visible window with GPU acceleration and clean resize.
- Add a stable persistent profile and an incognito profile outside the repo.

Gate: visible release build launches twice, navigates, resizes, closes cleanly,
and preserves only the normal profile.

### Day 2: Freeze the engine adapter

- Add `saccade_engine_api` with versioned capabilities, tab identity, origin,
  page revision, fact batches, action maps, input receipts, and typed errors.
- Make engine selection capability-based. Remove Servo-specific assumptions
  from new host paths without rewriting old Servo code.
- Connect CEF browser process to `saccade-mcp` through the owner-only local
  transport.

Gate: the existing integration host examples attach, inspect capabilities,
navigate one granted tab, pause, and close without knowing the engine name.

### Day 3: Truth and reflex

- Install the renderer-side allowlisted fact collector before page scripts.
- Emit redacted control, geometry, semantic-object, and revision facts through
  CEF renderer/browser IPC.
- Dispatch pointer/keyboard primitives through CEF browser input APIs.
- Port the Chrome truth/reflex fixture unchanged.

Gate: three independent 100-target release runs each achieve 100/100 hits,
zero value leaks, and fact-to-page-receipt latency at or below 20 ms p95,
without CDP.

### Day 4: Safety, forms, and replay

- Reuse the current grant, policy, ownership, stale-basis, verification, and
  replay semantics.
- Run ordinary input, textarea, select, radio, checkbox, contenteditable,
  long-table, and sensitive-field handoff fixtures.
- Add hostile-page cases: forged labels, attempted binding calls, monkeypatched
  DOM methods, hidden controls, cross-frame facts, navigation races, stale
  action maps, and capability replay.

Gate: ordinary draft fill verifies; protected fields expose completion only;
cross-tab/stale/forged requests fail closed; reports contain no sentinels.

### Day 5: Human browser and dogfood release

- Add a small native toolbar: back, forward, reload/stop, address, profile,
  trusted Agent grant/status, tabs, and clear error/recovery state.
- Run the local game reflex, original MouseAccuracy, FORMMAX, public reading,
  GitHub/Gist read and harmless draft, profile restart, and tab-close recovery.
- Package one local macOS release with license inventory and rerun commands.

Gate: one person can browse, log in once, grant the visible tab, review agent
work, restart, and retain normal-session login without seeing debug tooling.

### Contingency Days 6-7

Use only for CEF bundle/signing mechanics, renderer IPC long-tail latency, or a
security gate. Do not spend them polishing unrelated features.

## Week-One Definition of Done

Week one produces a **macOS dogfood release**, not a public general-browser
release. It is done only when:

- CEF is the default visible engine and no Chrome installation is required;
- the MCP/integration contract remains version `1.x` and engine-neutral;
- the 3x100 no-CDP reflex gate passes;
- sensitive sentinel scans and adversarial bridge tests pass;
- normal and incognito profile behavior is measured;
- FORMMAX ordinary draft plus human sensitive handoff passes;
- local game and WebGL render and accept actions;
- GitHub is tested and honestly routed if a specific workflow remains broken;
- every action has a receipt, block, or typed failure;
- release artifacts identify exact CEF/Chromium versions and licenses.

CEF is Chromium, not branded Google Chrome. Proprietary codecs, DRM/Widevine,
provider anti-bot decisions, and Chrome-only services are separate measured
capabilities. We will not market "every Chrome site works".

## Cleanup Plan

Cleanup is evidence-driven and runs after processes are stopped. No cleanup
command deletes files on its first invocation.

### C0: Inventory and manifest

- Add a dry-run inventory that classifies git-tracked source, ignored build
  output, canonical evidence, duplicate evidence, profiles, caches, and release
  kits.
- Parse documentation references to `runs/` and protect every referenced
  artifact plus one canonical report per closed gate.
- Write size, reason, last use, SHA-256, and proposed disposition to a manifest.

Gate: dry run explains every proposed deletion and reports reclaimable bytes.

### C1: Safe generated storage

Current measured storage is approximately:

| Path | Size | Initial rule |
| --- | ---: | --- |
| `runs/webgl_runtime/` | 325 MB | Keep referenced reports/final evidence; prune duplicate PNG frames. |
| `runs/browser_session_worker/` | 221 MB | Keep canonical reports/replays; prune unreferenced screenshots and debug streams. |
| `runs/dogfood_profile/` | 193 MB | Do not delete auth state; remove only proven cache subtrees while closed. |
| `runs/chrome_compat_profile/` | 174 MB | Keep until CEF login persistence passes; then ask before deletion. |
| `target/` | 21 MB | Regenerable; safe after active builds stop. |
| `dist/` | 11 MB | Keep current kit and latest evidence kit; remove superseded ignored kits. |

The two evidence directories are mostly generated PNGs and can reclaim roughly
half a gigabyte without deleting canonical JSON reports. Profile cleanup must
preserve cookies/storage until the user confirms the CEF profile works.

### C2: Move runtime state out of the repo

- New CEF profiles: `~/Library/Application Support/Saccade/Profiles/`.
- CEF downloads/build cache: `~/Library/Caches/Saccade/cef/<revision>/`.
- Temporary run frames: system temporary directory with bounded retention.
- Repository `runs/`: concise reports, replay summaries, and selected evidence
  only; never live browser profiles.

Do not copy Servo or Chrome cookies into CEF silently. The user logs into the
new normal profile once. After persistence passes, old profiles become
user-approved deletion candidates rather than archived evidence.

### C3: Retire duplicate product paths after parity

Only after the CEF week-one gate passes:

- make `saccade-cef` the default release route;
- keep Servo behind an explicit `--engine servo` adapter;
- mark `bins/saccade-shell` and old Servo 0.2 product wrappers legacy;
- keep Chrome/CDP scripts under reference/test ownership, never product;
- stop packaging duplicate old dogfood launchers;
- remove a legacy path only after its cited gate has a CEF replacement and the
  documentation/link check is green.

No mass rewrite of `crates/saccade_browser` occurs during the CEF week. That
would mix migration risk with deletion risk. First pass the new adapter; then
shrink the old implementation in isolated commits.

### C4: Documentation consolidation

- Canonical active files: `CURRENT_ACTION_ITEMS.md`, this plan,
  `integration_contract_v1.md`, the security plan, and the compatibility
  ledger.
- Mark stale plans historical rather than deleting their evidence.
- Add an archive index and link checker before moving old milestone reports.
- Keep public claim documents short and derive claims only from canonical
  artifacts.

## Adoption Package

The CEF refactor should leave a large vendor with three choices:

1. run the signed Saccade CEF browser locally;
2. embed the Saccade engine adapter in its Chromium/CEF product;
3. implement the versioned adapter for another engine.

The vendor package needs a small SDK, protocol schemas, threat model, security
test corpus, SBOM/license inventory, benchmark artifacts, and an explicit list
of residual risks. The external claim is:

> Saccade ships on CEF/Chromium and keeps its agent contract independent of the
> rendering engine.

It is not "a Servo browser with an AI script" and not "Chrome automation with
extra JSON." The durable product is bounded browser authority plus redacted
facts, verified actions, and replay.
