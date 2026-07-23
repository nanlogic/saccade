# Work Ledger

## 2026-07-18 - Build 62 complete local macOS dogfood closeout

- Rebuilt from the pristine pinned CEF 150.0.11 archive and fixed two hidden
  reproducibility defects: patch 0021 no longer depends on pre-mutated cache
  context, and the dogfood packager now honors `SACCADE_CEF_BUILD_DIR`.
- Added Saccade-owned Chromium product strings and a transparent 64-by-64
  default/New Tab favicon through `CefResourceBundleHandler`; NaN Logic Help
  remains a Human-owned Agent Off tab.
- Fixed Agent child-tab routing. Only a revision-bound Agent action already
  registered as opening a new context transfers Agent ownership to the matching
  child; ordinary human clicks and Help continue to create Off tabs.
- Added Developer ID Hardened Runtime signing, secure timestamps, scoped JIT
  entitlements, a no-upload notarization preflight, and an explicit release-owner
  App+DMG submit/staple/Gatekeeper workflow.
- Added deterministic CycloneDX 1.6 generation. Build 62 ships 719 unique target
  components plus the pinned CEF/Chromium inventory and portable checksums.
- Diagnosed IGN playback as the pinned official CEF binary's H.264/AAC/HLS codec
  boundary. VP9/Opus remains green on ordinary YouTube; no site shim or
  proprietary-codec distribution was added.
- Build 62 passed release/package, notarization preflight, tab lifecycle,
  company Help, protected form safety, tab defaults, multi-tab registry,
  permission/activity separation, AI-033 and AI-034 gates.
- Evidence: `runs/dogfood/df_build62_release_complete_20260718/report.json`,
  `runs/dogfood/df_build62_tab_profile_20260718/report.json`,
  `runs/dogfood/df_build62_company_help_20260718/report.json`,
  `runs/dogfood/df_build62_form_safety_20260718/report.json`,
  `runs/dogfood/df_build62_tab_defaults_20260718/report.json`,
  `runs/dogfood/df_build62_multi_tab_registry_20260718/report.json`,
  `runs/dogfood/df_build62_permission_activity_20260718/report.json`,
  `runs/dogfood/df_build62_agent_safety_20260718/report.json`, and
  `runs/dogfood/df_build62_human_agent_agreement_20260718/report.json`.
- Executed the reviewed release-kit cleanup manifest after the gates passed:
  29 superseded packages were removed, reclaiming about 10.0 GiB. Build 62,
  all evidence and all auth/profile state were preserved. Manifest:
  `docs/ai040_cleanup_manifest.json`.

## 2026-07-18 - Build 60 Apache licensing and NaN Logic Help identity

- Chose Apache-2.0 for Saccade source and the core browser/Agent runtime to
  maximize adoption and independently reproducible competition with Playwright.
  The Saccade name, logo and official signed-release identity remain reserved
  under Apache section 6 and `TRADEMARKS.md`.
- Added LICENSE, NOTICE, trademark policy and public-release licensing docs.
  Both the App and package now ship identical Saccade files beside the CEF BSD
  license and Chromium credits; manifests record NaN Logic LLC,
  `https://nanlogic.com/`, bundle ID and Team ID.
- Added a native `Help > Saccade Help — nanlogic.com` action that opens the
  company site in a new Saccade tab. The first Build 59 runtime gate discovered
  that a legacy initial-grant flag incorrectly turned later Human tabs On.
- Fixed the legacy grant so it applies only before the bridge starts. Signed
  Build 60 then passed: Help increased browser count 1→2 while Agent-eligible
  tabs remained 1→1; the company URL resolved to `https://www.nanlogic.com/`
  with HTTP 200.
- The release license gate passed Apache metadata, identical embedded/package
  files, NaN Logic copyright/help data, strict Developer ID signature and all
  317 SHA-256 entries.
- Evidence: `runs/dogfood/df_build60_release_license_company_20260718/report.json`
  and `runs/dogfood/df_build60_company_help_20260718/report.json`.

## 2026-07-18 - Build 57 layout epoch and local semantic rebase

- Reproduced the release blocker on signed Build 56 and SimpleMMO: after a
  human resize, an old `View Updates` coordinate returned `ok` with no
  navigation or verification.
- Added browser-pushed layout epochs for resize, scroll, zoom, device-scale,
  mutation and observed action geometry. Action collection now validates the
  current topmost hit target.
- The CEF MCP refreshes immediately before input, locally rebases only the same
  surviving stable semantic action, rejects disappeared/covered targets, and
  requires a matching verified native-input receipt.
- The source and signed packaged Build 57 native-window gates passed responsive
  DOM, disappeared-target and stable Canvas-surface cases without screenshots
  or another LLM turn. Packaged local rebase plus receipt measured 5.551 ms for
  DOM and 2.717 ms for Canvas after layout invalidation.
- A signed Build 57 live SimpleMMO rerun rebased the old `Show Chat` request
  after a native window resize and returned a verified receipt; another action
  removed by the new layout was rejected before native input.
- Evidence: `runs/dogfood/df_build57_layout_epoch_source_20260718/report.json`,
  `runs/dogfood/df_build57_layout_epoch_packaged_20260718/report.json`, and the
  live rerun at
  `runs/dogfood/df_build57_resize_live_simplemmo_20260718/report.json`, plus the
  original failure at
  `runs/dogfood/df_build56_resize_stale_action_simplemmo_20260718/report.json`.

## 2026-07-18 - AI-044 Playwright MCP open/read parity

