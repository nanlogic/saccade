# Saccade Dogfood Acceptance Checklist

Date: 2026-07-16

Use this file for the joint inspection pass. Check one item at a time and
record evidence before moving to the next item.

Status values:

- `PASS`: the installed build met the pass condition.
- `PARTIAL`: part of the condition passed; the listed gap remains.
- `FAIL`: a reproduced defect blocks the condition.
- `NOT RUN`: no installed-build result exists yet.

## A. Browser product

### 01. Product identity and branding

- Status: `PASS`
- Check: Inspect the Dock, app menu, About panel, process names, bundle metadata,
  icon, version, settings labels and bundled credits.
- Pass: Human-facing surfaces say `Saccade`; About shows a non-empty Saccade
  version and identifies Chromium as the engine; helper processes and bundle IDs
  use Saccade names; CEF/Chromium licenses and website compatibility tokens remain.
- Current evidence: Signed Build 62 uses Saccade for the app, executable, menu,
  icon, five Helper bundles and Chrome-runtime product strings. Helper IDs use
  `ai.saccade.browser.helper...`; About says `Based on Chromium`; the menu says
  `Settings…`; NaN Logic LLC and `nanlogic.com` are present in bundle,
  About/Help and release metadata. The app now supplies its own transparent
  64-by-64 Saccade favicon for Chromium's default/New Tab resources, closing the
  last visible Chromium-glyph gap without changing compatibility tokens. The
  real Help menu opens the company site in an internal Human-controlled Agent
  Off tab. The release gate passed product metadata, helper identity, icon,
  SBOM, checksums and strict signing. `Chromium Safe Storage` remains the
  accepted CEF keychain service name for this engine build.
- Source: `engines/cef/scripts/build_macos.sh`;
  `engines/cef/patches/0016-saccade-menu-branding.patch`;
  `engines/cef/patches/0023-nan-logic-company-help.patch`;
  `engines/cef/patches/0024-saccade-brand-resources.patch`;
  `runs/dogfood/df_build62_release_complete_20260718/report.json`;
  `runs/dogfood/df_build62_company_help_20260718/report.json`.

### 02. Native browser basics

- Status: `PASS`
- Check: Launch `/Applications/Saccade.app` and use New Tab, address entry,
  Back, Forward, Reload/Stop and Command-L.
- Pass: Tabs, the New Tab button, address bar, Back, Forward and Reload/Stop are
  visible. Command-L focuses the address bar, and each control works in the
  selected tab.
- Current evidence: Native tabs and address entry are present. AI-039 recorded all
  controls visible and passed its packaged regression. The current signed build
  is open on `test_pages/native_basics/page-one.html` for the human input check;
  macOS denied synthetic keyboard access, so this result stayed Partial until the
  user completed the controls sequence. On 2026-07-16, `/Applications/Saccade.app`
  was not present, but `dist/saccade-cef-dogfood-current/Saccade.app` opened the
  same local test page successfully. Human testing then confirmed visible tabs,
  tab switching, Back, Reload and Command-L/address entry behavior; the only
  remaining browser-basics complaint was `target=_blank` routing, which belongs
  to item 03. MCP navigation belongs to item 13.
- Source: `DF-R01`; `docs/ai039_native_browser_chrome.md`.

### 03. Tab versus popup routing

- Status: `PASS`
- Check: Open a local `target=_blank` link, a normal website child page and a
  legitimate OAuth or dialog popup.
- Pass: Normal pages become tabs in the current window. OAuth and dialog popups
  remain child windows.
- Current evidence: DF-001 was reproduced from the NaNMesh quickstart link. The
  CEF handler now redirects Chromium `NEW_FOREGROUND_TAB` and
  `NEW_BACKGROUND_TAB` requests into Saccade's existing Chrome-style tab strip,
  while leaving popup/window dispositions to the default child-window path. Local
  source regression `runs/cef_day5/session_tab_routing_20260716/report.json` and
  packaged regression
  `runs/cef_day5/session_tab_routing_packaged_final2_20260716/report.json`
  passed: the `target=_blank` child opened as a distinct ordinary tab, close
  recovered the parent tab and no ordinary value was logged. Human live testing
  on the rebuilt `ai042-tab-routing-20260716` kit confirmed the NaNMesh
  quickstart link now opens as a tab in the existing Saccade window. Legitimate
  popup/dialog behavior is preserved by leaving popup/window dispositions on the
  default child-window path. Build 62 additionally proved that a verified
  Agent-dispatched `opens_new_context` action creates an Agent On tab, while a
  human or Help-created tab remains Off; consent is not inferred from URL alone.
