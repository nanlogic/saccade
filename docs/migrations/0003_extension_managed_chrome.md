# Migration 0003: Extension and managed Chrome route

- Source baseline: `8c4defb3f8b0ed9b0cb4cb6ff522f9a550ddb76b` in the historical
  `/Users/waynema/Documents/GitHub/SACCADE` worktree.
- Reviewed source paths: the uncommitted, contract-aligned
  `extension/manifest.json`, `extension/src/{protocol,consent,collector,service_worker}.js`,
  control-related portions of `extension/src/truth.js`, and their focused
  tests. These files are not present in the source commit tree. That mismatch
  is recorded here instead of attributing uncommitted source to the commit.
- Destinations: `extension/manifest.json`, `extension/src/{protocol,consent,
  collector,service_worker}.js`, the four files under
  `extension/src/controls`, and the Extension Node tests.
- Retained: fixed Extension identity, strict v1 Native Messaging envelopes,
  agent-owned tab ACL, HTTP/HTTPS-only tab opening, observation identity and
  revision binding, opaque action tokens, fresh preparation, topmost and focus
  checks, safe state projection, and option object identity.
- Rewritten: the collector recognizes only button, text field, checkbox,
  select, and select option. It projects each supported control through the
  Registry. No historical `truth.js` classifier was copied.
- Intentionally excluded: downloads, protected fill, local loops, PDF,
  arbitrary selectors or coordinates, secondary browser routes, and every
  control family outside the first slice.
- Development route: `scripts/dev.sh` manages a dedicated Chrome for Testing
  profile, `com.nanlogic.saccade.dev`, a fixed installed Runtime path, a local
  fixture server, Codex MCP backup and restore, exact process IDs, and
  persistent local evidence. Chrome for Testing 151 reads its Native Messaging
  manifest from `/Library/Google/ChromeForTesting/NativeMessagingHosts`, so
  `up` performs one idempotent, administrator-confirmed installation there.
- Automated route: `scripts/dev_probe.py` calls
  `tabs.open -> web.observe -> web.act` through MCP JSON-RPC. It does not use
  Playwright, CDP, or a browser automation fallback. Failure diagnostics are
  saved without textfield contents.
- Static checks: Extension Node tests, Rust workspace tests and Clippy,
  Catalog generation, and the single-architecture gate.
- Native development evidence: the macOS Chrome for Testing run at
  `20260728T200308Z` produced four receipts with `accepted_by_os` dispatch and
  `verified` postconditions for click, type, click, and select. The same run
  rejected an old token, exposed Profile behavior through MCP, removed the
  Profile-banned Save control from observation, restored the default Profile,
  and passed the textfield-content leak scan. Evidence is stored outside the
  repository under `~/Library/Application Support/Saccade Dev/evidence/`.
  This does not satisfy Chrome and Edge release evidence for the same release
  candidate.
- Public status: all four Catalog rows stay `implementation`; Chrome and Edge
  remain `pending`.