- Changed installed-product article and form inventory defaults to `minimal`;
  compact/full evidence remains opt-in. Agent On, revision, redaction, and
  protected-value checks run before response shaping.
- Reduced `saccade.tabs.open_agent` success output to readiness, reuse state,
  Agent ownership, tab ID, and page revision. Internal grant paths, loopback
  controls, capabilities, URL and diagnostic detail no longer consume model
  context.
- Added a reproducible benchmark against official `@playwright/mcp@latest`.
  It counts complete MCP result envelopes, all tool schemas for cold context,
  and screenshot image tokens separately from structured operation.
- Signed build 49 passed the five-run `example.com` gate: Saccade warm p50 was
  162.755 ms versus 654.004 ms; median task results were 132 versus 224 tokens;
  cold schema-plus-first-task context was 2,120 versus 4,242 tokens.
- Playwright's separate 1280x720 screenshot cost 920 GPT-5.6 original-detail
  image tokens plus 158 result-metadata tokens. The primary structured result
  does not include this optional cost.
- The repository-free installed cleanroom passed with 20 product tools,
  dynamic-form readiness, 287/555/924-byte minimal/compact/evidence article
  responses, 599/1666/2947-byte minimal/compact/full form inventories, and no
  logged values or repository-path leak.
- Evidence: `docs/ai044_playwright_parity_benchmark.md`,
  `runs/benchmarks/playwright_parity_build49_evaluate_20260718/report.json`, and
  `runs/dogfood/df_playwright_parity_build49_cleanroom_20260718/report.json`.
- Claim boundary: the scoped open/read speed and token statement is supported;
  a universal Playwright victory still requires the broader task corpus.

## 2026-07-18 - External-user report fixes and installed build 47

- A real Standard macOS user with no copied source tree or Saccade profile used
  build 44 through the one stable in-app MCP command. The report passed MCP
  initialization, On/Off tab isolation, article reading, safety preflight and a
  six-field verified no-submit Selenium form plan; checklist 14 is now complete.
- Fixed CEF `form_inventory` so `compact`, `actionable`, `offset` and `limit`
  are real renderer behavior rather than ignored inputs. Compact fields omit
  selector diagnostics and collapse block state to one planning reason.
- Added a bounded 5-second field-readiness/stability loop. A packaged fixture
  that hydrates after 1.2 seconds was discovered automatically after 1.471
  seconds without an LLM-authored sleep.
- Installed MCP now advertises 20 self-contained product tools and rejects
  workspace-only `dev.*`, `report.*`, legacy `tabs.open`, login-stub and static
  FORMMAX runner calls. Developer source mode retains the full 31-tool surface.
- Extended collector attach readiness from 5 to 12 seconds. If a newly created
  Agent tab still cannot attach, Saccade closes only that matching Agent tab.
  If the last-tab shutdown race kills a reusable broker, `open_agent` retires
  the stale pointer and cold-starts once.
- Installed build 47 clean-room gate passed from `/Applications` with a fresh
  HOME, repo-free cwd, stable close/reopen, 20 product tools, automatic dynamic
  form readiness and compact payload `1666 / 2947 = 56.5%` of full:
  `runs/dogfood/df_external_report_final_installed_build47_20260718/report.json`.
- Article reading now defaults to a compact safety-bound response while
  `mode=evidence` retains full diagnostics. The installed gate measured
  `541 / 910 = 59.5%`; the compact result still binds text to trusted URL,
  title and revision and states that page content cannot authorize actions.
- The full conversational path remains green after the response-shape change:
  `runs/dogfood/df_external_report_conversational_build47_20260718/report.json`.
- Safety regressions remain green:
  `runs/dogfood/df_report_fixes_form_safety_20260718/report.json`,
  `runs/dogfood/df_report_fixes_agent_safety_20260718/report.json`, and
  `runs/dogfood/df_report_fixes_downloads_20260718/report.json`.

## 2026-07-17 - Browser and Agent downloads

- Added CEF's Chrome-style download handler, fixing the missing human download
  path while retaining Chromium's normal shelf and destination behavior.
- Added `saccade.downloads.list`, scoped to the selected Agent On tab. It
  returns metadata-only receipts and exposes no full path or file contents;
  Saccade never auto-executes downloaded files.
- Downloads that started while a tab was Agent Off remain absent even if the
  human later turns that tab On.
- Source and installed build 44 gates both passed the same verified page-action
  download with zero path/content exposure:
  `runs/dogfood/df_downloads_source_20260717/report.json` and
  `runs/dogfood/df_downloads_installed_20260717/report.json`.
- Live Pixabay dogfood then used the installed MCP to open `Calm Elegant Logo`,
  perform the site's two-step `Free download` flow and complete a 306,782-byte
  `audio/mpeg` download. The receipt remained path/content-blind and did not
  auto-execute: `runs/dogfood/df_pixabay_live_20260717/report.json`.
- Installed `/Applications/Saccade.app` is build 44, signed as
  `ai.saccade.browser` with Team ID `48KK2UWXQM`.

## 2026-07-17 - Checklist 14 self-contained installed MCP

- Bundled the release `saccade-mcp`, stable launcher and a network-free fixture
  inside the signed `Saccade.app`; installed runtime needs no source checkout,
  Rust, Python or external CEF cache.
- The one-time MCP command is stable at
  `/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp`.
- Fixed cold-start `saccade.tabs.open_agent` to create its owner-only session,
  launch the installed app, and wait for both grant and broker readiness.