- Source: `DF-001`.

### 04. Saved-profile Keychain behavior

- Status: `PASS` (locked-Keychain boundary recorded)
- Check: From the installed app, choose `Always Allow`, relaunch three times,
  install one rebuilt package with the same Team ID and relaunch again.
- Pass: The saved profile prompts at most once. Incognito needs no persistent
  cookie key. Record the requesting binary and designated requirement.
- Current evidence: Current packaged app is `ai.saccade.browser`, Team ID
  `48KK2UWXQM`, and the saved profile is persistent with cookies/storage not
  exposed to the agent. `codesign -dv --verbose=4` on
  `dist/saccade-cef-dogfood-current/Saccade.app` reports
  `Authority=(unavailable)` and `Info.plist=not bound`, so the next check should
  record the designated requirement and confirm whether this local dogfood
  signing shape is causing repeated `Chromium Safe Storage` prompts. The user
  later reported repeated Google image challenges but no Keychain prompt at all;
  that is browser reputation / anti-bot behavior, not DF-002. A subsequent user
  check again reported no Keychain prompt. The first automated relaunch attempt
  used `open -a Saccade.app <url>`, which can hand the URL to the system browser
  and is not valid evidence. The valid check used
  `dist/saccade-cef-dogfood-current/bin/open-saccade
  https://www.nanmesh.ai/agents` for three app launches/exits; the user observed
  no Keychain prompt. This satisfies the saved-profile relaunch gate for the
  current dogfood kit. Re-check after the next replacement build.
- Source: `DF-002`.

## B. Agent permission and tab lifecycle

### 05. Tab defaults and LLM-started session

- Status: `PASS`
- Check: Create one tab by hand and ask the LLM to start a Saccade session while
  the app is already running.
- Pass: The human tab starts Agent Off. Saccade reuses the running process and
  creates one foreground Agent On tab without taking over the human tab.
- Current evidence: Packaged regression
  `runs/dogfood/df_r02_r03_tab_defaults_owner_case_final_20260716/report.json`
  passed.
  It launched a Human broker tab with no initial grant (`agent_enabled=false`,
  `browser_count=1`, no URL or tab identity exposed), then called MCP
  `saccade.tabs.open_agent` against the running broker. The MCP reported
  `browser_was_running=true`; the browser state became `agent_enabled=true`,
  `browser_count=2`, `tab_identity=cef:2` for the Agent tab. Closing the Agent
  tab recovered the Human tab as Agent Off (`agent_enabled=false`,
  `browser_count=1`) with no readable URL or tab identity, proving the Human tab
  was not taken over. The MCP JSON owner contract now emits lowercase
  `owner=agent`/`owner=human`, while owner input and grant-artifact validation
  accept either case.
- Source: `DF-R02`, `DF-R03`.

### 06. Current-tab truth

- Status: `PASS`
- Check: Focus one Agent On tab and ask the LLM what page is visible.
- Pass: Zero-argument attach returns that tab's redacted URL, title and revision.
  It returns no protected values, cookies, storage or capabilities.
- Current evidence: The installed dogfood returned the focused NaN Mesh tab.
- Source: `DF-R04`.

### 07. Multi-tab Agent On discovery

- Status: `PASS`
- Check: Leave two tabs Agent On and one tab Agent Off, then list eligible tabs.
- Pass: The LLM sees safe metadata for exactly the two On tabs, can attach by
  opaque tab ID and cannot discover the Off tab.
- Current evidence: DF-R06 source regression
  `runs/dogfood/df_r06_multi_tab_registry_final_20260716/report.json` passed.
  The test launched one Human Off broker tab, opened two dedicated Agent On tabs,
  and then called `saccade.tabs.list`. The live browser registry reported
  `browser_count=3`, `eligible_count=2`, two opaque `browser_tab_id` values
  (`cef:2`, `cef:3`) and `agent_off_tabs_omitted=true`. The registry entries
  exposed only safe metadata (`browser_tab_id`, owner, active state, title,
  origin and revision), with no full URL, capabilities, cookies or storage.
  `saccade.tabs.grant_current` then attached by opaque `browser_tab_id=cef:2`,
  proving Agent On tabs are selectable without making the Human Off tab
  discoverable.
- Source: `DF-005`, `DF-R06`.

### 08. Human permission versus Agent activity

- Status: `PASS`
- Check: On a human-enabled tab, run attach, idle, pause, disconnect and reconnect.
- Pass: Agent activity changes among disconnected, idle, working and paused while
  the human-controlled On/Off permission stays unchanged. The Agent cannot turn a
  human tab On.
- Current evidence: DF-R07 source regression
  `runs/dogfood/df_r07_permission_vs_activity_final_20260717/report.json`
  passed. The test launched a Human-owned tab with the browser-owned Agent switch
  already On, attached through MCP and preserved `owner=human`. Calling
  `saccade.tabs.pause_agent` changed runtime state to `agent_activity=paused`
  while `agent_enabled=true` and `agent_input_grant=true` stayed unchanged in the
  browser grant and `shell_status`. The live registry still listed the eligible
  On tab with `paused=true`, but `saccade.web.truth` was blocked by the paused
  runtime gate. Closing and restarting the MCP client, then calling
  `saccade.tabs.grant_current`, resumed the same Human-owned On tab to
  `agent_activity=idle` without requiring the Agent to toggle permission.
- Source: `DF-006`, `DF-R07`.

### 09. Agent Off hard gate and switch truth

- Status: `PASS`
- Check: Turn every tested tab Off and attempt list, attach, read and action calls.
- Pass: The broker returns no readable tab identity or page data. UI state,
  selected browser ID, owner-only pointer and broker state agree.
- Current evidence: With both tested tabs Off, the broker returned
  `agent_enabled=false`, `paused=true` and no URL or tab identity.
- Source: `DF-R08`.

## C. Reading, input and execution

### 10. Article and website review

- Status: `PASS`
- Check: Turn the current article or application tab On and ask for a description,
  usefulness review and available actions immediately after load.
- Pass: Saccade returns bounded redacted article/truth/action data before the
  readiness deadline, without requiring navigation away and back.
- Current evidence: Enabling Agent access now actively refreshes the renderer
  collector, and current-tab attachment waits for the collector readiness
  handshake before reporting success. The first bounded article read then passed
  without any intervening navigation; truth/actions remained revision-bound and
  protected values were absent from MCP output and replay.
- Source: `DF-R09`;
  `runs/dogfood/df_r08_article_attach_ready_final_20260717/report.json`.

### 11. Protected-value isolation and ordinary fill

- Status: `PASS`
- Check: Enter a protected test value through the local path, then ask the LLM to
  fill ordinary fields around it.
- Pass: Password, OTP, CVV, SSN and payment-card values never enter model context,
  logs, screenshots or replay. Passport and driver-document values require a
  user-confirmed local fill. Ordinary requested fields fill without overwriting
  existing values.
- Current evidence: The Agent requested only the named passport field and page
  revision. Saccade collected the test value in a browser-owned native prompt;
  MCP accepted and returned no value, while the LLM-visible receipt reported
  only `completed_without_value`. The existing SSN remained value-blind, the
  sensitive-page screenshot was blocked with no artifact, browser logs and
  replay contained no protected sentinel, and two ordinary fields filled with
  a verified receipt without overwriting the Human note. The existing CEF form
  safety suite also remained green.
- Source: `DF-R10`; `docs/PRODUCT_CONTRACT.md`;
  `runs/dogfood/df_r10_protected_local_fill_release_20260717/report.json`;
  `runs/dogfood/df_r10_form_safety_regression_20260717/report.json`.

### 12. Host-owned site-action policy

- Status: `PASS`
- Check: Let the LLM host apply its own policy to gameplay, navigation, rewards,
  progression and a purchase flow on an Agent On tab.
- Pass: Saccade adds no site-action confirmation. It still enforces Agent On,
  protected-value isolation, revision and target binding, input validity and
  receipts.
- Current evidence: The MCP contract and CEF adapter now identify the LLM host
  as the site-action policy owner and add no Saccade confirmation for purchase
  or submit actions. The local hostile-page gate dispatched both an ordinary
  action and a semantic submit through the browser input path, returned verified
  receipts, and still rejected a stale revision. Missing, wrong and cross-session
  capabilities remained blocked; protected-value and capability leaks stayed at
  zero. Agent Off and form-safety regressions also remained green.
- Source: `DF-008`, `DF-R09`, `DECISION_PRODUCT_071`.
  Evidence: `runs/dogfood/df_r12_host_policy_20260717/report.json`;
  `runs/dogfood/df_r12_r13_host_policy_navigation_20260717/report.json`.

### 13. Navigation, canonical actions and readiness

- Status: `PASS`
- Check: Exercise MCP Back, Forward and Reload; inspect a page with duplicate DOM
  representations; repeat immediate attachment across 20 dynamic-page runs.