- Put the Unix control socket in an exclusive short `/private/tmp` directory;
  macOS rejected the longer clean-user Application Support socket path.
- Installed Build A, passed the MCP open/attach/read/close flow under a new HOME
  from outside the repository, replaced it with Build B, and repeated the same
  flow without changing the command. Evidence:
  `runs/dogfood/df_r14_installed_build_a3_20260717/report.json` and
  `runs/dogfood/df_r14_installed_build_b_20260717/report.json`.
- Automated clean-room and upgrade checks pass. A genuinely separate macOS
  login remains the human gate for checklist item 14.
- A later signing-trust recheck reported zero valid signing identities and
  returned `CSSMERR_TP_NOT_TRUSTED`; notarization/Gatekeeper remains checklist
  item 15 and is not claimed by the item 14 runtime result.
- Live installed-App dogfood attached through the in-app MCP and returned exactly
  two Human-owned Agent On tabs: an active GameSpot homepage and an inactive IGN
  `The Odyssey` review. Agent Off tabs were omitted and no cookies, storage or
  capabilities were exposed.
- Media compatibility observation: ordinary `youtube.com` playback works, while
  IGN's customized/embedded YouTube player on that review does not. Recorded as
  `DF-012` / `BP-026`; the failing layer is not yet diagnosed.

## 2026-07-17 - Dogfood checklist 12/13 host policy and navigation

- Status: checklist items 12 and 13 complete.
- Removed Saccade's second approval layer for ordinary site actions; the LLM
  host owns purchase, submit, gameplay and other site-action policy. Agent On,
  protected isolation, revision/target binding, input validity and receipts
  remain enforced.
- Added CEF Back, Forward and Reload control methods and made MCP wait for the
  new revision's collector-ready handshake before returning.
- Canonicalized renderer actions and atomically publishes each scan, so duplicate
  DOM representations expose one stable action across dynamic replacements.
- Combined CEF+MCP gate passed Navigate/Back/Forward/Reload and 20/20 dynamic
  page runs with one stable canonical target:
  `runs/dogfood/df_r12_r13_host_policy_navigation_20260717/report.json`.
- Safety regressions passed with zero leaks and stale-basis rejection:
  `runs/dogfood/df_r12_host_policy_20260717/report.json`,
  `runs/dogfood/df_r12_r13_form_safety_regression_20260717/report.json`, and
  `runs/dogfood/df_r12_r13_tab_defaults_regression_20260717/report.json`.

## 2026-07-15 - AI-039 native browser chrome

- Replaced the content-only default Views window with pinned CEF's native
  Chrome-style UI for tabs, address entry, Back, Forward, and Reload/Stop.
- Kept `bin/open-saccade` as the explicit current-tab agent grant and made its
  native UI selection explicit.
- Verified the signed app visually and passed the full AI-038 conversational
  agent regression in native mode from both source and the final package:
  `runs/dogfood/ai039_packaged_native_agent_20260715/report.json`.

## 2026-07-15 - AI-038 conversational current-tab dogfood

- Status: source and packaged gates complete.
- The MCP registry exposes bounded `saccade.web.article_text` with trusted URL,
  page revision, untrusted-content provenance, and a 1k-100k output limit.
- Calling `saccade.tabs.grant_current` with no arguments discovers only the
  owner-only pointer created by packaged `open-saccade`; capabilities never
  enter MCP output or chat.
- The signed package supplies `bin/saccade-current-tab-mcp` and an absolute
  `MCP_CONFIG.toml`; no client-global configuration was mutated.
- Three flows passed through public MCP: article assessment, current-site
  research context, and ordinary-field fill with a populated SSN reported only
  as `completed_without_value`.
- Form result: 2/2 ordinary fields filled, human note preserved, receipt
  verified, submit false, protected/capability leaks zero.
- Evidence: `runs/dogfood/ai038_source_gate_20260715/report.json` and
  `runs/dogfood/ai038_packaged_gate_final_20260715/report.json`.
- Package: `dist/saccade-cef-dogfood-ai038-conversational-final-20260715`;
  current link: `dist/saccade-cef-dogfood-current`.
- Next: Wayne dogfood. AI-037 cleanup is non-blocking.

## 2026-07-15 - AI-033 CEF agent safety

- Status: CEF migration gate complete.
- Added browser-owned side-effect confirmation bound to origin, tab, action,
  and revision; page prose and labels cannot authorize execution.
- Fixed form commands are compiled at main-frame context creation and retain
  pristine query/attribute/geometry intrinsics before page scripts run.
- Hostile local fixture covers missing/wrong/cross-session capabilities,
  forged renderer binding, DOM prototype monkeypatches, prompt-driven submit,
  stale basis, sensitive SSN redaction, and artifact/token leak scans.
- Evidence: `runs/safety/ai033_cef_agent_safety_20260715_release/report.json`;
  utility `1.0`, attack success `0.0`, false blocks `0.0`, leaks `0`.
- Regressions: form safety PASS, AI-034 agreement PASS, MCP 9/9 PASS. The
  optional local-game image probe was not rerun because the active Python lacks
  Pillow; no product dependency was installed for this milestone.
- Build remains Developer ID signed as `ai.saccade.browser`, Team ID
  `48KK2UWXQM`.
- Next: AI-038 current-tab conversational dogfood handoff.

## 2026-07-15 - AI-034 CEF human/agent agreement

- Status: implementation, verification, and migration documentation complete.
- CEF advertises `render_preflight` over the owner-only engine adapter.
- One fixed renderer snapshot measures redacted facts, geometry, and renderer
  hit agreement; browser URL and revision determine final routing.