- Pass: Advertised navigation works in the selected WebView, each logical target
  produces one stable action, and the collector meets its readiness deadline.
- Current evidence: CEF now advertises and implements Back, Forward and Reload
  on the selected browser. MCP waits for the post-navigation collector handshake
  before returning. A dynamic fixture exposed two visible DOM links for the same
  destination; the renderer published one canonical action whose ID survived a
  DOM subtree replacement. Navigate, Back, Forward and Reload all settled in the
  same WebView. Twenty consecutive dynamic-page navigations passed collector
  readiness and returned the same canonical action ID (`20/20`).
- Source: `DF-009`, `DF-010`, `DF-011`.
  Evidence:
  `runs/dogfood/df_r12_r13_host_policy_navigation_20260717/report.json`.

## D. Installation and release

### 14. Self-contained MCP installation

- Status: `PASS`
- Check: Install on a clean macOS user account and configure the documented MCP
  command without using the source repository.
- Pass: The LLM attaches to an Agent On tab. Replacing the app with the next build
  does not break the configured command.
- Current evidence: Build A was installed at `/Applications/Saccade.app` and
  exercised from `/private/tmp` with a new temporary HOME and minimal PATH. Its
  signed in-app MCP started the installed browser, opened an Agent On tab,
  attached to the same WebView, read the bundled article and closed the tab
  without a repository or external runtime. Build B then replaced the app;
  the unchanged command
  `/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp` repeated
  the full flow successfully. Automated clean-room and in-place upgrade gates
  are `PASS`. On 2026-07-18 a genuinely separate Standard macOS user with no
  copied repository or Saccade profile configured only the stable in-app MCP
  command and passed initialization, tool discovery, On/Off tab isolation,
  public reading, safety preflight and a verified six-field no-submit form plan.
  Build 47 then repeated the repo-free clean-HOME gate, including last-tab close
  followed by immediate LLM cold-start, without changing the configured command.
- Evidence:
  `runs/dogfood/df_r14_installed_build_a3_20260717/report.json` and
  `runs/dogfood/df_r14_installed_build_b_20260717/report.json`; external-user
  report dated 2026-07-18; build 47:
  `runs/dogfood/df_external_report_final_installed_build47_20260718/report.json`.
- Source: `DF-003`.

### 15. Signing, notarization and Gatekeeper

- Status: `PARTIAL`
- Check: Verify Developer ID signing, notarize, staple and run offline Gatekeeper
  assessment on the shipped DMG and app.
- Pass: A second Mac installs through normal double-click flow without a security
  workaround.
- Current evidence: Build 62 was rebuilt from a pristine pinned CEF archive and
  signed with Developer ID Team `48KK2UWXQM`, Hardened Runtime, secure timestamp
  and least-privilege JIT entitlements on the required Chromium helpers. The
  no-upload preflight verified every nested Mach-O signature, runtime flags,
  timestamps and absence of `get-task-allow`. The release script is ready to
  submit, staple and assess both App and DMG, but was intentionally not invoked
  during dogfood. Actual Apple notarization, stapling and offline Gatekeeper on a
  second clean Mac remain the only open macOS release-owner actions.
- Source: `DF-004`.

### 16. Public-release metadata and license decision

- Status: `PASS`
- Check: Inspect the app version, build number, copyright, release manifest,
  Saccade license decision and bundled CEF/Chromium credits.
- Pass: Saccade metadata is complete, the public-distribution license is chosen,
  and the package keeps the required CEF/Chromium licenses and version record.
- Current evidence: Saccade source and core runtime use Apache-2.0 to maximize
  adoption and independently reproducible comparison with Playwright. Apache
  section 6 and `TRADEMARKS.md` reserve the Saccade name, logo and official
  signed-release identity. Signed Build 62 identifies NaN Logic LLC and
  `nanlogic.com`; package and App both contain the Saccade license, NOTICE and
  trademark policy alongside the CEF BSD license and Chromium credits. The
  machine gate matched version/build, copyright, bundle/Team identity,
  deterministic CycloneDX 1.6 SBOM, portable checksums, Hardened Runtime and
  strict signing, with no license-decision placeholder.
- Source: `LICENSE`; `NOTICE`; `TRADEMARKS.md`;
  `docs/public_release_licensing.md`;
  `engines/cef/scripts/build_dogfood_release_macos.sh`;
  `runs/dogfood/df_build62_release_complete_20260718/report.json`;
  `runs/dogfood/df_build62_company_help_20260718/report.json`.