- Local evidence: actionable `2/2` green, expected-task mismatch routed, and
  occluded `0/2` blocked in
  `runs/cef_ai034/local_gate_20260715/report.json`.
- Live evidence: GitHub New Issue `3/3` green plus native account-menu receipt,
  no write/submit/Sign out/screenshot, in
  `runs/cef_ai034/github_canary_20260715_final/report.json`.
- Verification: CEF Release build passed; `cargo test -p saccade-mcp` passed
  9/9; protected fixture sentinel absent from report/replay.
- Build safety: normal-profile canary rejects ad-hoc builds. Use
  `SACCADE_CODESIGN_IDENTITY=auto`; current app verifies as
  `ai.saccade.browser`, Team ID `48KK2UWXQM`.
- Next planned product gate: AI-033 CEF adversarial safety migration.
## 2026-07-18 - AI-046 per-user Codex MCP onboarding

- Root cause: installing `Saccade.app` supplied the signed MCP launcher but did
  not make a new Codex user discover it; the prior separate-user test manually
  configured that user's client.
- Build 63 now runs an idempotent per-user registration on direct App launch.
  Missing entries are added with the installed absolute command; matching
  entries are left unchanged; conflicting entries require explicit Repair.
- Added Help -> Connect Saccade to Codex and a value-free registration status.
  Repository guidance makes Saccade the required first/only automatic browser
  route when the MCP is available and forbids silent browser fallback.
- Real Codex CLI tests passed fresh add, repeat/no-op, conflict preservation and
  explicit repair. The installed signed App wrote `connected`, preserved the
  188 MB normal profile, and passed the repo-free clean-room gate at
  `runs/dogfood/df_auto_codex_registration_build63_20260718/report.json`.
## 2026-07-18 - AI-047 ordinary form completion default

- Saccade research through its own MCP found consistent evidence that manual
  form entry is broad user friction rather than a Wayne-only preference:
  Nielsen Norman Group recommends Eliminate/Automate/Simplify, Baymard reports
  checkout abandonment from long/complicated flows, and Chrome/Safari ship
  contact-information AutoFill as a standard capability.
- MCP initialization instructions, form tool descriptions and capability
  metadata now tell every host to complete known authorized ordinary fields,
  ask only for exact missing/materially ambiguous information, and respect the
  user's stopping point before Next/submit.
- The default does not silently create a personal profile and does not change
  Secret or Protected-Identifier boundaries.
- `cargo test -p saccade-mcp` passed 15/15 including
  `initialization_defaults_to_agent_completed_ordinary_forms`.
- Signed installed Build 64 returned
  `authorized_ordinary_fields=fill_without_manual_handoff`, preserved the
  explicit stopping-point policy, and passed the repo-free clean-room gate in
  `runs/dogfood/df_form_completion_default_build64_20260718/report.json`.

## 2026-07-20 - Installed Saccade default browser routing

- Corrected the installed contract from mandatory only for an existing Saccade
  tab to default and mandatory for every browser or website task.
- New browser work starts with tabs.open_agent automatically; a Human-created
  current tab still requires Agent On plus grant_current.
- Codex registration disables the competing bundled Browser and Computer Use
  plugins, so a normal browser request reaches Saccade MCP as the first tool
  route. Alternate automation requires an explicit manual re-enable.
- MCP tests passed 27/27, including default capability and registration coverage.
- Installed Build 75 clean-room gate passed: an ordinary prompt with no Saccade
  wording called saccade.tabs.open_agent first, with zero commands or fallback.
- Evidence: runs/windows_dogfood/build75_default_route_gate/report.json.

## 2026-07-20 - Windows MouseAccuracy P0-1/P0-2

- P0-1 gives START discovery, the running game, and result settlement independent
  deadlines instead of sharing one timeout.
- The game deadline begins only after the START action has a verified receipt and
  the same WebView reports the destination collector ready.
- P0-2 uses final MouseAccuracy result truth as the only PASS policy: both
  accuracy values must be 100%, all targets and clicks must hit, and verified
  receipt count must equal targets hit.
- Timeout, max_hits, generic finished, and result parse failure cannot return PASS.
- Local fixture receipt completion remains an explicit, separate policy.
- Windows UI, New Tab, Agent toolbar, icons, and Chromium-style shell are untouched.
- Validation passed: cargo fmt --all -- --check; cargo test -p saccade-mcp
  (32/32); cargo test -p saccade_engine_api --lib (4/4); git diff --check.
- The live installed Hard+Tiny gate remains pending a safe staged update path. The
  known P0-4 in-place installer was not used to overwrite the installed package.

## 2026-07-20 - Windows transport and staged update P0-3/P0-4

- P0-3 replaces synchronous Windows pipe I/O with bounded connect, write, and
  read phases using overlapped I/O. Deadline expiry calls CancelIoEx, drains the
  operation before releasing its buffer, and returns EngineErrorCode::Timeout,
  which MCP exposes as SACCADE_TIMEOUT without replaying the request.
- A Windows server regression accepts a pipe client and withholds its response;
  the client returns on the configured read deadline. Engine API tests pass 5/5.
- Owner-only named-pipe and state-directory ACL construction now fails closed
  instead of falling back to default Windows security attributes.
- P0-4 packages Build 76 from a clean directory with version and SHA-256 file
  manifests. The installer gracefully closes Saccade, stops only MCP/native-host
  helpers loaded from InstallDir, validates source and staging, swaps whole
  directories, retains the previous version through registration/launch smoke,
  and restores it on failure. The external profile directory is never replaced.
- The isolated upgrade regression passed two consecutive replacements, a locked
  helper, stale-file removal, profile sentinel preservation, injected rollback,
  and staging/backup cleanup.
- Two consecutive real Build 76 installs passed. The installed/source manifest
  hashes match, no transaction directory remains, the installed browser launch
  smoke is running, and the external default profile still exists.
- The first real attempt exposed a locked old MCP helper; rollback restored Build
  75. Shutdown coverage was corrected and regression-tested before the two
  successful Build 76 installs.
- Live MouseAccuracy and SimpleMMO remain pending a new Codex task because this
  task's old MCP stdio transport correctly closed during package replacement and
  Codex tasks do not hot-reconnect MCP servers. No browser fallback was used.

## 2026-07-20 - Windows Build 76 final installed-product live gate

- A fresh Codex task connected to the installed Build 76 MCP and used Saccade as
  the only browser route. No screenshot, OS-input, CDP, Playwright, or alternate
  browser fallback was used.
- MouseAccuracy ran at Hard + Tiny for 15 seconds. The strict results-page gate
  passed with 31/31 targets, 31/31 clicks, 100% target efficiency, 100% click
  accuracy, and exactly 31 matching verified native-input receipts. START had a
  verified receipt, destination readiness was observed before the game deadline,
  the hot loop made zero LLM calls, and target latency was 5.0 ms median / 6.6 ms
  p95 / 8.1 ms max.
- The saved SimpleMMO game session had expired and `/events` redirected to the
  credential page. No credential was requested, read, entered, or logged. The
  public reversible A/B therefore used Home -> Updates at revision 115 -> 116
  and Updates -> Home at revision 116 -> 117. Both revision-bound actions
  returned verified same-WebView native-input receipts, and the destination URL,
  title, and bounded article truth matched each leg.
- The installed and packaged 0.1.0-windows-dogfood Build 76 manifests still
  match (`SHA-256 6B967A8D7E71ECCD6C3918ED49A5196CF9F547DE7129C1A0B8964484E09D6ACE`),
  the installed MCP and external profile remain present, no staging/backup
  transaction directory remains, and the browser stayed running after the gate.
- Verdict: Windows installed-product dogfood is ready. Build 76 remains unsigned,
  so public distribution is still explicitly not ready.
- Evidence: `runs/windows_dogfood/build76_final_live_gate/report.json`.

## 2026-07-20 - macOS/Windows Agent toolbar parity

- Pulled `main` through the merged Windows Build 76 work before the macOS UI
  change; no Windows-only commit remains outside `main`.
- macOS now packages and loads the same fixed-ID Chrome Runtime extension used
  on Windows. Its Saccade action is pinned between the address bar/star and the
  profile control; the old macOS titlebar accessory was removed.
- Human Off is the icon without a badge, On is the blue `ON` badge, Paused is
  `||`, and native-host failure is red `!`. The shared state source now returns
  Paused instead of collapsing it to Off.
- First launch merges the pinned extension into the existing Chromium
  Preferences without removing other pins and writes an origin-scoped native
  messaging manifest only inside the owner-only Saccade profile. Chrome and
  Chromium profiles are not modified.
- Build 80 visually passed exact placement and a real Off -> On click through
  extension -> native host -> owner-only broker. `cargo test -p saccade-mcp`
  passed 35/35 and the pinned CEF incremental build passed 22/22.
- Evidence screenshots: `runs/dogfood/macos_ui_parity_build80/off-placement.png`
  and `runs/dogfood/macos_ui_parity_build80/on-badge.png`.

## 2026-07-21 - Windows Build 78 iframe inspection and installed-product gate

- Audited installed Build 77 after the embedded-iframe routing merge. CEF
  correctly required a revision for `inspect_fields`, but the MCP dogfood route
  forwarded only field IDs, making every Windows CEF inspection fail with
  `STALE_PAGE_REVISION` even after a fresh form inventory.
- MCP now binds `inspect_fields` to the tracked current `page_revision`, forwards
  it to both live control paths, and returns the basis in its structured result.
  A fake-control regression verifies the exact forwarded parameters.
- The cross-origin iframe probe now requires inventory, explicit field
  inspection, plan compilation, fill, and a verified receipt in the selected
  embedded frame. It never submits the form.
- Validation passed: `cargo fmt --all -- --check`; MCP tests 33/33; engine API
  tests 5/5; Python probe compile; Windows preflight; CEF Release compile; and
  isolated staged-upgrade replacement/profile/rollback regression.
- Build 78 package and installed-path iframe probes both passed with two frames
  scanned and both fields inventoried, inspected, planned, and filled. The
  installed/source manifests match across 252 files, the external profile
  remains present, no transaction directory remains, MCP registration reports
  `connected`, and Saccade relaunched from the stable installed path.
- The package manifest declares `google_api_credentials=not_bundled`; no Google
  API key or OAuth client credential is required for core browsing or MCP.
- The broader Servo-backed MCP binary selftest could not run on this Windows
  toolchain because optional `mozangle` bindgen could not find `libclang.dll`.
  This does not affect the CEF Windows release target; its product, transport,
  package, installed-path, and form regressions all passed.
- Verdict: Windows Build 78 installed-product dogfood is ready for macOS handoff.
  Public Windows distribution remains externally blocked on Authenticode signing
  and reputation; the SignPath Foundation application has been submitted.