### 17. Browser and Agent downloads

- Status: `PASS`
- Check: Download a local fixture through the normal browser flow, then repeat
  through an Agent On tab using a verified page action and query the result.
- Pass: The file completes. MCP reports metadata-only progress/status for that
  tab without a full local path, file contents or auto-execute authority.
- Current evidence: Source and installed build 44 both downloaded
  `saccade-free-sound-license.txt`; the installed run used only the App-bundled
  MCP and fixture. Both reports recorded `full_path_exposed=false`,
  `file_contents_exposed=false` and `auto_execute_allowed=false`. Live Pixabay
  dogfood also completed `alexzavesa-calm-elegant-logo-519008.mp3` through the
  site's two-step `Free download` flow with the same metadata-only boundary.
- Evidence: `runs/dogfood/df_downloads_source_20260717/report.json` and
  `runs/dogfood/df_downloads_installed_20260717/report.json`; live site:
  `runs/dogfood/df_pixabay_live_20260717/report.json`.
- Source: `DF-013`, `BP-027`, `DECISION_PRODUCT_074`.

### 18. Cookie and site-data controls

- Status: `PASS`
- Check: Verify persistent normal-profile login state, disposable incognito
  state, value-free profile status, safe full-profile deletion, and the absence
  of raw Cookie/storage data from MCP output and replay.
- Pass: Normal mode preserves site sessions; incognito state disappears on
  exit; `clear-profile` refuses unsafe paths and running-browser deletion,
  reports no values, and signs the selected profile out; the installed release
  includes an accurate privacy/Cookie description. Site-specific controls are
  either verified through Chromium Settings or explicitly left unclaimed.
- Current evidence: Normal-profile restart, Chromium Safe Storage, incognito
  cleanup, and the Agent's no-Cookie/no-storage contract pass. Signed Build 56
  includes value-free status and bounded full-profile deletion both beside the
  release and inside `Saccade.app`; the packaged regression passed dry-run,
  confirmed deletion, invalid-name rejection, symlink rejection, and zero raw
  Cookie/storage values. The privacy document explicitly delegates
  site-specific controls to Chromium Settings and leaves that UI unclaimed.
- Source: `docs/privacy_and_cookie_model.md`;
  `docs/cef_macos_signing_keychain_report.md`;
  `runs/cef_day5/product_profile_github_restart_20260715/report.json`;
  `runs/dogfood/df_build56_profile_controls_20260718/report.json`;
  `runs/dogfood/df_build56_installed_cleanroom_retry_20260718/report.json`.

### 19. Resize and stale-layout invalidation

- Status: `PASS`
- Check: Read an action map, let the human resize the native Saccade window
  across a responsive breakpoint, then execute the pre-resize action without
  refreshing.
- Pass: A viewport, scroll, zoom, device-scale or element-layout change advances
  a browser-owned layout epoch before native input. The Agent path refreshes
  the action map just in time. It may rebase locally only when the same stable
  semantic action still exists and then must verify a native-input receipt;
  disappeared, covered or ambiguous targets fail closed without dispatch. DOM
  and Canvas-surface paths both pass without a screenshot or another LLM turn.
- Current evidence: Signed Build 57 passed the source and packaged native-window
  matrix. A responsive DOM action and a resized Canvas-surface action rebased
  locally and returned verified receipts; a desktop-only action that vanished
  at the breakpoint was rejected before input. The packaged run measured
  288.002 ms from the macOS resize command to observed layout invalidation,
  5.551 ms for DOM rebase plus receipt and 2.717 ms for Canvas rebase plus
  receipt. The original signed Build 56 SimpleMMO failure remains the real-site
  reproduction: its stale `View Updates` coordinate had returned `ok` without
  navigation or verification. Playwright 1.59.1 timed out safely for the hidden
  DOM locator but its old absolute-coordinate click also returned without error
  and did nothing.
- Evidence:
  `runs/dogfood/df_build56_resize_stale_action_simplemmo_20260718/report.json`;
  `runs/dogfood/df_build57_layout_epoch_source_20260718/report.json`;
  `runs/dogfood/df_build57_layout_epoch_packaged_20260718/report.json`;
  `runs/dogfood/df_build57_resize_live_simplemmo_20260718/report.json`.

## Inspection order

Run `01` through `19` in order. Fix a `FAIL` only after reproducing it and
recording the evidence. Re-run the affected item plus items `06`, `09` and `11`
after any permission, browser-lifecycle or input-path change.

The detailed defect history remains in `docs/dogfood_punch_list_20260716.md`.