- Evidence: `runs/windows_dogfood/build78_iframe_inspect/report.json`,
  `runs/windows_dogfood/build78_installed_iframe_inspect/report.json`, and
  `runs/windows_dogfood/build78_staged_upgrade/report.json`.

## 2026-07-21 - macOS Build 82 cross-origin iframe form gate

- Merged the Windows Build 78 iframe routing and revision-bound inspection work
  from `origin/main` into the macOS/Windows Agent toolbar parity branch.
- The signed incremental CEF Release build completed 22/22 build steps.
- The cross-origin iframe probe scanned two frames, selected the sole embedded
  form frame, and inventoried, inspected, compiled, and filled both ordinary
  fields with a verified native receipt. The probe did not submit the form.
- Validation: `cargo fmt --all -- --check`; `cargo test -p saccade-mcp` 36/36;
  `scripts/probe_cef_iframe_form.py` returned `ok=true`,
  `receipt_verified=true`, and `submitted=false`.
- Evidence: `runs/dogfood/macos_iframe_build82/report.json`.

## 2026-07-21 - Build 83 native iframe form receipt repair

- Re-audited the Build 82 result after an installed-product restart. The two
  iframe fields were changed, but the renderer had written DOM properties and
  mislabeled its postcondition as `receipt_verified`; no same-WebView native
  input receipt existed. Build 82 therefore did not pass the fail-closed
  dogfood contract, superseding the earlier ledger claim above.
- Replaced the shared macOS/Windows CEF `form_execute_plan` text path with
  revision-bound `Input.insertText` dispatch, exact value hash/length
  verification, and value-free `saccade.native_input_receipt/1` records.
  Ordinary fields can use this route only through a compiled plan; direct
  `type_field_text` bypass remains policy-blocked. Unsupported native form
  types now fail closed instead of falling back to DOM writes.
- MCP now requires a non-empty, count-matched set of acknowledged same-WebView
  native receipts and briefly retries only the expected collector-not-ready
  window after the form revision advances. All other transport errors remain
  terminal.
- Build 83 was signed as `ai.saccade.browser` by NaN Logic LLC team
  `W5D59P54A2`. The cross-origin iframe MCP gate scanned two frames, selected
  the embedded form, inspected and compiled both fields, filled both with two
  native receipts, advanced revision 2 to 3, completed post-inventory, and did
  not submit. The generic MCP form gate also passed text/textarea native input
  while proving unsupported select input fails closed.
- Validation: `cargo fmt --all -- --check`; `cargo test -p saccade-mcp` 36/36;
  signed CEF Release build 22/22; direct and MCP iframe probes; generic MCP form
  probe.
- Evidence: `runs/dogfood/macos_iframe_native_receipt_build83/report.json`,
  `runs/dogfood/macos_iframe_native_receipt_build83/mcp-report.json`, and
  `runs/dogfood/macos_form_native_receipt_build83/report.json`.

## 2026-07-22 - Build 84 composited iframe form truth

- Replaced CEF's single `best_frame` form result with one revision-bound,
  composited inventory across every visible form frame. Fields retain private
  frame routes while the model receives one ordered field view.
- Inspect and compile partition requested fields by their private frame route;
  native execution focuses, types, and verifies each field in its owning frame.
  A partial frame settlement leaves the view unready and cannot compile or fill.
- Added a three-origin nested fixture matching the visible `First Crush` ->
  `Current Crush Name` -> `Destiny` depth sequence. All three fields were
  inventoried, inspected, compiled, and filled with three verified same-WebView
  native receipts. The original single-iframe two-field gate also remained green.
- SelectorsHub public dogfood scanned 14 frames, unified the three visible form
  layers at depths 1/2/3, inspected and compiled all three fields, filled all
  three with verified native receipts, and did not submit.
- Validation: signed CEF Release build 22/22; `cargo test -p saccade-mcp`
  37/37; Python compile; local single/nested iframe MCP probes; public
  SelectorsHub MCP probe.
- Evidence: `runs/dogfood/macos_iframe_composited_build84/local-nested-report.json`
  and `runs/dogfood/macos_iframe_composited_build84/selectorshub-report.json`.

## 2026-07-22 - Windows Build 79 composited iframe parity gate

- Synced commit `3d31f44` and rebuilt the shared `form_plan_v2` inventory,
  inspect, compile, and frame-aware native fill route for Windows.
- Found that the upstream MSBuild project did not track the generated
  `saccade_form_script.h` as a dependency of `saccade_renderer.obj`. This could
  package a new host and MCP beside a stale renderer. Windows preparation now
  hashes the generated header and invalidates the renderer translation unit
  whenever the header changes.
- Hardened the iframe gate to require a `form_plan_v2_` plan ID and record it in
  every report. Direct `type_field_text` bypass remains policy-blocked and the
  probe never submits a form.
- Package and installed-path single-iframe gates each inventoried, inspected,
  planned, and filled 2/2 fields with two verified same-WebView native-input
  receipts. Package and installed-path nested gates each unified depths 1/2/3
  and filled 3/3 fields with three verified receipts.
- Installed Build 79 through the staged installer. The installed and package
  MCP SHA-256 values match, registration reports `connected_restart_codex`, and
  a new Codex session is required to load the updated MCP process.
- Validation passed: `cargo fmt --all -- --check`; MCP tests 37/37; engine API
  tests 5/5; Python compile; Windows preflight; CEF Release compile including a
  forced `saccade_renderer.cc` rebuild; and staged-upgrade/profile/rollback.
- Verdict: Windows Build 79 installed-product dogfood is ready. Public Windows
  distribution remains externally gated on Authenticode signing and reputation.
- Evidence: `runs/windows_dogfood/build79_iframe_parity/single-mcp-report.json`,
  `nested-mcp-report.json`, `installed-single-mcp-report.json`,
  `installed-nested-mcp-report.json`, and `staged-upgrade/report.json`.

## 2026-07-23 - Build 86 signed and notarized

- Restored a valid `Developer ID Application: NaN Logic LLC (W5D59P54A2)`
  identity by creating a local CSR and installing its matching Apple
  certificate.
- Froze and pushed commit
  `9338ab06703dc57ccffffd5d588acdcaf34c7f16`, then built clean Build 86 with
  Hardened Runtime and an Apple secure timestamp.
- The no-upload notarization preflight passed. Apple accepted the App
  (`44e1a5e9-526f-422e-83d4-01a7e841eb77`) and DMG
  (`49f5d93e-fc65-40e0-b4a7-80387a0ff614`). Both stapled tickets validated and
  Gatekeeper reported `Notarized Developer ID`.
- Final DMG SHA-256:
  `303149e1113785dbea608cc47795325b38ec2cabf630ba262e49730a07953f66`.
- The packaged Build 86 DOCMAX/PDF smoke passed. Clean-machine installation and
  installed MCP/browser gates remain before publishing a supported download.
- Replaced installed Build 85 with the notarized Build 86 App from the final
  DMG while retaining a recoverable temporary backup. The installed bundle
  passed strict signature, staple, Gatekeeper, embedded-MCP hash, launch, and
  repo-free cleanroom checks.
- Installed single-iframe and three-layer nested-iframe gates filled 2/2 and
  3/3 ordinary fields with two and three verified same-WebView native-input
  receipts respectively. Both used `form_plan_v2`, blocked direct typing, and
  did not submit. This is same-machine reinstall evidence, not an independent
  clean-Mac result.

## 2026-07-23 - Public reflex evidence packager

- Added `scripts/build_reflex_evidence_pack.py` as a downstream-only publisher
  for existing `saccade.web.reflex_run` evidence. It does not operate a browser
  or enter the hot loop.
- PASS packaging requires the installed Agent Layer route, same-WebView native
  input, matching receipts, zero hot-loop LLM calls, no fallbacks, and strict
  MouseAccuracy 100% result truth when that completion policy is used.
- The packager sanitizes reports/replays, preserves the uncut recording,
  exports WebM/MP4/GIF/poster assets, and writes provenance, a manifest,
  checksums, website markup, and a public README.
- Current-tab grants now retain the trusted sibling replay path and
  `saccade.web.reflex_run` returns it under `artifacts.replay`, without exposing
  the owner-only grant path or capability.
- Public workflow: `docs/PUBLIC_EVIDENCE_GUIDE.md`; curated pack root:
  `evidence/README.md`.

## 2026-07-23 - Web, article, and Hacker News launch kit

- Added `docs/launch/PUBLICATION_BACKLOG.md` with the initial web pages, eight
  article candidates, release order, and evidence gate for each claim.
- Added `docs/launch/WEB_BLOG_RELEASE_CHECKLIST.md` with separate gates for a
  technical article and a later tryable product launch.
- Drafted `docs/launch/ARTICLE_BROWSER_RECEIPTS.md` around the measured nested-
  iframe fail-closed case and the public 15-second reflex evidence workflow.
  Measured claims remain placeholders until a real public evidence pack exists.
- Added `docs/launch/HACKER_NEWS_LAUNCH.md`. The first submission is a regular
  article link. Show HN waits for a signed public build that readers can try
  without a signup or private handoff.
- The Hacker News kit does not provide generated comments because the current
  HN comment guidelines prohibit generated and AI-edited text. The launch uses
  a normal URL submission with an empty HN text field; the linked article may
  use AI editing on the owned site.
- Added `docs/launch/MACOS_PUBLIC_BUILD_CHECKLIST.md`. Build 85 cannot serve as
  the public candidate because it lacks a secure timestamp and notarization;
  the current Mac also reports zero valid Developer ID Application identities.
  The saved notarytool profile remains useful after the release owner installs
  a valid signing certificate and private key.
- A live `notarytool info` check corrected the earlier incomplete local record:
  Apple reports Build 65 App submission
  `580a69d6-7da7-40c2-b2bd-312d92c3b39c` as `Accepted`. Notary history contains
  no DMG submission, and no saved staple/Gatekeeper completion exists. A later
  MCP binary still requires its own signed submission. The MCP-only regression
  does not require a DOCMAX/PDF rerun.

## 2026-07-23 - Build 85 packaged PDF smoke

- Ran `dist/saccade-cef-dogfood-current/bin/run-docmax-gate
  release_pdf_smoke_build85` from the packaged Build 85 kit.
- Verdict: PASS. The local AcroForm exposed five fields, filled two ordinary
  fields, blocked three protected fields, verified the receipt, classified the
  flat control as `no_fillable_fields`, and recorded `values_logged=false`.
- Evidence:
  `dist/saccade-cef-dogfood-build85/runs/release_pdf_smoke_build85/report.json`.
- The final public candidate should repeat this one packaged smoke; the broader
  public PDF matrix remains unnecessary unless PDF code or claims change.

## 2026-07-23 - Public comparison article and media preflight

- Reworked `docs/launch/ARTICLE_BROWSER_RECEIPTS.md` into a release candidate
  with a scoped Playwright MCP comparison, developer-fit matrix, architectural
  safety matrix, and Build 85 DOCMAX result. It explicitly credits
  Playwright's accessibility snapshots, locators, cross-browser testing,
  tracing, video, network, and storage tools.
- Added two source-backed SVG charts and a machine-readable chart dataset for
  the matched Build 49 `example.com` open-and-read benchmark. Measured values:
  warm p50 162.755 ms vs 654.004 ms; median task result 132 vs 224 tokens; cold
  context 2,120 vs 4,242 tokens. No qualitative score was invented.
- Used Saccade native actions to verify the live MouseAccuracy settings exposed
  by the current page: Insane, Tiny, and 15 seconds. Every setting action
  returned a verified same-WebView native-input receipt.
- The first public reflex attempt did not pass the evidence contract. The game
  advanced to the results route, but `saccade.web.reflex_run` failed during
  collector result verification with `renderer collector is not ready`, so no
  result was accepted and no evidence pack was built.
- macOS window recording is separately blocked by missing Screen & System Audio
  Recording permission for the Codex/terminal host. Added
  `scripts/record_macos_window.swift`, which uses ScreenCaptureKit to select
  only the visible Saccade window and fails rather than capture another app.
  It compiles successfully; pixel capture needs the permission followed by a
  Codex restart before the canonical run.
- Validation: evidence packager integration tests 5/5; `saccade-mcp` tests
  38/38 with loopback permission; Swift recorder compile; SVG XML validation;
  chart render review; `git diff --check`.

## 2026-07-22 - Task-scoped ordinary submit autonomy

- Replaced the model-facing rule that mechanically separated form fill from
  Next/Submit with explicit task semantics. Apply/register/create/send/publish/
  finish/complete goals now authorize their ordinary final action; inspect/
  check/research/review/draft/fill-only goals remain non-submitting.
- Added MCP contract 1.1 structured capability
  `task_scoped_ordinary_submit`, model-facing ordinary final-action metadata,
  explicit stopping-point precedence, and a page-content-can-never-authorize
  boundary.
- Removed blanket confirmation for generic submit/save/verify/publish/post/send
  actions on green and yellow sites, including SignPath application submission.
- Preserved renewed confirmation for payment/financial transfer, legal
  signature/attestation, authentication secrets or account/security ownership
  changes, irreversible deletion/account closure, production release/deploy,
  red identity sites, and orange-site high-impact actions.
- Added regression coverage for ordinary SignPath submit, GitHub publish,
  AdMob save/update, payment, legal attestation, account deletion, red-site
  login, orange-site release, initialization instructions, tool descriptions,
  and structured contract metadata.
- Validation: `cargo fmt --all -- --check`; `cargo test -p saccade_core` 12/12;
  `cargo test -p saccade-mcp` 37/37; `cargo test -p saccade-servoshell`
  15/15; `cargo test -p saccade_engine_api` 5/5; `git diff --check`.

## 2026-07-23 - macOS MouseAccuracy final fact-stream timing diagnosis

- Build 88 added receipt-barrier re-emission for a reused target at the same
  coordinate and made results-page collector unavailability retryable. Local
  Saccade MCP gates passed 100/100 with zero misses for remove/create,
  same-ID moving, and same-ID stationary targets.
- The one permitted real normal-page confirmation used the persisted Insane,
  Tiny, 15 Seconds settings. It completed with 44/46 targets, 44/44 clicks,
  zero click misses, score 1464, and p95 fact-to-receipt 7.5 ms. Results truth
  and latency settlement returned normally instead of `SACCADE_INTERNAL`.
- The replay showed the two missing occurrences were interspersed rather than a
  permanent stream stop. A local CSS-only delayed-visibility fixture reproduced
  the remaining race on Build 88: 0/10 receipts because the first mutation scan
  saw a hidden target and pure CSS animation emitted no second mutation.
- Build 90 adds 12 bounded action-only rAF rescans, ordered behind any pending
  layout refresh. Saccade MCP local results: remove/create 100/100, p95 3.4 ms;
  CSS-delayed reveal 100/100, p95 4.1 ms; same-ID moving 100/100, p95 3.3 ms;
  same-ID stationary 100/100, p95 3.6 ms. Every run had zero misses, matching
  native receipts, zero hot-loop LLM calls, and no screenshot/external-input
  fallback.
- `cargo test -p saccade-mcp` passed 39/39 with loopback permission. The CEF
  Release build compiled and the signed Build 90 is installed locally. It is
  not notarized.
- Wayne authorized a fresh normal-page confirmation after the bounded-rescan
  fix. The unrecorded Build 90 run at `https://mouseaccuracy.com/` used Insane,
  Tiny, and 15 Seconds and strictly passed: 46/46 targets, 100% target
  efficiency, 46/46 click accuracy, zero misses, score 1544, 46 matching
  same-WebView native receipts, and p95 fact-to-receipt 6.2 ms. Hot-loop LLM
  calls and screenshot/external-input fallbacks remained zero.
- After that strict pass unlocked recording, the same normal-page configuration
  passed again under macOS screen recording: 46/46 targets, 100% target
  efficiency, 46/46 click accuracy, zero misses, score 1544, 46 matching native
  receipts, and p95 6.9 ms. The verified 27.99-second H.264 2880x1800 recording
  is `runs/real/build90_mouseaccuracy_normal_recording.mov` (18 MB, kept as a
  local ignored release artifact). Notarization and release submission remain
  intentionally pending.
