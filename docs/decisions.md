# Decisions

## M0 - Servo pin and local platform

- Pinned `servo` to `=0.2.0`, the latest crates.io release found by `cargo search servo --limit 5` on 2026-06-11.
- Pinned Rust to `1.88.0`. `servo 0.2.0` declares `rust-version = "1.86.0"`, but Cargo's current resolution pulled transitive packages (`image 0.25.10`, `time 0.3.47`, `sea-query-derive 1.0.0`, `built 0.8.1`) that reject Rust 1.86. Using Rust 1.88 keeps the Servo pin exact without transitive downgrade churn.
- M0 is being attempted on macOS arm64 (`Darwin ... RELEASE_ARM64_T8103`) even though the benchmark target remains Linux/X11 per the spec. Any platform-specific gap here is M0 evidence, not a benchmark decision.
- Matched workspace `euclid = "0.22"`, `image = "0.25"`, and `winit = "0.30.13"` to the `servo 0.2.0` crate metadata.
- Added a local `[patch.crates-io]` for `servo-fonts 0.2.0` on macOS because the published crate fails Rust 1.88 with `E0716` in `platform/macos/font.rs`. The patch is a one-line-lifetime workaround and should be removed if Servo/servo-fonts publishes a fixed pinned release or if the Linux/X11 benchmark build does not need it.
- M0 boot uses `WindowRenderingContext` rather than `SoftwareRenderingContext` because the M0 scope explicitly asks for a 1280x800 windowed WebView. `SoftwareRenderingContext` remains visible in the pinned API for later CI/headless exploration.

## M2 - Replay-safe core data model

- `MotorAction::Noop.reason` is stored as `String` rather than the sketch's `&'static str` so replay events can derive both `Serialize` and `Deserialize` and round-trip from JSONL. Hot-loop code can still pass static text with `.into()`.

## M3 - Synthetic detector timing

- Added a 16x16 block-gated prepass for `PixelDetector` after the first full-frame synthetic timing run exceeded the 3 ms M3 budget in debug tests. The prepass samples each block, expands to neighboring blocks, and still computes connected components/centroids at full resolution inside active regions.

## M4 - Calibration input pacing

- `mousemax calibrate` waits 300 ms before the first calibration click after resetting the page. Without this, Servo occasionally reports an empty hit-test result for the first synthetic input event even though subsequent clicks land correctly. The measured coordinate convention remains `InputSpace::CssLogical` with 0.000 px max error.

## M5 - Selftest page evidence mix

- `mousemax selftest-pages` now feeds `.target` DOM bounding boxes into the same `DetectionPipeline` used by pixel evidence. The canvas and WebGL fixtures still draw targets into canvas and validate clicks by page-side coordinates, but they expose a synchronized `.target` proxy so the zero-fork runner can verify the DOM/pixel fusion and motor path deterministically on Servo/macOS.
- The runner filters candidate clicks to `y >= 100` CSS px during selftests so HUD text such as `#truth` cannot be selected by foreground detection. The overlay page remains the negative control and passes only when no click is sent.
- Local M5 gate passed at DPR 1 with all 7 fixture pages: DOM, SVG, canvas arc, canvas sprite, overlay, high-DPI grid, and WebGL-style canvas. The explicit DPR 2 high-DPI check is still target-platform work for Linux/X11, where `WINIT_X11_SCALE_FACTOR=2` is meaningful.

## M6 - Arena runner scope

- The local arena uses the M1-observed Epic cadence (`306 ms`) with Tiny radius `7 CSS px`, deterministic xorshift RNG, and a canvas-rendered target layer plus synchronized `.target` DOM proxies for `observe_only` instrumentation. The page's own counters remain authoritative: hit/miss is determined from `mousedown` coordinates against active canvas targets.
- `run --site arena` writes replay JSONL from inside the Servo hot loop. M6 counts `targets_seen` only for tracker appearances inside the selftest game area (`y >= 100 CSS px`) so HUD foreground components do not inflate the benchmark denominator.
- M6 passed 5 consecutive local macOS windowed runs with command `cargo run -p mousemax -- run --site arena --spawn-speed epic --target-size tiny --duration 15 --seed 42 --replay`: `run_1781183354`, `run_1781183425`, `run_1781183453`, `run_1781183483`, `run_1781183511`. Results were 44-45 hits, 0 misses, hits == targets_seen, 0 stale/false-positive/unknown clicks, and replay p95 detect-to-dispatch between `0.200` and `0.250 ms`.
- Added `scripts/e2e_arena.sh` as the repo e2e entrypoint for one arena run plus replay summary. Full 5-run stability remains the M6 release gate command sequence rather than a default `cargo test`, because each run opens a Servo window for 15 seconds.

## M7 - Real site benchmark

- `run --site real` opens `https://mouseaccuracy.com/classic/`, selects the requested speed and target size, and uses the page's own score counters as the final hit/miss authority.
- The default runner window is now 1920x1080, with `--window-width` and `--window-height` available from the CLI. The run config records those dimensions in replay metadata.
- `observe_only` uses live `.target` rectangles from the page as browser-owned layout evidence. `instrumentation=none` disables DOM target data and uses rendered RGBA pixels with a red connected-component detector.
- M7 passed 5 consecutive real-site observe-only runs and one pure pixel run on macOS arm64. The pure pixel artifact is `runs/real/run_1781193985`: 47 hits, 0 misses, 47 targets seen, 47 clicks sent, p95 detect 6.3 ms, p95 first-visible-to-dispatch 16.0 ms.
- Each arena and real run now saves `before.png` and `after.png` in the run directory. These screenshots are artifact evidence, not detector inputs.
- Chrome/Safari visual parity is a separate launch concern. Current Servo rendering can differ from Chrome in layout, scaling, fonts, and page code paths, so public demos must not imply Chrome visual equivalence until the Chrome adapter or a visual parity layer proves it.

## M8 - Replay and artifact polish

- `mousemax replay --render-summary <png>` renders a replay-derived click map from click receipts and verification outcomes. The renderer does not claim to be a captured browser frame.
- `mousemax validate-run <run_dir>` checks a result bundle against the MOUSEMAX acceptance invariants and cross-checks replay counters against `result.json`. `--require-click-map` also requires the M8 visualization artifact.
- `mousemax serve` adds the M8 HTTP shell: `/bench/mouseaccuracy/start`, `/bench/mouseaccuracy/status`, and `/bench/mouseaccuracy/result`. The server starts runs through a child `mousemax run` process so Servo's event loop stays isolated from the HTTP loop.

## M9 - Release validation

- `scripts/validate_m9_release.sh` packages the known M7 artifact checks into one command. It validates code compilation, replay summary regeneration, click-map rendering, artifact presence, and `validate-run`.

## M10 - FORMMAX fixture

- Added a local FORMMAX capacity fixture under `test_pages/formmax/`. It covers two-page long tables, lazy row rendering, mixed field types, receipt JSON, and confirmation-gated sensitive fields.
- `scripts/formmax_fixture_smoke.js` validates the deterministic fixture oracle and writes `runs/formmax/fixture_smoke/result.json`.
- The fixture now starts with empty ordinary fields and produces receipts from submitted DOM state. The smoke oracle rejects a blank submitted state, so FORMMAX cannot pass merely because the fixture knows the expected rows.

## N4 - FORMMAX Servo runner

- Added `formmax run --fixture ... --replay` as the local practical form workflow gate.
- The runner opens the fixture in Servo, scrolls both lazy-rendered pages, fills 672 non-sensitive fields, blocks the three sensitive fields as `requires_user_input`, submits the local fixture, verifies the receipt, and writes replay JSONL.
- The runner native-types one real text field (`CAP-001.site_name`) before the full fixture transaction, then drives the remaining trusted fixture DOM controls from the Servo page context. It proves rendered-page state, transaction replay, scroll/page coverage, receipt validation, sensitive policy, and a small native input bridge.
- Local verification passed with native typing. Current evidence run `runs/formmax/run_1781266239027/` has 2712 replay events, `before.png`, `after.png`, `native_input_verified=1`, and no table-value echo in replay.
- Added `formmax validate-run <run_dir>` to re-check result/replay artifacts, required event counts, sensitive field blocking, receipt validation, and replay value-leak policy.

## N4A - Servo native input probe

- Added `test_pages/native_input/` and `saccade-shell selftest-native-input`.
- The selftest measures a real `<input>` rect, clicks its center through Saccade's native mouse path, types `saccade42` with `InputEvent::Keyboard`, then verifies focus, DOM value, keyboard/input event counts, and zero keyboard dispatch failures.
- The same selftest now measures and clicks a real `<select>`. Servo raises `EmbedderControl::SelectElement`; Saccade selects option index `2`, submits it back to Servo, and verifies `select_value=gamma`, `input=1`, and `change=1`.
- Pinned Servo `0.2.0` emits `keydown`, `keypress`, `input`, and `keyup` for this path, but not `beforeinput`. `InputEventResult::Consumed` stays false despite successful DOM input, so verification should rely on DOM state, replay evidence, and dispatch-failure checks.
- The gate passed three consecutive local runs. This proves native keyboard text entry and native select/dropdown handoff are available; FORMMAX now uses text entry for one real text field and still needs select/dropdown integration for the `owner` field.

## M11 - PDF and sensitive gate feasibility

- `scripts/formmax_pdf_feasibility.py` generates a fillable AcroForm PDF and a flat PDF, fills only non-sensitive fields, and verifies tax ID, signature, and legal attestation fields stay gated.
- Browser-surface PDF filling remains reported as unsupported in the current harness; programmatic AcroForm filling is the viable first path.

## N1 - Trusted Tabs runtime

- Imported `SACCADE_NEXT_PLAN_v5.md` and froze MOUSEMAX as evidence. New product work starts with Trusted Tabs and DEVMAX rather than more mouse-game features.
- Added `saccade-shell` as a new binary. Servo calls remain inside `saccade_browser`; the shell binary calls exported browser-boundary functions.
- `selftest-tabs` creates two WebViews under one Servo instance. On macOS arm64 with pinned Servo `0.2.0`, same-origin cookies and localStorage are shared between the Human and Agent WebViews.
- Added core tab policy types: `TabId`, `TabOwner`, `ReadGrant`, `TabInfo`, and `TabVisualMarker`.

## Dogfood policy

- Saccade should become the default browser layer for local development inspection, form workflow tests, login handoff, and replay-backed actions. Chrome and Playwright remain compatibility baselines and escape hatches.
- Added `saccade-shell browse --url ...` as the first human-facing dogfood browser shell on macOS. It supports one Servo WebView with mouse click, wheel scroll, keyboard text entry, basic `<select>` handoff, reload, back, and forward. This is intentionally a dogfood shell, not a packaged Chrome-parity browser UI yet.
- Added `docs/site_policy_matrix.md` as the first product boundary for third-party sites. Saccade should run by default on local/owned/public low-risk pages, assist with human-in-loop on ordinary logged-in work, and fall back for high-risk auth, payment, legal, government identity, app release, security, and anti-automation blocks. The response to an explicit site block is evidence plus fallback, not stealth or bypass.
- Implemented the first shared `saccade_core` site/action policy classifier. MCP and the official ServoShell bridge now return `site_policy`; Red sites block agent truth/inspect/actions, and high-risk actions such as login, OTP, CAPTCHA, payment, release, submit, delete, signing, credentials, and security changes require the user.
- Added redacted block reports for official ServoShell bridge control errors. The bridge writes `control/block_report.json` without screenshots or full page dumps, strips URL query/fragment data, extracts visible request IDs when present, and points the user to the safe fallback path.
- Added `saccade.report.redacted_note` as the first safe copy/paste fallback path. It accepts user-supplied redacted text, strips obvious emails/long numbers/URL query fragments, writes an AI review prompt artifact, and keeps the agent away from the live high-risk site.
- Added `SACCADE_OWNED_DOMAINS` as a first-party dogfood allowlist for normal owned sites. The classifier reports these as `owned_domain` Green only after high-risk auth, government, financial, healthcare, cloud, shopping, and social classes have had priority, so it is not an anti-abuse bypass.
- Added a paste-ready handoff prompt for other Codex sessions in `docs/SACCADE_DOGFOOD_HANDOFF.md`, so web/game/product sessions know when to use Saccade, when to compare with Chrome, and when to route through the redacted fallback packet.
- Added `scripts/create_redacted_note_packet.js` so high-risk site fallback is easy to dogfood from the command line. It calls the existing MCP redacted-note tool, strips URL query/fragment data, and writes the same local AI review packet without live-site access.
- Added the browser compatibility metrics gate for font/control and large viewport validation. `font_control_metrics` shows explicitly sized controls can match Chrome element rects at `1280x760`, while text range rects are not reliable in Servo yet. The same gate refuses invalid large-width comparisons, reproducing `1600x760` requested as actual Saccade `1440x760` on this macOS session.

## N1B - Login handoff

- Added `selftest-login-handoff` as the local gate for explicit human-to-agent login transfer.
- The fixture now has an OTP field and a dashboard `Done` control. The selftest drives the Human tab through login, observes Done, then lets the Agent tab continue from the shared session.
- The gate requires `human_login=true`, `agent_session=true`, `password_exposed=false`, `otp_exposed=false`, and `agent_input_to_human_tab_blocked=true`.
- This proves the minimal local contract only. Product UI, replay metadata, and arbitrary third-party sensitive-field masking remain later work.

## N5 - Safety truth priority

- Human/Agent truth boundaries are promoted ahead of FORMMAX and MCP because the product is unsafe without them.
- Safety v1 rule: the user can see all page state, including values filled by the agent; the agent receives mediated truth and can see agent-filled values but not human-owned sensitive values such as SSN, government ID, credit card, or password.
- Added `saccade-shell selftest-safety`, backed by `test_pages/login_handoff/safety.html`.
- The local gate passed with `human_can_see_agent_values=true`, `agent_can_see_agent_values=true`, `masked_sensitive_fields=5`, `completed_without_value=4`, `requires_user_input=1`, and sensitive exposures all false.
- The safety UX should not be confirmation-heavy. Agent fills non-sensitive fields, user handles sensitive fields in the real browser, and agent sees status such as `completed_without_value` or `requires_user_input` rather than raw values.
- Chrome/Firefox visual parity is also promoted because UI design review loses credibility if Saccade renders materially different output from mainstream browsers.

## N6 - Chrome visual parity v0

- Added `scripts/capture_chrome_reference.sh` as the first Chrome reference artifact path.
- This uses an installed Chrome-family browser for page-content screenshots at a fixed viewport.
- Upgraded the path to Chrome CDP reference capture: it now writes screenshot, redacted truth/action map, network summary, and a manifest.
- Added a default balanced CDP block policy for common ad/analytics hosts. The blocking fixture proves three ad/analytics requests are blocked without counting them as DEVMAX page findings.
- Added `devmax audit --engine chrome` and `saccade.dev.audit_page(engine=chrome)` for local/file URLs.
- Added `scripts/visual_parity_compare.py` and six local fixtures to compare Chrome screenshots against Saccade live-worker screenshots. The latest full run passed and exposed real layout gaps while preserving matching action counts.
- The browser session worker now waits briefly after load completion and retries nearly-white screenshot readbacks, because the parity runner exposed blank screenshots on complex pages even when DOM truth was valid.
- This is not yet the final Chrome adapter; it does not include browser chrome, real user-profile session reuse, or Firefox capture. Later rendering work added non-mutating Chrome hit-test verification for Saccade action points.

## N2 - DEVMAX local self-test

- Added `devmax` as the first local development-audit binary. The initial gate is `cargo run -q -p devmax -- selftest-fixtures`.
- Added 16 deterministic local fixtures for common agent-built UI failures: blank page, console error, hydration error, missing asset, invisible text, overlap, offscreen action, no handler, broken validation, lazy route error, hidden submit, mobile break, blocking modal, blank canvas, z-index overlay, and wrong success state.
- The first engine is explicitly `static-fixture-v0`. It validates the CLI, report schema, fixture corpus, and replay artifact shape. It is not yet a Servo rendered-truth audit.
- The N2 gate passed with `total=16 detected=16 false_positives=0`.
- Expanded the static fixture corpus to the gauntlet minimum of 20 by adding stuck loading, disabled primary action, duplicate IDs, and wrong-route 404 cases.
- Latest static gate passed with `total=20 detected=20 false_positives=0`.

## N2B - DEVMAX Servo rendered probe

- Added `saccade_browser::devmax_probe` as the first DEVMAX browser-truth boundary. It opens a URL in Servo and returns compact JSON for title, URL, viewport, body content, interactive element rectangles, offscreen controls, computed text colors, and overlay blockers.
- Added `devmax audit --engine servo --url <http://...> --replay`.
- Added `devmax selftest-servo-fixtures`; because pinned Servo/winit cannot recreate an event loop in one process, this selftest runs each Servo audit in a child process.
- The Servo probe gate passed with `total=4 detected=4 false_positives=0` for blank page, invisible text, offscreen button, and modal-blocked action.

## N2C - DEVMAX screenshot pixel checks

- Extended `devmax_probe` to read back the Servo window with `RenderingContext::read_to_image` after browser truth collection.
- The probe now combines browser-reported canvas rectangles with screenshot RGBA sampling to detect blank canvas regions.
- `devmax selftest-servo-fixtures` now includes `canvas_chart_blank` and passed with `total=5 detected=5 false_positives=0`.
- Browser console and network capture remain pending; the current Servo probe does not claim those yet.

## N2D - DEVMAX click verification

- Extended `devmax_probe` to dispatch one real Servo mouse click against the first visible, enabled, unblocked action and then collect a second browser truth probe.
- The report now includes `clickVerification` evidence comparing before/after body text, URL, and body child count.
- `button_no_handler` is now covered by the Servo gate: a clickable `Save` button with no visible post-click effect is reported as `button_no_handler`.
- `devmax selftest-servo-fixtures` passed with `total=6 detected=6 false_positives=0`.

## N2E - DEVMAX console and resource capture

- Verified pinned Servo exposes `WebViewDelegate::show_console_message` and `WebViewDelegate::load_web_resource`.
- `devmax_probe` now records console messages and resource-load request metadata into `runtime`.
- `console_error` is now covered by the Servo gate from a real delegate console error.
- `missing_asset` is now covered by the Servo gate from a non-main-frame image resource request. This captures request metadata, not final HTTP status code yet.
- `devmax selftest-servo-fixtures` passed with `total=8 detected=8 false_positives=0`.

## N3 - MCP skeleton

- Added `saccade-mcp` as the first agent-facing tool surface.
- The skeleton registers `saccade.dev.*`, `saccade.tabs.*`, `saccade.web.*`, and `saccade.report.*` tool names with compact JSON/artifact-path return policy.
- `saccade-mcp selftest` verifies tool count, tab scoping, loopback-only local dev audit acceptance, and sensitive-field policy gating.
- Added `saccade-mcp serve-stdio` as a minimal MCP-style JSON-RPC stdio server. It handles `initialize`, `tools/list`, and `tools/call`.
- Implemented tool calls for `saccade.dev.open_local`, `saccade.dev.audit_page`, and `saccade.tabs.list`. `saccade.dev.audit_page` validates loopback/file URLs, spawns DEVMAX audit, and returns compact counts plus report/replay artifact paths.
- Added in-memory persistent tab state for the stdio server plus v0 implementations for `saccade.tabs.open`, `request_user_login`, `takeover`, `pause_agent`, `close`, `saccade.web.truth`, `saccade.web.actions`, and `saccade.web.act`.
- `saccade.web.truth/actions` use DEVMAX report state as compact browser truth/action-map evidence. `saccade.web.act` v0 requires an Agent-owned local tab, a fresh page revision, and the first enabled action in the current action map, then runs a Servo-backed DEVMAX verification pass.
- Added report tools: `saccade.dev.get_report`, `saccade.report.validate_run`, and `saccade.report.replay_summary`. `validate_run` can route FORMMAX run directories to the FORMMAX validator; `replay_summary` returns event counts without replay contents.
- Added `saccade.web.fill_form` v0 for the local FORMMAX fixture. It runs the FORMMAX Servo runner, blocks sensitive fields, validates the run, and returns result/replay/screenshot artifact paths. Evidence run: `runs/formmax/run_1781270729709`.
- Added v0 handlers for the remaining MCP tools: `saccade.dev.click_all_primary_actions` and `saccade.dev.fill_smoke_form`. All 17 registered tools now have handlers; the selftest verifies the full set, with local-only and max-one-action limits where appropriate.
- MCP tools that generate DEVMAX/FORMMAX evidence now append compact artifact records to `runs/mcp/artifacts.jsonl`.
- The next step is replacing in-memory/report-backed tabs with browser-backed tab sessions.

## N3B - Browser session smoke

- Added `saccade-shell selftest-browser-session` as the first browser-backed session gate.
- The gate uses one Servo WebView path to open a local fixture, collect truth/actions, dispatch a native mouse click, and collect post-action truth.
- The local fixture advances `data-session-revision` from `0` to `1`; the report/replay gate requires the revision to advance and the click verification to be non-no-op.
- Added `saccade-shell browser-session-worker --url ...` as a long-lived JSONL worker. MCP Agent-owned local tabs now spawn one worker process per tab and route `truth`, `actions`, and `act` to the live Servo WebView.
- Human takeover closes the worker before changing ownership, so the agent does not retain an input channel after handoff.
- The worker now writes `report.json` and `replay.jsonl` under `runs/browser_session_worker/worker_*/`.
- Worker truth/action maps redact form values before they leave the browser process. Sensitive fields expose kind and completion status, not raw values. Local redaction fixture passed with SSN, credit-card, password, and ordinary name values absent from stdout/report/replay.
- Worker screenshots are now policy-gated: non-sensitive pages save PNG artifacts, while pages with sensitive fields skip screenshot capture and log `screenshot_skipped_sensitive_fields`.
- Added `saccade.report.validate_run` kind `browser_session_worker` to validate worker report/replay, screenshot references, and replay raw-value leak checks.
- Added a live `audit` method to the browser worker. `saccade.dev.audit_page(engine=servo)` now uses the live Agent tab when present, producing compact findings and worker artifacts from the same WebView instead of spawning a separate DEVMAX audit.
- This is still v0: one child process per Agent tab, no shared browser process yet, and static audit fallback, click-all verification, and FORMMAX tools still run as child workflows.

## N6B - Servo Grid parity probe

- Added a dedicated `layout_probe` visual fixture and layout-probe truth fields for Chrome/Saccade comparison.
- Added `--saccade-grid on|off|default` to `scripts/visual_parity_compare.py`. The Saccade worker reads `SACCADE_SERVO_GRID=1` and enables pinned Servo's `Preferences::layout_grid_enabled`.
- Measurement resolved the "Saccade looks mobile" dashboard issue: viewport stayed `1280x800`; with Grid off, CSS Grid fell back to block flow; with Grid on, computed Grid styles matched Chrome.
- Evidence runs:
  - Grid off: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290226025/index.html`
  - Grid on focused: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290279853/index.html`
  - Grid on full gauntlet: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781290368953/index.html`
- Grid on reduced dashboard diff from `0.172743` to `0.031496` and reduced modal overlay diff from `0.163102` to `0.024039`. It does not claim full Chrome parity; remaining work is font metrics, canvas/SVG, sticky/scroll, media-query coverage, DPR/window chrome, and Chrome/Firefox reference modes.

## DECISION_RENDERING_001 - Rendering profiles, not Servo/Chrome parity claims

- Saccade will not promise Servo/Chrome pixel parity.
- Added explicit rendering profiles:
  - `servo-safe`: pinned Servo defaults and baseline regression control.
  - `servo-modern`: pinned Servo plus measured experimental prefs; currently `layout.grid.enabled`.
  - `chrome-reference`: Chrome-rendered visual reference path for UI parity and public demos.
- `SACCADE_SERVO_GRID=1` remains a legacy override, but profile selection is now the preferred interface.
- `saccade-shell browse` and `saccade-shell browser-session-worker` can opt into profiles with `--rendering-profile`.
- `chrome-reference` in the live worker path is a structured stub until the Chrome adapter exists; it reports `renderer_crash` with `fallback_recommended=chrome-reference`.
- Focused gate: `scripts/validate_rendering_profiles.sh`.

## DECISION_RENDERING_002 - Servo-modern Grid regression gates passed

- Added `--rendering-profile` to `mousemax run` and `formmax run`.
- `mousemax` arena replay metadata now records the resolved rendering profile and Servo Grid state.
- `formmax` result JSON now records the resolved rendering profile and Servo Grid state.
- R2 MOUSEMAX gate passed with `servo-modern`:
  - Command: `RUST_LOG=error cargo run -q -p mousemax -- run --site arena --spawn-speed Epic --target-size Tiny --duration 15 --seed 42 --replay --rendering-profile servo-modern`
  - Result: `/Users/waynema/Documents/GitHub/SACCADE/runs/arena/run_1781294025/result.json`
  - Summary: `PASS`, `hits=45`, `misses=0`, `targets_seen=45`, `false_positive_clicks=0`, `stale_clicks=0`, `detect_to_dispatch.p95=0.2ms`.
- R2 FORMMAX gate passed with `servo-modern`:
  - Command: `RUST_LOG=error cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay --rendering-profile servo-modern`
  - Result: `/Users/waynema/Documents/GitHub/SACCADE/runs/formmax/run_1781294062952/result.json`
  - Validation: `cargo run -q -p formmax -- validate-run runs/formmax/run_1781294062952`
  - Summary: `rows=96`, `pages=2`, `filled=672`, `native_typed=1`, `blocked_sensitive=3`, `receipt_verified=true`, `validation_errors=0`, `replay_value_leaks=0`.
- These gates prove enabling Grid in `servo-modern` did not break the current MOUSEMAX arena or local FORMMAX workflow. Defaulting dogfood to `servo-modern` remains a separate product decision.

## DECISION_RENDERING_003 - Dogfood/session default is servo-modern

- After the focused rendering profile gate and the MOUSEMAX/FORMMAX gates passed, dogfood and browser-session workers now default to `servo-modern`.
- `servo-safe` remains available as an explicit baseline via `--rendering-profile servo-safe`.
- MOUSEMAX/FORMMAX runner defaults remain conservative; they use `servo-safe` unless a profile is explicitly requested or `SACCADE_RENDERING_PROFILE` is set.
- `scripts/validate_rendering_profiles.sh` now verifies the default browser-session worker profile and prints `default_worker_profile=servo-modern`.

## DECISION_RENDERING_004 - Visual parity classifier separates action safety from pixel parity

- Added a first-pass diff classifier to `scripts/visual_parity_compare.py`.
- Verdicts are intentionally product-facing:
  - `PASS_ACTION_GREEN`: action map and layout are acceptable for agent action.
  - `PASS_ACTION_YELLOW_VISUAL`: action is acceptable, but Chrome is still required for polished visual review.
  - `PASS_ACTION_YELLOW_RASTER`: action is acceptable, but Chrome is required for raster/canvas/pixel judgement.
  - `FAIL_LAYOUT`: layout differs enough to threaten coordinates.
  - `FAIL_ACTION_MAP`: viewport or action map differs enough to block agent action.
- The classifier compares action count, labels, Saccade click-point escape distance against the Chrome reference rect, action rect geometry, layout probes, screenshot dimensions, and raster/text diff ratios.
- Latest full `servo-modern` evidence: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781300179891/index.html`.
- Result: no red verdicts across the current seven local visual fixtures. `form_controls` stays yellow because Servo reports narrower native control rects, but the Saccade click points remain within tolerance.
- This is a routing gate, not a pixel-parity claim: public demos and UI design review still route to `chrome-reference`.

## DECISION_RENDERING_005 - Chrome hit-test verifies Saccade action coordinates

- Chrome reference capture can now accept `--verify-actions-file` and write `chrome_click_verification.json`.
- The verifier checks enabled non-sensitive Saccade actions only. Disabled, blocked, or sensitive actions are skipped because the agent should not click them.
- Verification is non-mutating: Chrome uses `document.elementFromPoint` and label/control association rules to confirm the Saccade click point would hit the same actionable target. It does not dispatch real click events.
- Latest full `servo-modern` evidence: `/Users/waynema/Documents/GitHub/SACCADE/runs/visual_parity/parity_1781300179891/index.html`.
- Result: all verifiable Saccade action points hit the expected Chrome targets across the seven local visual fixtures. `modal_overlay` verifies `2/2` and skips `4` correctly blocked page-level actions.
- A Chrome hit-test failure is now an `FAIL_ACTION_MAP` classifier input.

## DECISION_RENDERING_006 - Browser-frame previews are labeled report artifacts

- Visual parity HTML reports now include browser-frame previews around the Chrome and Saccade page-content screenshots.
- These previews expose URL context and make public/demo review easier for non-engineering readers.
- They are explicitly labeled report wrappers, not native Chrome/Saccade browser UI screenshots.
- Native browser UI capture with real Chrome/Safari window chrome remains pending for public launch polish.

## DECISION_DEMO_001 - Demo comparison pack records native browser UI capture status

- Added `scripts/capture_native_browser_ui.py` for macOS native Chrome/Safari window capture attempts through AppleScript plus `screencapture`.
- Added `scripts/build_demo_comparison_pack.py` to combine native browser UI capture attempts, Saccade visual parity evidence, and Chrome hit-test summaries into `demo_review.html`.
- `demo_review.html` now embeds the Saccade worker screenshot directly next to Chrome page-content and pixel-diff thumbnails, so reviewers do not have to open the nested visual parity report to find Saccade evidence.
- Native browser UI screenshots are public-demo artifacts only; browser truth, safety policy, replay, and hit-test verification remain separate evidence.
- Latest pack: `/Users/waynema/Documents/GitHub/SACCADE/runs/demo_pack/demo_1781306995672/demo_review.html`.
- Current result captures real Chrome and Safari native browser UI screenshots, records Firefox as unavailable on this machine, and embeds Saccade worker screenshots for all seven local visual fixtures.
- The seven-fixture pack verifies Chrome hit-test 35/35, skips four blocked modal actions correctly, and has no red action-map verdicts. `canvas_svg` remains `PASS_ACTION_YELLOW_RASTER`, which routes raster/canvas judgement to Chrome.
- The pack serves the default native-capture fixture over `127.0.0.1` to avoid Safari's `file://` load confirmation dialog.

## DECISION_DEMO_002 - MOUSEMAX public parity references are browser-visible

- `scripts/prepare_mousemax_parity_pack.sh` now tracks Chrome, Safari, and Firefox URL-bar reference slots.
- `runs/real/run_1781193985/parity_review.html` now includes captured Chrome and Safari URL-bar screenshots for `https://mouseaccuracy.com/classic/`.
- Firefox is recorded as pending because Firefox is not installed on this machine. The native capture script accepts `--browser firefox` and writes `capture_unavailable` evidence when unavailable.
- The MOUSEMAX validator still passes for the pure-pixel run: 47 hits, 0 misses, 47 targets seen, 47 clicks sent, `instrumentation=none`.

## DECISION_USER_FLOW_001 - Full local user workflow gate

- Added `saccade-shell selftest-user-flow` and `test_pages/login_handoff/user_flow.html`.
- The gate proves the product flow Wayne identified: Human login, explicit handoff, Agent-owned tab continuation, agent fills normal fields, user sees agent-filled values, sensitive fields stay unfilled/unread by the agent, user changes page and fills part, agent fills the remaining normal fields, and agent checks user input without receiving raw sensitive values.
- Current result passes with `round1_agent_filled=4`, `round1_requires_user_input=3`, `user_page_change_seen=true`, `user_normal_checked=true`, `sensitive_status_checked_without_value=true`, `agent_completed_remaining=2`, `preserved_user_values=true`, `same_agent_tab_continued=true`, `final_sensitive_completed_without_value=4`, and `sensitive_values_exposed=false`.

## DECISION_USER_FLOW_002 - Manual worker supports safe user/agent co-presence

- The live `browser-session-worker` now forwards real mouse, wheel, keyboard, browser shortcut, and select-control input into the visible Servo WebView so Wayne can directly inspect, edit, and navigate during dogfood.
- Added `fill_agent_fields` to the worker JSONL protocol. It writes only fields that are both `data-owner="agent"` and `data-sensitive="none"`, then recomputes sensitivity in the page before writing.
- The worker rejects human-owned or sensitive fields even when the caller explicitly requests values for them. Rejections expose only field ID, owner, and sensitivity kind.
- Replay for fill events records field IDs and rejection metadata only; it sets `values_logged=false`.
- Probe result: `task-1` and `task-2` filled, `ssn` and `tax-id-empty` rejected, `sensitive_fields_seen=3`, screenshots skipped by sensitive-field policy.

## DECISION_USER_FLOW_003 - Explicit inspect allows non-sensitive user-value checks

- Added `inspect_fields` to the worker JSONL protocol for explicit, named field checks.
- The worker returns values only when the target field is recomputed as non-sensitive and declared `data-sensitive="none"`, regardless of whether the field owner is Human or Agent.
- Sensitive fields return `completion_state` and `value_redacted=true`, never raw values.
- Replay records inspected field IDs, value-returned/redacted counts, and `values_logged=false`; it does not log returned values.
- Probe result: `agent-page2-code` and `agent-page2-owner` returned values, `user-quantity` returned a non-sensitive value, while `signature` and `tax-id-empty` returned redacted status only.

## DECISION_USER_FLOW_004 - MCP exposes live safe fill and inspect

- Added first-class MCP tools `saccade.web.fill_agent_fields` and `saccade.web.inspect_fields`.
- `fill_agent_fields` requires an Agent-owned live worker tab and a fresh `basis_page_revision`; it rejects stale fill attempts before reaching the worker.
- `inspect_fields` requires explicit field IDs and uses live worker redaction, so sensitive fields expose status only.
- MCP selftest now opens the user-flow fixture, fills `task-1`, rejects `ssn`, inspects `task-1`, and verifies `ssn` remains redacted through the MCP surface.
- Latest evidence: `MCP PASS tools_registered=19` with report `/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781363828594/report.json`.

## DECISION_BROWSER_001 - Worker viewport tracks logical window size

- Real-site GitHub Gist dogfood exposed a browser-alignment issue: the visible window could be enlarged while the page layout viewport, DOM rects, and action map still behaved as the original `1280x800` viewport.
- Root cause was in Saccade's adapter, not a Servo version blocker. The worker resized the shared `WindowRenderingContext` before calling `WebView::resize`, so Servo's pinned resize path saw the target size already applied and returned before sending layout viewport updates.
- Worker and dogfood windows now treat configured width/height as logical/CSS pixels, construct the rendering context from the actual physical `window.inner_size()`, and set `hidpi_scale_factor` from `window.scale_factor()` instead of hardcoding `1.0`.
- Runtime resize now calls `webview.set_hidpi_scale_factor(...)` and `webview.resize(...)` without pre-resizing the rendering context, so Servo owns render-surface and page-viewport synchronization.
- Verification on `https://example.com/` with `browser-session-worker --width 1280 --height 800`: startup runtime geometry was `2560x1518`, HiDPI `2.0`, JS viewport `1280x759`; macOS resize expanded it to runtime `2720x1518` and JS viewport `1360x759`; shrinking to `1000x700` produced runtime `2000x1336` and JS viewport `1000x668`.
- `cargo check -q -p saccade-shell`, `selftest-focused-type`, and `selftest-browser-session` passed after the fix.

## DECISION_BROWSER_002 - Dogfood shell adds keyboard URL entry

- `saccade-shell browse` now supports a keyboard address command with `Cmd+L`.
- The command is displayed in the native title bar, so it does not inject DOM, resize the page, or squeeze third-party layouts.
- Address entry swallows keyboard events while active. Enter opens the parsed URL, Esc cancels, bare domains default to `https://`, and local addresses such as `localhost:3000` default to `http://`.
- This improves daily dogfood but does not close the final browser chrome gap. Clickable URL/back/forward/reload/stop controls remain BP-003 work.

## DECISION_BROWSER_003 - Editor probes distinguish authoring from search/login controls

- Added `saccade-shell inspect-editors` so real-site editor routability can be checked through the live `browser-session-worker` without typing, publishing, or printing editor values.
- The command supports `--profile-dir`, making it the standard BP-004 retest path after human login inside Saccade.
- Editor routing now counts visible authoring editors separately from generic writable controls. A lone search box routes as `route_login_or_non_authoring_page`, not `usable_visible_editors`.
- Shared-profile Gist probe currently reaches only `Search Gists`, so authenticated Gist editor validation is blocked on Wayne logging in inside the Saccade profile.

## DECISION_BROWSER_004 - WebGL is promoted to a P1 dogfood blocker

- Local game dogfood at `http://127.0.0.1:4173/` reproduced the macOS GL warning `GLD_TEXTURE_INDEX_2D is unloadable` and Saccade missed the gameplay canvas layer that Chrome rendered.
- Added `test_pages/webgl_runtime/index.html` as a minimal repeatable fixture with 2D canvas, WebGL texture drawing, `readPixels`, and visible frame timing status.
- The minimal fixture reproduced a related `GLD_TEXTURE_INDEX_RECTANGLE is unloadable` warning. It showed 2D canvas OK and simple WebGL texture OK, but slow frame progress.
- BP-011 is now P1: route canvas/WebGL-heavy judgement to Chrome/reference until a scripted Saccade runtime selftest is green and the adapter/backend root cause is known.

## DECISION_BROWSER_005 - Minimal WebGL fixture has a scripted green baseline

- Added worker method `webgl_runtime_probe` to read `window.__saccadeWebglRuntime` without scraping visible text.
- Added `saccade-shell selftest-webgl-runtime`; it opens the minimal fixture, waits for frames, captures runtime status, runs an audit for screenshot/replay artifacts, and checks for GL texture warnings in worker output.
- Latest minimal fixture result is green: `canvas2d=ok`, `webgl_context=ok`, `texture=ok`, `read_pixels=ok_132_204_22`, `frames=30`, `avg_frame_ms=18.38`, `last_error=none`, `gl_warning=false`.
- Therefore BP-011 is now narrowed: simple WebGL is healthy under the scripted gate, while the live game still fails. Next isolate game/composition-specific behavior.

## DECISION_BROWSER_006 - Live WebGL game has a scripted red gate

- Added `scripts/probe_webgl_game_runtime.py` to capture the same local game in Saccade and Chrome, compare gameplay-layer pixels, record GL texture warnings, and write a machine-readable report.
- Added shared `scripts/webgl_page_probe.js` and worker method `webgl_page_probe` so the gate now records canvas count, CSS rects, backing sizes, DPR, visible layers, and context type without reading form values.
- Latest live-game probe on `http://127.0.0.1:4173/` routes `blocked_missing_gameplay_layer` after CSS viewport normalization: Chrome `edge_ratio=0.036279`, Saccade `edge_ratio=0.000754`, `gl_warning=True`, diagnosis `render_pipeline_after_dom_ready`.
- Both engines report one visible `canvas#game`; the current game canvas reports `context_type=none_or_2d`, so the next reductions should target Canvas2D/compositor/HiDPI texture behavior as well as WebGL.
- The probe artifacts live at `runs/webgl_runtime/game_probe_1781449261494/`.
- BP-011 is now a repeatable live-game gate: keep using Chrome/reference for canvas/WebGL-heavy dogfood until reductions identify and fix the complex game/composition trigger.

## DECISION_BROWSER_007 - Full-window Canvas2D is the minimal red gate

- Added `test_pages/canvas_runtime/index.html` and `scripts/probe_canvas_reductions.py` to run local Canvas2D reductions through the same Chrome-vs-Saccade pixel gate.
- Latest reduction run: `CANVAS_REDUCTIONS variants=4 blocked=4 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781450500515/report.json`.
- Static full-window Canvas2D is enough to reproduce the missing-layer failure: Chrome `edge_ratio=0.052731`, Saccade `edge_ratio=0.0`, Chrome `saturated_ratio=0.005621`, Saccade `saturated_ratio=0.0`.
- DPR backing scale, animation timing, and DOM HUD overlay are not required triggers; all variants are red.
- The reductions did not emit GL texture warnings, so the warning is not required for the Canvas2D captured-layer failure.
- BP-011 should now debug the full-window Canvas2D paint/compositor/screenshot-readback path before spending time on game-specific logic or WebGL shader reductions.

## DECISION_BROWSER_008 - Small 1x Canvas2D is captured; DPR and full opaque canvas are red

- Extended the Canvas2D fixture with sizing/backing variants and added `scripts/probe_canvas_reductions.py --preset sizing`.
- Latest sizing run: `CANVAS_REDUCTIONS variants=7 blocked=4 green_or_review=3 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781452258085/report.json`.
- `small-static` and `small-attribute` are `green_or_needs_review`, with Saccade `edge_ratio` around `0.021` and `saturated_ratio=0.007302`; Saccade can capture Canvas2D content in this path.
- `static`, `alpha-false`, and `dpr-no-transform` are red at full-window size with Saccade `edge_ratio=0.0`.
- `small-dpr` is red even at `720x420`, so DPR backing is a separate trigger from full-window size.
- `dom-background` is green despite a GL warning, so the warning remains neither required nor sufficient for the missing captured-layer failure.
- Next BP-011 reductions should find the opaque-canvas size threshold and split full-canvas background fill from transparent canvas plus foreground drawing.

## DECISION_BROWSER_009 - Canvas2D screenshot red threshold starts near 1154x650 backing pixels

- Added parametric variants such as `size-960x540`, `size-1152x648`, and `dpr-size-360x210`, plus runner preset `scripts/probe_canvas_reductions.py --preset threshold`.
- The aggregate runner now records largest canvas CSS rect and backing size for each variant.
- Latest threshold run: `CANVAS_REDUCTIONS variants=7 blocked=5 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781454026421/report.json`.
- 1x opaque Canvas2D remains green at `size-960x540` with Saccade backing `962x542`, `edge_ratio=0.028963`, and `saturated_ratio=0.007318`.
- 1x opaque Canvas2D goes red at `size-1152x648` with Saccade backing `1154x650`, `edge_ratio=0.0`, and `saturated_ratio=0.0`.
- DPR backing remains risky at small CSS size: `dpr-size-360x210` routes red with backing `724x424`, just below the edge threshold, and `small-dpr` is fully red with backing `1444x844`.
- Next reductions should refine the 1x threshold between `960x540` and `1152x648`, remove border/shadow from the threshold fixture, and then split fill style from backing size.

## DECISION_BROWSER_010 - Canvas2D mid-size reductions are readback-unstable

- Added borderless/no-shadow variants such as `bare-size-1024x576`, runner preset `--preset threshold-bare`, and runner option `--repeat N`.
- Bare threshold run: `CANVAS_REDUCTIONS variants=6 blocked=2 green_or_review=4 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781455791930/report.json`.
- The bare variants give exact canvas rect/backing sizes, removing the border drift from the prior matrix.
- `bare-size-960x540` was green; `bare-size-1152x648` was red in both bare-threshold passes.
- Midpoints are unstable: `bare-size-1024x576` flipped from red to green, and `bare-size-1088x612` flipped from green to red.
- `--repeat 2` on `bare-size-1024x576` produced two green runs in `runs/webgl_runtime/canvas_reductions_1781456029665/report.json`.
- Therefore BP-011 should treat the current failure as size/backing plus screenshot readback or presentation timing, not a clean monotonic canvas-size threshold.

## DECISION_BROWSER_011 - Large Canvas2D gradient fill is the stable red path

- Added Canvas2D fill-mode variants and `scripts/probe_canvas_reductions.py --preset fill`.
- Latest fill run: `CANVAS_REDUCTIONS variants=12 blocked=2 green_or_review=10 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781457595886/report.json`.
- At `960x540`, gradient, solid, and transparent foreground variants are all green across two repeats.
- At `1152x648`, the gradient-backed variant is red across two repeats with Saccade `edge_ratio=0.0` and `saturated_ratio=0.0`.
- At the same `1152x648` size, solid full-canvas fill and transparent foreground drawing are green across two repeats.
- This narrows BP-011 from "large Canvas2D" to large Canvas2D gradient/background paint plus screenshot readback/presentation timing.
- The GL warning remains inverted and unreliable: the red gradient runs had no warning, while the green solid/transparent runs did.

## DECISION_BROWSER_012 - Two-stop gradient plus foreground is enough to trigger BP-011

- Added `gradient2`, `gradient3`, gradient-only, and full-window solid/gradient variants plus `scripts/probe_canvas_reductions.py --preset gradient`.
- Latest gradient run: `CANVAS_REDUCTIONS variants=14 blocked=5 green_or_review=9 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781459417372/report.json`.
- `bare-gradient2-size-1152x648` is red across two repeats with Saccade `edge_ratio=0.0` and `saturated_ratio=0.0`, so the failure does not require a three-stop gradient.
- Full-window `static` gradient plus foreground is red across two repeats, while full-window `full-solid` plus the same foreground is green across two repeats.
- Three-stop `bare-size-1152x648` remains unstable, flipping green/red in this run.
- Gradient-only variants cannot be classified by the current gameplay-layer edge gate because Chrome's smooth gradient has too little edge structure. The next BP-011 step should add a smooth-gradient metric rather than treating those as green.

## DECISION_BROWSER_013 - Smooth-gradient metric classifies gradient-only capture

- Added smooth metrics to `scripts/probe_webgl_game_runtime.py`: `max_channel_range`, `luma_range`, and `luma_stdev`.
- The classifier uses smooth thresholds only when Chrome lacks enough edge/saturation signal for the normal gameplay-layer gate.
- Latest focused run: `CANVAS_REDUCTIONS variants=4 blocked=2 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781461281103/report.json`.
- `bare-gradient2-only-size-1152x648` is green across two repeats: Chrome smooth signal is `channel_range=39`, `luma_range=14.666667`; Saccade smooth signal is `channel_range=19`, `luma_range=8.333333`.
- `bare-gradient2-size-1152x648` remains red across two repeats: Chrome has foreground edge/saturation; Saccade has `channel_range=0` and `luma_range=0`.
- BP-011 is now narrowed to gradient plus foreground/presentation ordering, not smooth gradient alone.

## DECISION_BROWSER_014 - Draw order does not explain gradient plus foreground loss

- Added gradient ordering reductions to `test_pages/canvas_runtime/index.html` and the `gradient` preset.
- Latest ordering run: `CANVAS_REDUCTIONS variants=8 blocked=6 green_or_review=2 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781463104679/report.json`.
- `bare-gradient2-foreground-first-size-1152x648` is red across two repeats, despite drawing foreground first and then painting the gradient behind it with `destination-over`.
- `bare-gradient2-delayed-foreground-size-1152x648` is also red across two repeats. One run preserved the Saccade smooth gradient signal (`channel_range=19`, `luma_range=8.333333`) while still missing foreground edge/saturation; the other was fully black (`channel_range=0`, `luma_range=0`).
- The next BP-011 split should compare in-page canvas backing/readPixels or checksum against the audit screenshot to prove whether pixels exist inside the page but disappear in screenshot readback.

## DECISION_BROWSER_015 - BP-011 is after Canvas2D backing update

- Added `pixelProbe` to `scripts/webgl_page_probe.js` so Chrome and Saccade record page-side 2D canvas backing metrics without logging page values.
- Changed `scripts/probe_webgl_game_runtime.py` and `scripts/chrome_reference_cdp.py` to capture screenshots before running the canvas pixel probe, avoiding `getImageData()` as a screenshot warm-up.
- Latest focused run: `CANVAS_REDUCTIONS variants=2 blocked=2 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781464997166/report.json`.
- `bare-gradient2-size-1152x648` is red in the Saccade screenshot (`edge=0.0`, `sat=0.0`) while Saccade page backing has foreground signal (`edge=0.034318`, `sat=0.011096`, `maxChannelRange=237`, `lumaRange=165.666667`).
- `bare-gradient2-delayed-foreground-size-1152x648` has the same split: screenshot foreground is missing, but page backing has foreground signal (`edge=0.034173`, `sat=0.01105`).
- BP-011 is now narrowed past page script, DOM readiness, and Canvas2D drawing. Next compare Servo `WebView::take_screenshot()` with our manual `paint()+read_to_image()` path and test `present()` / frame-ready sequencing.

## DECISION_BROWSER_016 - Park BP-011 after present readback attempt

- Tried the minimal embedder fix of calling `RenderingContext::present()` between `WebView::paint()` and manual `read_to_image()`.
- Focused run stayed red: `CANVAS_REDUCTIONS variants=2 blocked=2 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781465527374/report.json`.
- Both variants still routed `screenshot_readback_after_canvas_backing`.
- The runtime code change was reverted because it did not improve the red gate.
- BP-011 stays documented with red gates, but active debugging is parked while browser productization continues elsewhere. Resume with Servo `WebView::take_screenshot()` versus manual readback when canvas-heavy dogfood becomes the launch blocker again.

## DECISION_BROWSER_017 - Page click recovers from shell modes

- Added a dogfood browser focus-recovery step for active shell modes.
- A page mouse press now cancels title-bar address entry and dismisses active native `<select>` handoff before forwarding the original click to Servo.
- This keeps `Cmd+L` and select handoff from trapping ordinary page interaction while the full clickable browser toolbar is still pending.

## DECISION_BROWSER_018 - Mouse history buttons route to browser navigation

- Added dogfood browser handling for hardware mouse Back/Forward buttons.
- Back/Forward side-button presses now call the same browser history helpers as `Cmd+[` and `Cmd+]` instead of being forwarded as ordinary page mouse events.
- Visible clickable toolbar buttons were still pending at this point; see
  DECISION_BROWSER_020 for the native toolbar v0.

## DECISION_BROWSER_019 - Same-WebView shell navigation control plane

- Dogfood control endpoint now supports browser-shell primitives:
  `shell_status`, `navigate`, `reload`, `back`, and `forward`.
- These commands operate on the already-open dogfood WebView and do not inject
  toolbar DOM or CSS into the page, so page truth/action maps remain clean.
- Keyboard navigation and control-plane navigation now share the same internal
  helpers for URL loading, reload, history back, and history forward.
- Grant artifacts and MCP grant responses now advertise shell navigation as a
  same-WebView capability when a live dogfood control endpoint is present.
- Smoke evidence:
  `runs/mcp/same_webview_shell_nav_smoke_1781579239152.json`.
- Grant artifact:
  `runs/current_tab_grants/mcp_shell_nav_smoke.json`.
- Result: `shell_status`, `navigate`, `reload`, `back`, and `forward` all
  returned `runtime=saccade-dogfood-control-v0`; the smoke navigated from
  `current_tab_copilot` to `formmax`, reloaded, went back, and went forward in
  the same visible dogfood window.
- This is the control plane that backs the native toolbar v0 in
  DECISION_BROWSER_020. The MCP-facing named navigation tool remains open.

## DECISION_BROWSER_020 - Dogfood browser has native clickable toolbar v0

- Added a native shell toolbar overlay to the dogfood browser using Servo's
  exposed `RenderingContext::glow_gl_api()` plus
  `RenderingContext::prepare_for_rendering()`.
- The toolbar is not page DOM and does not inject CSS or JS into the loaded
  page. It paints in the shell after `WebView::paint()` and before
  `present()`.
- Toolbar hit-zones are consumed by the shell:
  Back, Forward, Reload, address command, and Copilot current-tab grant.
- The same control endpoint now reports toolbar status under `ping` and
  `shell_status`, including target rects and `page_dom_injected=false`.
- Smoke evidence:
  `runs/browser_shell/visible_toolbar_file_smoke.png`.
- Verification:
  `cargo check -p saccade_browser`, `cargo check -p saccade-shell`,
  `git diff --check`, and two short `saccade-shell browse` smoke runs.
- Known limitation: v0 overlays the top 44 CSS px of page content. A final
  browser chrome should use an offscreen/compositor or viewport layout so the
  toolbar does not obscure page content.

## DECISION_BROWSER_021 - Address strip text is shell-owned, not page DOM

- Upgraded the native dogfood toolbar address strip from a status/hit-zone to a
  visible shell editor affordance.
- It now paints URL/placeholder text, secure/search icon, active focus/error
  state, caret, and loading indicator directly through the shell GL overlay.
- This intentionally avoids injecting browser chrome into page DOM, CSS, or JS,
  preserving page truth/action maps for agent work.
- Verification:
  `cargo test -p saccade_browser toolbar`,
  `cargo test -p saccade_browser shell_title`,
  `cargo check -p saccade_browser -p saccade-shell`, and visual smoke
  `runs/browser_shell/toolbar_address_text_v1.png`.
- Known limitation: the current text renderer is a small GL bitmap font, good
  enough for dogfood function but not final platform-quality macOS browser
  chrome. Track polish separately from the browser/agent truth contract.

## DECISION_BROWSER_022 - Product browser UI routes to official ServoShell

- Wayne observed that the legacy Saccade URL bar still feels rough while the
  official ServoShell URL bar is already much closer to a normal browser.
- Source inspection confirmed that official ServoShell headed UI uses egui for
  browser chrome and already has Back, Forward, Reload/Stop, tabs, location
  input, `Cmd+L`, select-all-on-focus, and WebView resizing below the toolbar.
- Decision: stop investing in platform-quality browser chrome inside the legacy
  `crates/saccade_browser/src/dogfood.rs` GL toolbar. Keep that shell as a
  bridge/proof fallback, but make official ServoShell UI the product human layer.
- Integration route: first attach Saccade through official ServoShell
  WebDriver/adapter; escalate to a thin official ServoShell source fork only if
  WebDriver cannot meet trusted-tab isolation, screenshot policy, native input
  ownership, or millisecond reflex-loop gates.
- Do not patch the downloaded `/Applications/Servo.app` binary directly.

## DECISION_BROWSER_023 - Canvas red gate is manual readback, not Servo screenshot API

- Resumed AI-008/BP-011 with a same-page path comparison runner:
  `scripts/probe_canvas_screenshot_paths.py`.
- Added a local-fixture-only worker method, `take_screenshot_audit`, that calls
  Servo `WebView::take_screenshot()` and refuses non-local URLs. This is a
  diagnostic path only; normal real-page screenshot policy and sensitive-field
  guards still use the regular audit/truth flow.
- Latest path comparison:
  `CANVAS_SCREENSHOT_PATHS variants=2 errors=0 manual_blocked=1 take_blocked=0 route=manual_readback_only report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_screenshot_paths_1781805458432/report.json`.
- In `bare-gradient2-size-1152x648`, Saccade manual
  `paint()+read_to_image()` produced a white screenshot with
  `edge_ratio=0.0`, `saturated_ratio=0.0`, `luma_range=0.0`.
- The same Servo tab's `WebView::take_screenshot()` captured the foreground
  canvas correctly: `edge_ratio=0.028048`, `saturated_ratio=0.007514`,
  `luma_range=165.666667`. Page-side canvas backing also had foreground
  signal: `edgeRatio=0.034318`, `saturatedRatio=0.011096`.
- Control variant `bare-solid-size-1152x648` stayed green on both manual and
  `take_screenshot()` paths.
- BP-011 is now narrowed again: for non-hot diagnostic screenshots, prefer
  `WebView::take_screenshot()` on local/non-sensitive fixtures; do not trust
  manual readback as the only visual evidence for Canvas2D reductions. The
  reflex hot-loop path still needs its own readback gate because
  `take_screenshot()` is asynchronous and not allowed in the millisecond loop.

## DECISION_BROWSER_024 - Canvas diagnostics default to take-local screenshot mode

- Updated `scripts/probe_webgl_game_runtime.py` with
  `--saccade-screenshot-mode take-local|take|manual`, defaulting to
  `take-local`.
- `take-local` uses Servo `WebView::take_screenshot()` for `file://`,
  `localhost`, `127.0.0.1`, and `::1` fixture diagnostics, then falls back to
  the existing manual audit readback path for non-local URLs.
- Updated `scripts/probe_canvas_reductions.py` to pass the screenshot mode
  through and record `saccade_screenshot_method` per variant.
- Focused default gate:
  `CANVAS_REDUCTIONS variants=1 blocked=0 green_or_review=1 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781806451861/report.json`.
- Same variant forced through manual readback:
  `CANVAS_REDUCTIONS variants=1 blocked=1 green_or_review=0 errors=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/canvas_reductions_1781806531266/report.json`.
- Therefore AI-008B is closed: local diagnostic reports no longer overcall
  Canvas2D as red when only manual readback is blank. Manual/readback remains a
  deliberate gate via `--saccade-screenshot-mode manual` for hot-loop and
  readback-specific work.

## DECISION_BROWSER_025 - Reflex readback sees Canvas2D foreground

- Added lightweight sample metrics to the source ServoShell reflex bridge:
  `sample_saturated`, `sample_max_channel_range`, and `sample_luma_range`.
  These are observe-log summaries only; no screenshot or pixel dump is written.
- Added `scripts/probe_reflex_readback_canvas.js`, a focused local fixture gate
  that launches source ServoShell with `SACCADE_REFLEX_OBSERVE_PATH` and checks
  the actual reflex `RenderingContext::read_to_image()` path.
- Positive gate:
  `REFLEX_READBACK_CANVAS route=readback_foreground_present ok=true frames=5 readback_ok=5 max_channel_range=235 max_luma_range=158 max_saturated_ratio=0.006338 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/reflex_readback_canvas_1781806982624/report.json`.
- Negative gradient-only control:
  `REFLEX_READBACK_CANVAS route=readback_blank_or_flat ok=false frames=5 readback_ok=5 max_channel_range=19 max_luma_range=9 max_saturated_ratio=0 report=/Users/waynema/Documents/GitHub/SACCADE/runs/webgl_runtime/reflex_readback_canvas_1781807000176/report.json`.
- Therefore AI-008C is green for the focused local Canvas2D foreground gate:
  the ms/reflex readback path sees foreground pixels when source ServoShell is
  used. The older blank screenshot remains scoped to the legacy worker manual
  diagnostic readback path.

## DECISION_BROWSER_026 - Mythcastera failure is missing IntersectionObserver

- `https://mythcastera.com/` returns healthy HTML from Vercel and renders in
  Chrome, but downloaded official ServoShell replaced the page with Next's
  default global error UI: "This page couldn't load."
- ServoShell stderr showed the concrete runtime error:
  `IntersectionObserver is not defined`.
- Servo's DOM truth confirmed the failed production page had only
  `body_text_length=70` and actions `Reload` / `Back`; Chrome saw
  `bodyTextLength=8694` and 26 actions.
- The site source in `/Users/waynema/Documents/GitHub/nan-game/site` now
  guards direct `IntersectionObserver` usage and Framer Motion `whileInView`
  usage. Missing support degrades to static visible content instead of
  throwing during hydration.
- Local production verification passed: ServoShell on
  `http://127.0.0.1:4277/` saw `body_text_length=8691`, no
  `IntersectionObserver` error, and a dark hero screenshot instead of the
  white Next error page.
- This is an owned-site compatibility fix, not proof that Servo implements
  IntersectionObserver. Keep broad third-party pages routed until measured.

## DECISION_BROWSER_027 - AI-008D live game reflex readback gate is green

- Built the source-fork release ServoShell because the prior release binary had
  been cleaned from `target/release/servoshell`.
- Debug source ServoShell kept the Saccade reflex bridge but was not accepted as
  product performance evidence for this gate: it produced readback, semantic
  facts, and motor receipts, but game time advanced at only `time_scale=0.095`
  in `runs/local_game_reflex/ai008d_live_game_1781809461/report.json`.
- Downloaded official Servo.app ran the local game at normal speed
  (`time_scale=0.983`) but did not have the Saccade `SACCADE_REFLEX_*`
  command/readback bridge, so it produced zero readback frames and zero command
  receipts in `runs/local_game_reflex/ai008d_official_probe_1781809552/report.json`.
- Source-release ServoShell is therefore the accepted AI-008D target because it
  has both normal timing and the in-process reflex bridge.
- Gate command:
  `node scripts/run_local_game_reflex_loop.js --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --url http://127.0.0.1:4173/ --headless --window-size 1280x900 --duration-ms 15000 --policy visual --output-dir runs/local_game_reflex/ai008d_live_game_release_1781810191`.
- Result:
  `live_game_reflex_readback_green`, `ok=true`, 1292/1292 readbacks,
  foreground route `readback_foreground_present`, 176 semantic facts, 53
  commands, 53 command receipts, `time_scale=0.989`, `fill_delta=12`,
  `drop_delta=25`, and `hp_delta=0`.
- Therefore AI-008D is closed for the local game: the hot-loop readback path,
  Browser Fact Stream, semantic classification, visual motor policy, receipts,
  replay, and review artifact all appear in one source-release artifact.
- BP-011 remains monitoring for broad third-party Canvas/WebGL sites. Do not
  generalize the local-game pass to unknown pages without evidence.

## DECISION_BROWSER_028 - Servo 0.2 is retired from default dogfood

- The dogfood release builder exposed an avoidable cost: defaulting to
  `saccade-shell` pulls the legacy embedded `servo=0.2.0` stack into release
  builds even though the product browser path has moved to ServoShell 0.3.
- We are not doing an in-place `servo=0.2.0 -> 0.3.x` upgrade inside
  `crates/saccade_browser`. That keeps the wrong browser shell and creates a
  heavy Servo API migration.
- Instead, default dogfood now builds only `saccade-mcp` and
  `saccade-servoshell`. The legacy shell is opt-in:
  `SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh`.
- New default launcher:
  `dist/saccade-dogfood-*/open-saccade <URL>` -> `saccade-servoshell bridge --no-headless --url <URL>`.
- New local game wrapper:
  `dist/saccade-dogfood-*/run-local-game-reflex http://127.0.0.1:4173/`.
- Evidence: `./scripts/build_dogfood_release.sh dist/saccade-dogfood-test-ai014`
  finished in 18 seconds and produced no `bin/saccade-shell`; bridge smoke
  passed at `runs/dogfood_release/ai014_bridge_smoke/report.json`; reflex
  wrapper passed at `runs/local_game_reflex/ai014_kit_reflex_smoke/report.json`.
- Full retirement plan: `docs/servo_0_2_retirement_plan.md`.

## DECISION_SERVOSHELL_006 - First official ServoShell adapter gate is Rust-owned

- Added `bins/saccade-servoshell` as the first product-gate adapter over
  official ServoShell's WebDriver server.
- The adapter does not import or upgrade the `servo` crate; it launches the
  installed `/Applications/Servo.app/Contents/MacOS/servoshell` binary with a
  random `127.0.0.1` WebDriver port and `--temporary-storage`.
- Screenshot policy is explicit: `forbidden` by default, and
  `guarded_diagnostic` only captures after the redacted truth preflight reports
  no visible sensitive surface.
- First gate command passed:
  `cargo run -q -p saccade-servoshell -- selftest --servoshell /Applications/Servo.app/Contents/MacOS/servoshell`.
- Evidence:
  `runs/servoshell_adapter/adapter_1781482592445/summary.json`.
- Local game truth probe also passed without screenshot capture:
  `runs/servoshell_adapter/probe_1781482435257/report.json`.

## DECISION_SERVOSHELL_007 - Adapter safety matrix covers visible, hidden, and editable secrets

- Added `test_pages/browser_session_safety_matrix/index.html`.
- The fixture covers 9 redaction kinds: `ssn`, `credit_card`, `password`,
  `government_id`, `api_token`, `otp`, `email`, `hidden`, and
  `recovery_token`.
- Tightened the adapter truth bundle so sensitive action labels do not fall
  back to editable/text-area content when a field lacks a clean label.
- Latest gate command passed:
  `cargo run -q -p saccade-servoshell -- selftest --servoshell /Applications/Servo.app/Contents/MacOS/servoshell`.
- Evidence:
  `runs/servoshell_adapter/adapter_1781483074229/summary.json`.
- Raw fixture secret grep over the run directory returned no matches.

## DECISION_SERVOSHELL_008 - FORMMAX runs through official ServoShell adapter

- Added `saccade-servoshell formmax-selftest`.
- The gate preserves the browser truth-layer shape: collect redacted page/field
  truth, fill ordinary controls in the live browser transaction, block
  sensitive fields, verify the receipt, and replay only field/action metadata
  with `echo_values=false`.
- Official ServoShell adapter evidence:
  `runs/servoshell_adapter/formmax_1781484157780/result.json`.
- Result: `rows=96`, `pages=2`, `filled=672`, `blocked_sensitive=3`,
  `receipt_verified=true`, and `replay_events=2715`.
- Leak check passed for representative table values in result/replay artifacts.
- The local FORMMAX fixture now renders table rows with DOM APIs instead of
  `tr.innerHTML`, because official ServoShell repeatedly warned
  `foster parenting not implemented` and the adapter timed out during lazy
  table rendering on the old fixture implementation.

## DECISION_SERVOSHELL_009 - Local game smoke passes on official ServoShell

- Ran the adapter against `http://127.0.0.1:4173/`.
- Evidence:
  `runs/servoshell_adapter/probe_1781484941056/report.json`.
- The page loaded with title `Blend or Die - Prototype`, no sensitive surface,
  and a guarded diagnostic screenshot at
  `runs/servoshell_adapter/probe_1781484941056/screenshot.png`.
- Tightened action-map visibility filtering to reject controls whose ancestors
  have hidden-style classes such as `hidden`, `is-hidden`,
  `visually-hidden`, or `sr-only`; this removed a false visible `Restart`
  action from the game's hidden end-screen overlay.
- Re-ran `saccade-servoshell selftest` and `formmax-selftest` after the change;
  both passed.

## DECISION_SERVOSHELL_010 - Focused typing runs through official ServoShell adapter

- Extended `saccade-servoshell selftest` with a focused typing gate for
  textarea, contenteditable, and sensitive password fixtures.
- Textarea typing uses official ServoShell WebDriver `element/value` against
  the current active element after a redacted focused-field preflight.
- Contenteditable typing uses the same preflight and attempts WebDriver first;
  if WebDriver does not change the field, it records and uses the safe
  contenteditable insert fallback.
- Focused sensitive fields are blocked before typing with
  `focused_field_sensitive`.
- Replay records field metadata, selector hashes, before/after lengths, method,
  and `echo_values=false`; it does not log typed text.
- Latest gate:
  `cargo run -q -p saccade-servoshell -- selftest --timeout-sec 35`.
- Evidence:
  `runs/servoshell_adapter/adapter_1781623388958/summary.json`.
- Result: focused textarea changed via `webdriver_element_value`,
  contenteditable changed via `js_contenteditable_insert_fallback`, focused
  password was blocked, and grep over the run directory found no typed text or
  safety-matrix fixture secrets.

## DECISION_SERVOSHELL_011 - Native input/dropdown gate runs through official ServoShell adapter

- Extended `saccade-servoshell selftest` with the local native input fixture.
- Text input uses official ServoShell WebDriver `element/value` and verifies
  the field value only inside the process; reports and replay store value
  length, event counts, and `echo_values=false`, not the typed text.
- The official ServoShell WebDriver `<select>` click/popup path returned an
  unusable execute response during probing, so the adapter does not claim
  native select popup control. It records a route instead: attempt WebDriver
  `element/value`, then use `js_select_fallback` if select value and
  `input/change` events are not complete.
- Latest gate:
  `RUST_LOG=error cargo run -q -p saccade-servoshell -- selftest --timeout-sec 35`.
- Evidence:
  `runs/servoshell_adapter/adapter_1781624931973/summary.json`.
- Result: `input_value_matches=true`, `input_events=9`,
  `select_value=gamma`, `select_input_events=1`,
  `select_change_events=1`, and `select_method=js_select_fallback`.
- Grep over the run directory found no typed text or safety-matrix fixture
  secrets.

## DECISION_SERVOSHELL_012 - Login handoff has an external adapter pass with a clear boundary

- Extended `saccade-servoshell selftest` with a local HTTP
  `login_handoff` fixture server and official ServoShell WebDriver gate.
- The external adapter proves a same-session product handoff: the human phase
  logs in on the visible login page, clicks explicit `Done`, then the agent
  phase continues in the same ServoShell session with inherited cookie/storage.
- The gate records `agent_before_handoff_blocked_by_policy=true` because the
  adapter does not expose an agent phase before Done. It does not claim
  independent multi-WebView Human/Agent tab ownership; that remains a
  thin-fork or in-process bridge concern.
- Screenshot policy blocks capture on the login page because a visible
  sensitive surface is present.
- Latest gate:
  `RUST_LOG=error cargo run -q -p saccade-servoshell -- selftest --timeout-sec 35`.
- Evidence:
  `runs/servoshell_adapter/adapter_1781626639174/summary.json`.
- Result: `human_login=true`, `handoff_done=true`, `agent_session=true`,
  `password_exposed=false`, `otp_exposed=false`,
  `agent_before_handoff_blocked_by_policy=true`, and
  `screenshot_decision=blocked_sensitive_surface`.
- Grep over the run directory found no password/OTP or safety-matrix fixture
  secrets.

## DECISION_SERVOSHELL_013 - Official ServoShell bridge writes MCP-compatible current-tab grants

- Added `saccade-servoshell bridge`.
- The bridge launches official ServoShell with WebDriver, creates one
  WebDriver session, starts a loopback line-delimited JSON control endpoint,
  and writes `runs/current_tab_grants/servoshell_latest.json`.
- The grant artifact uses `grant_type=current_tab_copilot`, `owner=Human`,
  `read_grant=FullTruth`, `agent_input_grant=true`, and the existing
  `saccade-dogfood-control-v0` endpoint protocol so MCP can parse it without a
  new schema.
- Bridge v0 supports `ping`, `shell_status`, `truth`, `actions`, `navigate`,
  `reload`, `back`, and `forward`.
- It intentionally does not yet claim `fill_agent_fields`, `inspect_fields`,
  `act`, or `formmax_live_fill`; those require another adapter pass or MCP
  capability negotiation.
- Smoke gate:
  `RUST_LOG=error cargo run -q -p saccade-servoshell -- bridge --smoke --timeout-sec 35`.
- Evidence:
  `runs/servoshell_adapter/bridge_1781627953527/report.json`.
- Result: bridge smoke wrote
  `runs/current_tab_grants/servoshell_latest.json`, verified endpoint
  `ping/truth/actions`, saw one action on the smoke fixture, and preserved the
  full existing `saccade-servoshell selftest` pass afterward at
  `runs/servoshell_adapter/adapter_1781627800314/summary.json`.

## DECISION_REFLEX_001 - Local game reflex is the current kill gate

- The project goal is not ServoShell plus WebDriver automation. It is a
  human-visible browser with Saccade's millisecond agent truth/control path.
- WebDriver remains useful for product/safety gates such as FORMMAX and login
  handoff, but it is not accepted as the MOUSEMAX/local-game reflex runtime.
- The launch-critical target is now documented in
  `docs/local_game_reflex_gate.md`: an LLM can play our designed browser game
  by delegating the frame loop to a local Saccade reflex runtime.
- The LLM must not think on every frame. It sets strategy; the local reflex loop
  owns frame truth, detection, motor decision, input dispatch, verification, and
  replay.
- General product truth remains redacted DOM/layout/action/form truth without
  screenshots by default. Pixel/frame truth is allowed only for explicitly
  non-sensitive reflex modes such as local games and benchmarks.
- If official ServoShell cannot expose ms-level frame truth and input control
  externally, the next move is a thin in-process ServoShell bridge, not more
  WebDriver glue.

## DECISION_REFLEX_002 - Local game visual facts now drive motor policy

- The local game reflex runner defaults to `--policy visual`.
- The motor loop drains live Browser Fact Stream output, classifies
  `visual_object_seen` facts into `semantic_object_seen` facts, maintains a
  short-lived visual state, and chooses drag commands from those semantic
  objects.
- The orbit/debug policy remains as a baseline, but the accepted reflex path is
  now semantic-fact-driven.
- Release evidence:
  `runs/local_game_reflex/release_visual_gate_1781530639/report.json`.
- Result: `ok=true`, 51 drag commands, 51 command receipts, 467 drag phase
  dispatch receipts, 1400/1400 readback frames, 190 browser facts, 177 semantic
  facts, `fill_delta=16`, `drop_delta=18`, `hp_delta=0`, game
  `time_scale=0.993`, and dispatch p95 `0.091 ms`.
- Added `docs/SACCADE_DOGFOOD_HANDOFF.md` so the game-building session can
  continue iterating while Saccade owns browser/reflex/safety failures.

## DECISION_REFLEX_003 - Local game runs have a review-page artifact

- Added `scripts/build_local_game_reflex_review.js`.
- The script reads a local game reflex run directory and writes `review.html`
  from `report.json`, `replay.jsonl`, `commands.jsonl`, and
  `semantic_facts.jsonl`.
- The page surfaces the pass/fail verdict, fill/HP/drop deltas, time scale,
  dispatch/readback timing, Browser Fact Stream counts, semantic role/palette
  counts, a compact timeline, and an SVG map of recent semantic objects and
  drag commands.
- Generated review:
  `runs/local_game_reflex/release_visual_gate_1781530639/review.html`.
- Static verification found the expected PASS status, `Fill Delta`,
  `Semantic Facts`, `0.091 ms`, and the fact/motor map in the generated HTML.
- Codex Browser refused direct `file://` navigation to the local report under
  its URL policy, so visual verification used static page checks only.

## DECISION_REFLEX_004 - Reflex runner auto-generates review pages

- `scripts/run_local_game_reflex_loop.js` now calls the review builder after
  writing `report.json`.
- Each local game reflex run now prints and records a `review.html` artifact
  next to `report.json`, `replay.jsonl`, `facts.jsonl`, and
  `semantic_facts.jsonl`.
- Updated `docs/SACCADE_DOGFOOD_HANDOFF.md` with the command the game-building
  session should use when asking Saccade for a dogfood run.

## DECISION_N8_001 - Current Tab Co-Pilot starts as a local policy gate

- Added `saccade-shell selftest-current-tab-copilot`.
- Added `test_pages/current_tab_copilot/index.html` as the first local fixture
  for a user-started current-tab assist flow.
- The selftest simulates the selected-tab grant boundary before sending worker
  truth/action/fill requests. It then uses the live browser worker to collect
  redacted truth, collect actions, fill agent-owned non-sensitive fields, reject
  human-owned sensitive fields, inspect sensitive completion status without raw
  values, and require confirmation for Submit by policy instead of clicking it.
- Latest evidence:
  `runs/current_tab_copilot/copilot_1781533899239/report.json`.
- Worker replay:
  `runs/browser_session_worker/worker_1781533899321_98530/replay.jsonl`.
- Result: `CURRENT_TAB_COPILOT PASS selected_tab_seen=true
  grant_required=true redacted_truth=true agent_explains_page=true
  non_sensitive_filled=true sensitive_write_blocked=true
  sensitive_values_exposed=false confirmation_required=true`.
- This is not yet the final product UI. Next work is making the current-tab
  grant visible in the browser shell and exposing it through the agent-facing
  API.

## DECISION_N8_002 - Current Tab Co-Pilot has an MCP API gate

- Added `saccade.tabs.grant_current` to `saccade-mcp`.
- A granted current tab remains `TabOwner::Human`; MCP records a separate
  `agent_input_grant` so ownership and co-pilot permission do not get confused.
- The grant starts a live `browser_session_worker_v0`, returns redacted truth
  and actions, allows safe non-sensitive field fill, rejects sensitive writes,
  redacts sensitive field inspection, and blocks submit-style actions with
  `user confirmation required`.
- Latest MCP evidence:
  `runs/mcp/selftest_1781535319538/report.json`.
- Result: `MCP PASS tools_registered=20 tab_scoping=true local_dev_audit=true
  policy_gate=true`, with `tabs_grant_current=true`.
- No sensitive sentinel leak was found for `999-12-3456` or
  `SHOULD-NOT-WRITE` in the new MCP run and browser-worker artifacts.
- Next work is not another policy harness. Next work is visible browser UI that
  calls this grant for the real selected tab.

## DECISION_N8_003 - Dogfood browser exposes a visible current-tab grant

- Added a dogfood browser shortcut: `Cmd+Shift+G`.
- When triggered, the Saccade window title changes from `copilot=off
  Cmd+Shift+G` to `copilot=granted`, so the user can see that the current tab
  has been granted for agent help.
- The grant writes a compact artifact at `runs/current_tab_grants/latest.json`
  by default. The artifact records URL, title, rendering profile, owner
  `Human`, `read_grant=FullTruth`, `agent_input_grant=true`, and the MCP tool
  `saccade.tabs.grant_current`. It does not include page form values.
- Added `--auto-grant-copilot` and `--copilot-grant-path` to
  `saccade-shell browse` for smoke testing.
- Smoke evidence:
  `runs/current_tab_grants/smoke.json`.
- This is a visible user-grant bridge, not the final shared-process transport.
  MCP v0 still uses the grant URL to attach a worker. Next work is direct MCP
  access to the same live dogfood WebView after the user grants it.

## DECISION_N8_004 - MCP consumes dogfood current-tab grant artifacts

- `saccade.tabs.grant_current` now accepts `grant_path` in addition to direct
  `url`/`reason` arguments.
- The MCP tool validates the artifact instead of trusting the path: status must
  be `granted`, grant type must be `current_tab_copilot`, the selected-tab
  evidence must be present, owner must be `Human`, `agent_input_grant` must be
  true, and URL must be localhost, loopback, or file.
- After validation, MCP starts a live worker for the granted URL and marks the
  transport as `worker_from_grant_artifact_v0`. It also returns
  `same_webview_attached=false` so we do not overclaim same-WebView transport.
- Latest MCP evidence:
  `runs/mcp/selftest_1781575970510/report.json`.
- Result: `MCP PASS tools_registered=20 tab_scoping=true local_dev_audit=true
  policy_gate=true`, with `tabs_grant_current=true` and
  `tabs_grant_artifact=true`.
- No sensitive sentinel leak was found for `999-12-3456`,
  `SHOULD-NOT-WRITE`, or `SHOULD_NOT_WRITE` in the new MCP run and worker
  artifacts.
- Next work is direct MCP access to the already-open dogfood WebView.

## DECISION_N8_005 - Dogfood current-tab grants expose same-WebView control ping

- Dogfood browser now starts a loopback JSONL control endpoint for the live
  browser window.
- Current-tab grant artifacts include that `control_endpoint` with protocol
  `saccade-dogfood-control-v0`, scheme `tcp`, loopback host, and port.
- MCP validates the endpoint from the grant artifact and sends a `ping` before
  continuing with the existing worker-backed truth/action path.
- The MCP response now reports `same_webview_control_ping=true`,
  `same_webview_attached=false`, and
  `transport_status=same_webview_control_ping_plus_worker_truth_v0` when the
  ping succeeds. This is deliberately not yet a same-WebView truth/action claim.
- Smoke evidence:
  `runs/mcp/same_webview_control_smoke_1781572417690.json`.
- Grant artifact:
  `runs/current_tab_grants/mcp_bridge_smoke.json`.
- Result: MCP can prove it is connected to the already-open dogfood WebView's
  control plane. Next work is moving redacted truth, safe fill, and safe act
  commands over that same bridge.

## DECISION_N8_006 - MCP reads current-tab truth/actions from the same dogfood WebView

- Dogfood control endpoint now supports `truth` and `actions` in addition to
  `ping`.
- The endpoint reuses the browser-session worker's existing probe script and
  action-map redaction rules, so sensitive field values remain out of the
  agent-facing truth/action payload.
- When `saccade.tabs.grant_current` receives a dogfood grant artifact with a
  live control endpoint, MCP binds the tab to that endpoint and does not open a
  worker for redacted truth/actions.
- MCP `saccade.web.truth` and `saccade.web.actions` now prefer the same-WebView
  dogfood control route, then fall back to worker/report routes when no control
  endpoint exists.
- Smoke evidence:
  `runs/mcp/same_webview_truth_actions_smoke_1781575838106.json`.
- Grant artifact:
  `runs/current_tab_grants/mcp_truth_actions_smoke.json`.
- Result: `grant_current`, `web.truth`, and `web.actions` all returned
  `runtime=saccade-dogfood-control-v0`, with `same_webview_attached=true`,
  `transport_status=same_webview_control_truth_v0`, and `actions_count=6`.
- Next work is moving safe fill and safe act over the same bridge.

## DECISION_N8_007 - Same-WebView co-pilot bridge can fill, inspect, and safe-act

- Dogfood control endpoint now supports `fill_agent_fields`, `inspect_fields`,
  and `act`.
- The fill and inspect methods reuse the browser-session worker's existing
  field scripts, so agent-owned non-sensitive fields can be filled while
  sensitive fields are rejected or redacted.
- MCP `saccade.web.fill_agent_fields` and `saccade.web.inspect_fields` now treat
  dogfood control as a live browser session, while keeping the existing worker
  fallback.
- MCP `saccade.web.act` can dispatch safe non-side-effect actions through the
  same dogfood WebView. Submit/export/delete/payment/sign/confirm-like actions
  still stop at the MCP confirmation gate.
- Smoke evidence:
  `runs/mcp/same_webview_fill_act_smoke_1781576647007.json`.
- Grant artifact:
  `runs/current_tab_grants/mcp_fill_act_smoke.json`.
- Result: fill, inspect, actions, and safe act all returned
  `runtime=saccade-dogfood-control-v0`; ordinary fields filled=3, sensitive
  fields rejected=2, sensitive inspected values redacted=2, and Submit remained
  blocked by `user confirmation required`.
- Next work is current-tab FORMMAX plus browser shell basics for normal users.

## DECISION_N8_008 - Current-tab FORMMAX runs through the same dogfood WebView

- Dogfood control endpoint now supports `formmax_live_fill` in addition to
  `ping`, `truth`, `actions`, `fill_agent_fields`, `inspect_fields`, and `act`.
- MCP `saccade.web.fill_form` now treats dogfood control as a live browser
  session and prefers the same-WebView route when a current-tab grant artifact
  includes a live `control_endpoint`.
- The dogfood FORMMAX response returns counts, validation status, and masked
  sensitive-field presence only. It does not return the filled table values or
  sensitive values.
- FORMMAX control timeout is longer than ordinary probes because the local
  fixture intentionally fills 96 rows across two pages; ordinary truth/action
  probes keep the short timeout.
- MCP selftest now explicitly closes live browser workers before spawning the
  standalone static FORMMAX runner, preventing macOS GL/WebView resource
  contention during the full selftest sequence.
- Smoke evidence:
  `runs/mcp/same_webview_formmax_smoke_1781578030042.json`.
- Grant artifact:
  `runs/current_tab_grants/mcp_formmax_live_smoke.json`.
- Result: `runtime=saccade-dogfood-control-v0`,
  `engine=saccade-dogfood-control-formmax-live-v0`, rows=96, pages=2,
  filled=672, blocked_sensitive=3, receipt_verified=true,
  validation_errors=0, replay_events=2711.
- Latest full MCP selftest:
  `runs/mcp/selftest_1781578578728/report.json`.
- Next work is browser shell basics and real editor/contenteditable dogfood.

## DECISION_N8_009 - MCP exposes named shell navigation for granted dogfood tabs

- MCP now registers `saccade.browser.navigate` in the `browser` namespace.
- The tool supports `status`, `navigate`, `reload`, `back`, and `forward` on
  an already-granted Human-owned current tab that has a same-WebView dogfood
  control endpoint.
- The tool refuses non-granted tabs, paused tabs, and tabs without a same-WebView
  control endpoint. It forwards only to the dogfood shell control plane and does
  not inject browser UI or action helpers into page DOM.
- Browser navigation receipts update MCP's session URL, title, and page revision
  from the shell result, so later truth/action calls have a current basis.
- Stop remains open because the pinned public Servo/WebView surface used here
  already exposes reload/back/forward/navigation through our shell, but no
  product-safe public stop-loading call has been mapped yet.
- Latest full MCP selftest:
  `runs/mcp/selftest_1781583895286/report.json`.

## DECISION_BP_003_STOP_001 - Stop remains blocked on pinned Servo public API

- AI-001 asked whether the dogfood browser can wire product-grade Stop behavior
  on the current pinned Servo `0.2.0` API.
- Local rustdoc/source inspection found public `WebView::load`,
  `load_request`, `reload`, `can_go_back`, `go_back`, `can_go_forward`, and
  `go_forward`, but no public `stop_loading`, `stop_load`, `cancel_load`,
  `cancel_loading`, or `pub fn stop` equivalent.
- Search scope was local, pinned evidence: `target/doc/src/servo`,
  `target/doc/servo`, and `~/.cargo/registry/src/.../servo-0.2.0`.
- Decision: do not fake Stop with reload/navigation. Keep Stop as
  `blocked-public-api` in `docs/CURRENT_ACTION_ITEMS.md`; revisit only through
  official ServoShell source, newer Servo API proof, or a deliberate fork hook.

## DECISION_SERVOSHELL_016 - Productize external ServoShell bridge with artifacts before UI fork

- The official ServoShell bridge can already route truth/actions/fill/inspect/
  safe-act/FORMMAX/navigation through one user-granted Human tab.
- Product evidence was still incomplete because live bridge responses returned
  summary data while `artifacts.report` and `artifacts.replay` were null.
- Decision: keep official ServoShell UI intact for now and productize the
  external bridge contract first. Each control request writes sanitized
  `control/report.json` and `control/replay.jsonl`; FORMMAX appends its
  no-value event stream; the generic control summary records method, counts,
  policy, and verification metadata without request values.
- `shell_status` now includes a Copilot state object: Human owner, FullTruth
  read grant, agent input grant, side-effect confirmation requirement, and
  `sensitive_values_exposed_to_agent=false`.
- Visible in-window Human/Agent badge remains an AI-004 product UI task and may
  require a thin ServoShell fork; the MCP/source-of-truth state is now present.
- Evidence:
  `runs/mcp/selftest_1781636671768/report.json`,
  `runs/mcp/servoshell_bridge_grant_1781636716084/control/report.json`, and
  `runs/mcp/servoshell_bridge_grant_1781636716084/control/replay.jsonl`.

## DECISION_SERVOSHELL_017 - Upgradeability passes; trusted visible badge routes to UI fork

- The same MCP bridge gate now runs against a caller-selected ServoShell binary
  through `SACCADE_SERVOSHELL_BIN`.
- Installed official Servo.app evidence remains
  `Servo 0.3.0-302457869`.
- Source-release evidence uses
  `/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell`,
  which reports `Servo 0.3.0-805e6a423`.
- Result: `RUST_LOG=error SACCADE_SERVOSHELL_BIN=... cargo run -q -p
  saccade-mcp -- selftest` passed with
  `servoshell_bridge_grant=true`,
  `servoshell_bridge_formmax_live=true`,
  `servoshell_bridge_artifacts=true`,
  `web_fill_form_live=true`, and `browser_navigate=true`.
- Evidence:
  `runs/mcp/selftest_1781636405474/report.json` and
  `runs/mcp/servoshell_bridge_grant_1781636453696/control/replay.jsonl`.
- Visible badge decision: do not inject a page overlay, user stylesheet, or
  `document.title` mutation to fake a trusted Human/Agent/Copilot browser badge.
  Those options are spoofable or change the page the user is trying to inspect.
  External bridge state is available through `shell_status`; a trusted in-window
  badge belongs in a thin ServoShell UI fork or upstream chrome hook.

## DECISION_SERVOSHELL_018 - Real AdMob dogfood proves Human-confirmed account workflow

- Official ServoShell `0.3.0` plus the external Saccade bridge opened a logged-in
  Google AdMob app page for AI Chaos Story Lab:
  `https://admob.google.com/v2/apps/1195435270/overview`.
- The bridge read compact truth/action data from the Human-owned visible tab,
  navigated to App settings, found the App Store details Add flow, filled the
  App Store URL, searched, selected `AI Chaos Story Lab / Free | iOS / NaN Logic
  LLC / App Store`, and stopped before the side-effecting Save button.
- The Human clicked Save after visual confirmation. This is the desired
  copilot boundary for account/monetization surfaces: agent accelerates
  navigation and non-sensitive data entry, Human owns final confirmation.
- AdMob then reported that store details were updated, but app verification
  could not complete because app-ads.txt details did not yet match the AdMob
  account/crawler state.
- Independent checks showed Apple public metadata has
  `sellerUrl=https://www.nanlogic.com/ai-chaos-story-lab`, while
  `https://www.nanlogic.com/app-ads.txt` returns `200 text/plain` with:
  `google.com, pub-8420020231782289, DIRECT, f08c47fec0942fa0`.
- Decision: classify this as a successful high-value real-site dogfood with a
  pending provider verification/crawler wait, not a Saccade action failure. Keep
  screenshots forbidden and continue requiring Human confirmation for AdMob
  Save/Verify-style actions.
- Evidence:
  `runs/servoshell_adapter/admob_visible_1781731754/control/report.json` and
  `runs/servoshell_adapter/admob_visible_1781731754/control/replay.jsonl`.

## DECISION_SITE_POLICY_019 - Site classification is evidence-first

- Wayne flagged that Saccade must not guess which real websites are Green, Red,
  or anything else. Site rules should be added only after real Saccade dogfood,
  a reference-browser comparison, a provider block, or primary-source
  high-impact evidence.
- Unknown third-party sites now classify as `unmeasured_unknown` Yellow instead
  of `public_or_unknown_low_risk` Green. Saccade may still assist after Human
  grant, but screenshots are not default-allowed and final side-effect actions
  remain Human-confirmed.
- This keeps local/owned pages fast while forcing real-site product claims to
  have receipts before they become docs or code.
- Evidence:
  `cargo test -p saccade_core site_policy`.

## DECISION_SERVOSHELL_020 - Official ServoShell bridge is the default dogfood browser path

- AI-012 is closed: Saccade's default human-visible browser path is installed
  official ServoShell plus the external Saccade bridge, not the legacy GL
  toolbar.
- This keeps the human browser close to upstream ServoShell while letting
  Saccade attach the agent layer: redacted truth/actions, safe fill/inspect/act,
  FORMMAX live fill, navigation, replay artifacts, current-tab grants, and
  Copilot state.
- Same-machine dogfood does not need a custom app bundle, icon, or renamed fork.
  A future public `.app` should use distinct Saccade branding and can say
  "Powered by Servo"; it should not reuse official Servo branding.
- Trusted visible Human/Agent/Copilot browser chrome remains AI-004. The bridge
  exposes the authoritative state through `shell_status`, but Saccade should not
  fake a trusted badge with page DOM injection, user CSS, or title mutation.
- Latest close evidence:
  `runs/servoshell_adapter/ai012_close_bridge_smoke_1781794791/report.json`.
- Verification commands:
  `cargo test -p saccade-servoshell` and `cargo check -p saccade-mcp`.

## DECISION_DEVMAX_021 - Browser-backed DEVMAX findings carry visual crops and action receipts

- AI-009 is closed for browser-backed DEVMAX reports. Servo probe audits now
  write a full-page screenshot, per-finding crop PNGs, and multi-action click
  receipts.
- Each browser-backed finding gets `evidence.screenshot_crop`; report artifacts
  include `browser_screenshot`, `finding_crops`, and `action_receipts`; replay
  includes `devmax_action_receipt` events.
- The Servo fixture selftest now enforces the product bar: every browser-backed
  finding must have a crop, and at least one fixture must verify multiple action
  receipts. `button_no_handler` carries two inert buttons to prove this path.
- HTTP status awareness for resource loads remains a separate DEVMAX follow-up,
  not part of AI-009.
- Evidence:
  `runs/devmax/servo_selftest_1781796265942/summary.json`.
- Verification commands:
  `cargo check -p devmax`,
  `cargo run -q -p devmax -- selftest-servo-fixtures`, and
  `cargo run -q -p devmax -- selftest-fixtures`.

## DECISION_SERVOSHELL_022 - Trusted Copilot badge belongs in source-fork browser chrome

- AI-004 is closed for the first thin-fork product UI pass. The source
  ServoShell fork at
  `/Users/waynema/Documents/GitHub/servo-saccade-upstream` now draws a Saccade
  Human/Copilot/blocked/error badge in the egui toolbar, next to the address
  bar and outside webpage content.
- The badge reads bridge-compatible Copilot JSON from
  `SACCADE_COPILOT_STATUS_PATH` or direct env vars. It accepts the existing
  `copilot.status`, `owner`, `read_grant`, `agent_input_grant`,
  `user_confirmation_required_for_side_effects`,
  `sensitive_values_exposed_to_agent`, and `page_dom_injected` fields.
- Safety rule: if the status says sensitive values are exposed to the agent or
  page DOM was injected, the badge forces an `AI Error` state. A webpage cannot
  earn a trusted badge by changing DOM, CSS, or title text.
- `saccade-servoshell` now writes a per-launch Copilot status file under the
  system temp dir and passes `SACCADE_COPILOT_STATUS_PATH` to ServoShell. The
  installed official ServoShell ignores the env var; the thin fork displays it.
  Bridge reports and control artifacts record the status file path.
- Codex's macOS `screencapture` path produced black screenshots in this session,
  so the thin fork now supports explicit one-shot internal browser-chrome
  screenshots through `SACCADE_BROWSER_SCREENSHOT_PATH`. This reads the final
  window framebuffer after egui/browser chrome paint, captures the trusted badge,
  and avoids macOS screen-recording permission as a verifier dependency.
- Screenshot safety rule: this path is opt-in only. Saccade does not set it by
  default for bridge/dogfood runs, because real logged-in or sensitive pages
  remain screenshot-forbidden unless a human explicitly chooses diagnostic
  capture.
- Evidence:
  `runs/ai004_badge/bridge_smoke/report.json` and
  `runs/ai004_badge/internal_browser_chrome.png`.
- Verification commands:
  `cargo test -p servoshell saccade_copilot_badge`,
  `cargo check -p servoshell`,
  `cargo build -p servoshell --bin servoshell`, and
  `cargo check -p saccade-servoshell`.

## DECISION_SERVOSHELL_023 - Stop button unblocks in the source-fork browser path

- AI-001 is closed for the source ServoShell thin fork. The toolbar Stop button
  no longer logs "Do not support stop yet"; it queues a `Stop` UI command for
  the active WebView.
- The command evaluates `window.stop()` in the active page. In Servo this maps
  to the HTML navigation stop path and sends `ScriptToConstellationMessage::
  AbortLoadUrl` for in-progress navigations, so this is a real Servo abort path
  rather than a reload/no-op fallback.
- This does not change the earlier pinned `servo=0.2.0` public API finding:
  the old crate still exposes no public `stop_loading` method. The unblock is
  specifically because the product browser path now has a deliberate
  source-fork chrome hook.
- Evidence:
  `/Users/waynema/Documents/GitHub/servo-saccade-upstream` commit pending at
  implementation time.
- Verification command:
  `cargo check -p servoshell`.

## DECISION_SERVOSHELL_024 - Public tutorial pages use one-shot article_text

- A Chrome-vs-Saccade dogfood run on The Rookies' modular environment art
  tutorial showed that the bridge could read the page, but long learning tasks
  needed a direct article/main text surface and a command that exits cleanly.
- `saccade-servoshell bridge` now supports `article_text` over the same
  control endpoint and exposes `--read-article --exit --json` for one-shot
  learning-page runs. The bridge explicitly navigates the WebDriver session to
  the requested URL after session creation, reducing the observed `about:blank`
  race.
- The dogfood kit now includes `read-article <URL>`, which writes a JSON report
  under the kit's `runs/article/` and exits instead of leaving a live bridge.
- Safety rule: `article_text` respects the existing site policy. Red sites do
  not return article text; public/low-risk learning pages can return text for
  DB ingestion and Chrome-vs-Saccade comparison.
- Evidence:
  `runs/dogfood_release/article_rookies_smoke_20260619/report.json` extracted
  9,392 chars from `main.layout-content` with the expected title and URL.
- Verification commands:
  `cargo fmt --check -p saccade-servoshell`,
  `cargo check -p saccade-servoshell`, and
  `bash -n scripts/build_dogfood_release.sh`.

## DECISION_DOGFOOD_025 - AI-014 dogfood kit closes on ServoShell bridge/fork runtime

- AI-014 is closed for default dogfood runtime migration: the current kit at
  `dist/saccade-dogfood-ai014-close-20260619/` builds only `saccade-mcp` and
  `saccade-servoshell` by default, and omits legacy `bin/saccade-shell`.
- The legacy embedded Servo 0.2 shell remains opt-in only through
  `SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh`.
- Product gates now have fresh evidence through the ServoShell bridge/fork
  path: bridge smoke passed, one-shot `read-article` extracted 9,392 chars from
  The Rookies page, and the local-game reflex wrapper passed
  `live_game_reflex_readback_green`.
- The article gate still records known Servo page warnings such as macOS GL
  texture warnings and missing `IntersectionObserver`; these are tolerated for
  article truth as long as the report exits cleanly with correct URL/title/text.
- Evidence:
  `runs/dogfood_release/ai014_close_bridge_smoke_20260619/report.json`,
  `dist/saccade-dogfood-ai014-close-20260619/runs/article/ai014_close_rookies_article/control/report.json`,
  and `runs/local_game_reflex/ai014_close_reflex_smoke_20260619/report.json`.
- Verification commands:
  `./scripts/build_dogfood_release.sh dist/saccade-dogfood-ai014-close-20260619`,
  `cargo test -p saccade-servoshell --quiet`,
  `cargo check -p saccade-mcp --quiet`,
  `dist/saccade-dogfood-ai014-close-20260619/servoshell-bridge --smoke --output-dir runs/dogfood_release/ai014_close_bridge_smoke_20260619`,
  `dist/saccade-dogfood-ai014-close-20260619/read-article https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art ai014_close_rookies_article`,
  and
  `SACCADE_REFLEX_DURATION_MS=5000 SACCADE_REFLEX_FACT_INTERVAL_MS=500 dist/saccade-dogfood-ai014-close-20260619/run-local-game-reflex http://127.0.0.1:4173/ ai014_close_reflex_smoke_20260619`.

## DECISION_SERVOSHELL_026 - Logged-in Gist editor detection runs through the official bridge

- AI-005 is closed for real logged-in editor detection on the ServoShell bridge
  path. The bridge now exposes `inspect_editors` as a loopback control method
  and `saccade-servoshell bridge --inspect-editors --exit --json` as a
  one-shot probe.
- The probe reports editor metadata, geometry, route counts, and sensitivity
  class only. It does not return editor text values.
- Live dogfood evidence: Wayne logged into GitHub/Gist in the visible Saccade
  window. Saccade navigated the same session from
  `https://gist.github.com/starred` to `https://gist.github.com/new` and
  inspected the authenticated editor page.
- Result:
  `route.decision=usable_ignore_hidden_backing_fields`, `editor_count=7`,
  `zero_rect_count=2`, `visible_writable_count=5`,
  `visible_authoring_count=4`, and `sensitive_count=0`. The hidden GitHub
  backing textarea is present, but the visible CodeMirror/contenteditable
  surface is also present and routable.
- During the same session Wayne observed a separate browser product issue:
  after growing and shrinking the window, GitHub's account dropdown could be
  clipped and bridge truth showed some menu/nav rects outside the viewport
  (`1065x684@2x`). This is tracked separately as AI-015; it should not be
  mixed with the editor-detection result.
- Evidence:
  `runs/servoshell_editor/gist_live_20260619/control/replay.jsonl` and
  `runs/servoshell_editor/fixture_inspect_verify_20260619/report.json`.
- Verification commands:
  `cargo fmt --check -p saccade-servoshell`,
  `cargo check -p saccade-servoshell --quiet`, and the fixture one-shot:
  `RUST_LOG=error cargo run -q -p saccade-servoshell -- bridge --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/editor_reduction/index.html --inspect-editors --exit --json --output-dir runs/servoshell_editor/fixture_inspect_verify_20260619`.

## DECISION_SERVOSHELL_027 - Draft editor fill is narrow and value-redacted

- AI-005B has a local bridge pass. The official ServoShell bridge now exposes
  `draft_editor_fill` for visible draft authoring fields only: `description`,
  `filename`, and `body`.
- The method requires `block_sensitive=true` and `no_submit=true`; it never
  clicks Create/Publish/Submit and defaults to preserving user-entered values.
  A second fill attempt on the same fixture returns `already_has_user_value`
  instead of overwriting.
- Replay/report summaries record counts, slots, target hashes, lengths, and
  policy, but not draft text values. Grepping the run artifacts for both the
  inserted draft sentinels and hidden backing-field sentinel returned no
  matches.
- `saccade-servoshell bridge` now also accepts `--profile-dir`. Without it,
  bridge launches still use `--temporary-storage`; with it, Saccade creates the
  directory and passes it as ServoShell `--config-dir=...`. The dogfood release
  wrappers now default to the stable `runs/dogfood_profile/default` via
  `SACCADE_PROFILE_DIR`, so login can survive rebuilding the local kit.
- Live GitHub/Gist draft fill remains pending human login in the profile-backed
  window; the pre-login route correctly reports only the Search Gists field and
  blocks authoring fill.
- Evidence:
  `runs/servoshell_editor/fixture_draft_fill_clean_20260619/control/replay.jsonl`,
  `runs/servoshell_editor/profile_dir_smoke_20260619/report.json`, and
  `runs/servoshell_editor/gist_draft_fill_profile_live_20260619/control/replay.jsonl`.
- Verification commands:
  `cargo fmt --check -p saccade-servoshell`,
  `cargo check -p saccade-servoshell --quiet`,
  `cargo test -p saccade-servoshell --quiet`,
  `cargo run -q -p saccade-servoshell -- bridge --help`,
  and the profile-dir fixture smoke:
  `RUST_LOG=error cargo run -q -p saccade-servoshell -- bridge --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/editor_reduction/index.html --profile-dir runs/dogfood_profile/default --inspect-editors --exit --json --output-dir runs/servoshell_editor/profile_dir_smoke_20260619`.

## DECISION_SERVOSHELL_028 - Gist draft fill needs editor-chrome and auth-state gates

- Live Gist draft fill exposed two separate issues that should not be merged.
  First, the real GitHub editor can expose empty CodeMirror/editor chrome as a
  small nonzero text length. The first live draft fill wrote description and
  filename, but rejected body with `already_has_user_value` and
  `beforeLength=2`. The bridge now reads/writes CodeMirror through
  `getValue`/`setValue` when available and treats pure editor line-number
  chrome as empty fallback text.
- Local regression evidence now fills all three draft slots on the editor
  reduction fixture: `draft_fields_filled=3`, `draft_fields_rejected=0`,
  `chars_written=113`, then a second fill preserves existing values with
  `draft_fields_filled=0` and `draft_fields_rejected=3`. Grepping the fixture
  and live artifacts for the inserted draft strings returned no matches.
- Second, authenticated real-site fill is still not closed. After bridge
  restart, the `runs/dogfood_profile/default` cookie jar contains GitHub cookie
  names `_gh_sess`, `_octo`, and `logged_in`, but no authenticated session
  cookie evidence. The restarted profile-backed bridge reaches logged-out
  `https://gist.github.com/starred` with `Sign in`/`Sign up` actions, not the
  new-Gist authoring editor. AI-005B remains blocked on a fresh same-process
  human login, and AI-005C tracks whether cross-restart profile persistence is
  a product requirement or a documented same-process-only limitation.
- Bridge navigation now records a verified navigation outcome. If WebDriver
  `POST /url` returns success but the URL stays unchanged, Saccade attempts a
  same-page `window.location.assign` fallback and records that evidence. This
  is a local control-truth guard, not a bypass: site redirects, login pages, and
  provider blocks are still treated as the resulting page state.
- The profile/account dropdown clipping Wayne saw remains AI-015. It is a
  browser layout/resize product bug, not an editor-fill or safety-policy bug.
- Evidence:
  `runs/servoshell_editor/gist_draft_fill_profile_live2_20260619/control/replay.jsonl`,
  `runs/servoshell_editor/gist_draft_fill_profile_live4_20260619/report.json`,
  `runs/servoshell_editor/fixture_draft_fill_codemirror_fix_20260619/control/replay.jsonl`,
  and `runs/servoshell_editor/fixture_draft_fill_codemirror_fix_20260619/control/report.json`.
- Verification commands:
  `cargo fmt --check -p saccade-servoshell`,
  `cargo check -p saccade-servoshell --quiet`,
  `cargo build -p saccade-servoshell --quiet`, and
  `rg -n "Saccade live dogfood|Saccade dogfood draft|saccade-dogfood-draft|SECOND DRAFT|SECOND BODY|Fixture dogfood|Fixture body|fixture-draft" runs/servoshell_editor/gist_draft_fill_profile_live2_20260619 runs/servoshell_editor/fixture_draft_fill_codemirror_fix_20260619 || true`.

## DECISION_SERVOSHELL_029 - Headed window resize decoration axes fixed

- AI-015 found a concrete source ServoShell headed-window bug while
  investigating the GitHub profile dropdown clipping report. In
  `ports/servoshell/desktop/headed_window.rs`, `request_resize` computed
  decoration width/height as `(outer.height - inner.height,
  outer.width - inner.width)`, swapping the axes before translating WebDriver
  outer rect requests into inner window sizes.
- The source fork fix changes this to `(outer.width - inner.width,
  outer.height - inner.height)` and the release ServoShell binary was rebuilt.
- Runtime headed WebDriver probe on the rebuilt source-release ServoShell
  opened the local browser-session fixture, set window rects
  `900x700 -> 1200x740 -> 900x700`, and observed matching WebDriver
  `window/rect` results plus page truth `outerWidth` and `innerWidth` equal to
  the requested widths. Heights also matched for sizes inside the current
  screen bounds; an earlier `1280x900` request was capped to `793` high by the
  macOS display, which is a display-boundary route rather than a resize math
  failure.
- This is a product-window geometry fix, not a complete GitHub dropdown claim.
  The real profile/account menu still needs a post-fix retest after Wayne logs
  in or a local dropdown fixture reproduces the same clipping pattern.
- Verification commands:
  in `/Users/waynema/Documents/GitHub/servo-saccade-upstream`,
  `cargo fmt --check -p servoshell`,
  `cargo check -p servoshell`,
  `cargo build -p servoshell --bin servoshell --release`, and
  `git diff --check`; runtime probe used
  `/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell`
  with WebDriver `GET/POST /window/rect` against
  `test_pages/browser_session/index.html`.

## DECISION_SERVOSHELL_030 - Live authenticated Gist draft fill passed

- AI-005B is closed. Wayne completed GitHub login/2FA in the visible
  Saccade/ServoShell window, and Saccade did not inspect password/OTP fields or
  capture screenshots on the logged-in page.
- The same bridge session moved from `https://gist.github.com/starred` to the
  real new-Gist editor at `https://gist.github.com/new`. `inspect_editors`
  reported `usable_ignore_hidden_backing_fields` with `editor_count=7`,
  `zero_rect_count=2`, `visible_writable_count=5`,
  `visible_authoring_count=4`, and `sensitive_count=0`.
- `draft_editor_fill` filled exactly the three draft authoring fields:
  description, filename, and body. The body path used `codemirror_set_value`.
  Hidden backing fields were ignored, and no Create/Publish/Submit action was
  attempted.
- A second fill attempt with different text rejected all three fields as
  `already_has_user_value`, proving the bridge preserves user/agent-visible
  existing draft text instead of overwriting it by default.
- Artifact leak check found no inserted draft text in report/replay files.
  Replay/report artifacts record methods, lengths, receipts, and policy
  results, not draft values.
- AI-005C remains open as a separate product decision: whether authenticated
  real-site dogfood must persist across bridge restarts or be documented as a
  same-process human-login handoff until profile persistence is fixed.
- Evidence:
  `runs/servoshell_editor/gist_ai005b_live_20260619_192956/control/replay.jsonl`,
  `runs/servoshell_editor/gist_ai005b_live_20260619_192956/control/report.json`,
  and `runs/servoshell_editor/gist_ai005b_live_20260619_192956/report.json`.
- Verification commands:
  `rg -n "Saccade AI-005B|live dogfood draft|saccade-ai005b|SECOND SHOULD|SECOND BODY|Hidden backing fields|No Create|Publish" runs/servoshell_editor/gist_ai005b_live_20260619_192956 || true`
  returned no matches before this decision text was written.

## DECISION_SERVOSHELL_031 - Dropdown resize fixture narrows AI-015

- The remaining browser UI/layout complaint is still real, but the first
  minimized route is green. Added `test_pages/dropdown_resize/index.html`, a
  local page with a right-aligned account button and a profile/logout dropdown,
  plus `scripts/probe_servoshell_dropdown_resize.py`.
- The probe launches headed ServoShell with WebDriver, sets outer window rects
  `900x700 -> 1200x740 -> 900x700`, opens the dropdown at each size, records JS
  viewport and menu geometry, captures non-sensitive fixture screenshots, and
  fails if the menu/logout escapes either the JS viewport or screenshot-derived
  CSS width.
- Source-release ServoShell passed the gate:
  `runs/servoshell_ui/dropdown_resize_ai015_20260619/report.json`. The final
  shrink phase reported `innerWidth=900`, `documentClientWidth=900`, menu
  `right=882`, `menuWithinViewport=true`,
  `menuWithinScreenshotCssWidth=true`, and `logoutVisible=true`.
- Official `/Applications/Servo.app` also passed:
  `runs/servoshell_ui/dropdown_resize_official_20260619/report.json`. Its
  WebView width is smaller because of official browser chrome, but the menu
  still remained visible after grow/shrink.
- This does not close AI-015. It rules out a broad "all right-edge dropdowns
  fail after resize" shell geometry bug. The next evidence target is a logged-in
  GitHub/Gist account-menu geometry probe without screenshots or value capture.
- Verification commands:
  `python3 scripts/probe_servoshell_dropdown_resize.py --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --output-dir runs/servoshell_ui/dropdown_resize_ai015_20260619`,
  `python3 scripts/probe_servoshell_dropdown_resize.py --servoshell /Applications/Servo.app/Contents/MacOS/servoshell --output-dir runs/servoshell_ui/dropdown_resize_official_20260619`,
  `python3 -m py_compile scripts/probe_servoshell_dropdown_resize.py`, and
  `git diff --check`.

## DECISION_SERVOSHELL_032 - Real GitHub dropdown overflow reproduced safely

- Added `scripts/probe_github_dropdown_geometry.py`, a screenshot-free and
  value-free real-site geometry probe for logged-in GitHub/Gist account menus.
  The probe records only URL origin/path, viewport sizes, sanitized element
  paths, rects, overflow distances, click dispatch status, and hit-test
  booleans. It explicitly avoids screenshots, password/OTP reads, username or
  email capture, and menu text logging.
- The first iterations caught false positives: GitHub's cookie consent UI and
  2FA alternative-method controls looked like right-edge buttons. The probe now
  disqualifies cookie/consent/auth/two-factor/session controls and waits for an
  actual profile/avatar candidate before measuring.
- Source-release ServoShell with Wayne's same-process login reproduced the
  remaining AI-015 bug on `https://gist.github.com/starred`:
  `runs/servoshell_ui/github_dropdown_live_wait3_20260619/report.json`.
  The account/profile button was present, Sign out existed and was hit-testable,
  but the menu opened to the right of the avatar and overflowed the viewport
  horizontally by `152px` at `900x700`, `176px` at `1200x740`, and `152px` again
  after shrinking back to `900x700`.
- This differs from the local fixture. `test_pages/dropdown_resize/` keeps its
  right-edge dropdown inside the viewport on both source-release ServoShell and
  official `/Applications/Servo.app`. The remaining issue is therefore not a
  broad window-geometry failure; it is GitHub/Primer overlay positioning or
  Servo web-compat.
- Official `/Applications/Servo.app` could not be compared with the same
  authenticated profile in this run. It reached logged-out `/starred` and
  returned `auth_required`:
  `runs/servoshell_ui/github_dropdown_official_profile_20260619/report.json`.
- Servo stderr during the source-release GitHub run included web-compat signals
  around `adoptedStyleSheets`, `IntersectionObserver`, and a GitHub chunk error
  involving `getItemById`. These are evidence to investigate, not yet a proven
  root cause.
- Verification commands:
  `python3 scripts/probe_github_dropdown_geometry.py --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --profile-dir runs/dogfood_profile/default --wait-for-auth-sec 240 --output-dir runs/servoshell_ui/github_dropdown_live_wait3_20260619`,
  `python3 scripts/probe_github_dropdown_geometry.py --servoshell /Applications/Servo.app/Contents/MacOS/servoshell --profile-dir runs/dogfood_profile/default --wait-for-auth-sec 60 --output-dir runs/servoshell_ui/github_dropdown_official_profile_20260619`,
  `python3 -m py_compile scripts/probe_github_dropdown_geometry.py`, and
  `git diff --check`.

## DECISION_SERVOSHELL_033 - ServoShell bridge profile persistence flush uses official shutdown route

- AI-005C is closed for the local profile persistence primitive. The issue was
  not a generic cookie parser failure: source-release ServoShell and official
  Servo.app both write and reuse local profile cookies when the correct clean
  shutdown route is used.
- The official Servo WebDriver extension route is
  `DELETE /session/{id}/servo/shutdown`. Saccade had been using `POST` in the
  probe and was also deleting the WebDriver session / SIGTERMing the child in
  bridge shutdown paths, which can skip Servo's clean resource-thread exit and
  `cookie_jar.json` flush.
- `saccade-servoshell bridge` now calls the official `DELETE` shutdown route,
  waits up to 12 seconds for `graceful_servo_shutdown`, records the shutdown
  response in the report, and only then falls back to `DELETE /session` +
  SIGTERM/SIGKILL.
- The comparison probe now covers three paths against one local HTTP fixture:
  source-release ServoShell direct, official Servo.app direct, and Saccade
  bridge. The passing report is
  `runs/profile_persistence/ai005c_delete_shutdown_fix_20260619/report.json`;
  bridge evidence has `termination=graceful_servo_shutdown` and persistent
  local cookies visible in the second run.
- This proves Saccade profile flushing, not provider-specific login retention.
  GitHub/Google/Apple/AdMob may still require same-process handoff or fresh
  login depending on their own session-cookie, device-trust, and 2FA policy.
- Verification commands:
  `cargo check -p saccade-servoshell --quiet`,
  `cargo build -p saccade-servoshell --quiet`, and
  `python3 scripts/probe_servoshell_profile_persistence.py --output-dir runs/profile_persistence/ai005c_delete_shutdown_fix_20260619 --timeout-sec 20 --fixture-port 7805`.

## DECISION_SERVOSHELL_034 - GitHub dropdown is routed to Servo Web API compatibility

- AI-015 is routed, not fixed in Saccade shell geometry. The source fork's
  window-resize axis bug is already fixed, and the local right-edge dropdown
  fixture stays inside the viewport after `900x700 -> 1200x740 -> 900x700`.
- The remaining real GitHub/Gist failure requires GitHub's logged-in Primer
  menu stack. The same-process logged-in geometry probe showed the profile menu
  opening to the right of the avatar and overflowing horizontally by
  `152-176px`, while Sign out remained hit-testable:
  `runs/servoshell_ui/github_dropdown_live_wait3_20260619/report.json`.
- After AI-005C fixed clean profile shutdown, both source-release ServoShell and
  official Servo.app still reached logged-out/profile-not-found states with the
  persisted profile, so cross-restart GitHub login retention remains provider
  policy rather than a local storage primitive blocker.
- The new structured API feature fields show the same missing APIs in both
  source-release and official Servo.app on GitHub:
  `intersectionObserver="undefined"`,
  `documentAdoptedStyleSheets="undefined"`, and
  `shadowRootPrototypeAdoptedStyleSheets=false`. Stderr also shows GitHub module
  errors for `adoptedStyleSheets` and `IntersectionObserver`.
- `scripts/probe_github_dropdown_geometry.py` and
  `scripts/probe_servoshell_dropdown_resize.py` now use Servo's clean
  `DELETE /session/{id}/servo/shutdown` route during teardown.
- Product fallback: do not claim GitHub account-menu visual parity. Use the
  same-process GitHub editor/form flows that have measured passes, and route
  GitHub account/logout UI to a normal browser until Servo implements the
  missing APIs or a measured safe polyfill/fork exists.
- Verification commands:
  `python3 scripts/probe_servoshell_dropdown_resize.py --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --output-dir runs/servoshell_ui/dropdown_resize_shutdown_clean_20260619 --port 7135`,
  `python3 scripts/probe_github_dropdown_geometry.py --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell --profile-dir runs/dogfood_profile/default --wait-for-auth-sec 12 --output-dir runs/servoshell_ui/github_dropdown_source_api_features_20260619 --port 7145`,
  `python3 scripts/probe_github_dropdown_geometry.py --servoshell /Applications/Servo.app/Contents/MacOS/servoshell --profile-dir runs/dogfood_profile/default --wait-for-auth-sec 12 --output-dir runs/servoshell_ui/github_dropdown_official_api_features_20260619 --port 7155`,
  `python3 -m py_compile scripts/probe_github_dropdown_geometry.py scripts/probe_servoshell_dropdown_resize.py`, and
  `git diff --check`.

## DECISION_DOGFOOD_035 - Local dogfood handoff uses the packaged ServoShell bridge kit

- AI-016 closes the same-machine dogfood packaging gate, not the public macOS
  distribution gate. Developer ID signing, notarization, staple verification,
  and app-bundle polish remain BP-013/later release work.
- `scripts/build_dogfood_release.sh` now builds a self-contained local kit with
  `check-saccade`, `open-saccade`, `servoshell-bridge`, `read-article`, and
  `run-local-game-reflex`. The kit records build metadata, copies the current
  tracker/safety docs, writes `DOGFOOD_STATUS.md`, and updates
  `dist/saccade-dogfood-current` when the output lives under `dist/`.
- The generated wrappers default to stable `SACCADE_PROFILE_DIR` for the browser
  profile, and package-local `current_tab_grant.json` / `runs/` paths unless the
  caller explicitly overrides them. This keeps login state across rebuilt kits
  while preventing other sessions from accidentally writing bridge grants and
  reports into stale repository defaults.
- `check-saccade` keeps stdout machine-readable JSON and sends human status
  lines to stderr. This lets another Codex session run
  `dist/saccade-dogfood-current/check-saccade | jq ...` directly.
- The final verified AI-016 kit is
  `dist/saccade-dogfood-ai016-20260619-204157/`; it omits
  `bin/saccade-shell`, proving the legacy embedded Servo 0.2 shell is not part
  of the default dogfood runtime.
- Verification commands:
  `bash -n scripts/build_dogfood_release.sh`,
  `./scripts/build_dogfood_release.sh dist/saccade-dogfood-ai016-20260619-204157`,
  `dist/saccade-dogfood-ai016-20260619-204157/check-saccade`,
  `dist/saccade-dogfood-ai016-20260619-204157/servoshell-bridge --smoke --json`,
  `dist/saccade-dogfood-ai016-20260619-204157/read-article https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art ai016_rookies_article_final`, and
  `test ! -e dist/saccade-dogfood-ai016-20260619-204157/bin/saccade-shell`.

## DECISION_BROWSER_036 - Browser profile belongs to the human, agent grants are scoped

- Saccade's product model separates browser ownership from agent capability.
  The normal browser profile is human-owned local browser state: cookies,
  site storage, device/session trust, and future history-like state persist for
  the human when the site permits it.
- The agent does not own or receive the profile. It attaches to a current tab or
  live session only after an explicit grant, and receives policy-redacted
  truth/actions/control receipts rather than raw cookies, password data,
  storage dumps, or sensitive field values.
- The local dogfood implementation now has the first normal-profile primitive:
  wrappers default to stable `runs/dogfood_profile/default` via
  `SACCADE_PROFILE_DIR`, so rebuilding the local kit does not create a fresh
  login profile each time.
- AI-021 tracks the remaining product UX: visible normal/incognito/named
  profile modes, a chrome-level profile/grant badge, and a user-confirmed
  clear-profile command.
- Dogfood wrapper incognito now exists for throwaway browsing through
  `SACCADE_INCOGNITO=1` or `SACCADE_PROFILE_MODE=incognito`. It uses a marked
  temporary profile under the kit's `runs/incognito/` directory and deletes it
  when the command exits.

## DECISION_BROWSER_037 - Dogfood wrappers expose normal and incognito profile modes

- `scripts/build_dogfood_release.sh` now writes a shared `lib/profile.sh` into
  each dogfood kit. `open-saccade`, `servoshell-bridge`, `check-saccade`,
  `read-article`, and optional `open-legacy-saccade` all resolve the effective
  profile through that helper.
- Normal mode is still the default and uses stable
  `runs/dogfood_profile/default`. Incognito mode is enabled with
  `SACCADE_INCOGNITO=1` or `SACCADE_PROFILE_MODE=incognito`, creates a marked
  temporary profile, exports `SACCADE_EFFECTIVE_PROFILE_MODE=incognito` and
  `SACCADE_EFFECTIVE_PROFILE_PERSISTENT=0`, and deletes the temporary profile on
  wrapper exit.
- `saccade-servoshell bridge` now records `profile_mode` and
  `profile_persistent` in its JSON report, so `check-saccade` and other sessions
  can verify whether they are using normal or incognito storage.
- Verification:
  `bash -n scripts/build_dogfood_release.sh`,
  `cargo check -p saccade-servoshell --quiet`,
  `git diff --check`,
  `./scripts/build_dogfood_release.sh`,
  `dist/saccade-dogfood-current/check-saccade`, and
  `SACCADE_INCOGNITO=1 dist/saccade-dogfood-current/check-saccade`.
- Evidence: current kit `dist/saccade-dogfood-20260622-151603`; normal check
  reported `profile_mode=normal`, `profile_persistent=true`,
  `profile_dir=/Users/waynema/Documents/GitHub/SACCADE/runs/dogfood_profile/default`,
  and `termination=graceful_servo_shutdown`. Incognito check reported
  `profile_mode=incognito`, `profile_persistent=false`,
  `termination=graceful_servo_shutdown`, and the temporary profile directory did
  not exist after command exit.

## DECISION_BROWSER_038 - Public government article lookup needs bounded fallback

- A USCIS public guidance dogfood run exposed a gap in Saccade's article
  extraction path. `dist/saccade-dogfood-current/read-article
  https://www.uscis.gov/forms/filing-guidance/form-i-797-types-and-functions
  uscis_i797_types` did not return useful JSON within about 75 seconds and was
  interrupted.
- Direct official-page HTTP fetches with a normal user agent succeeded quickly
  for the I-797, biometrics appointment, adjustment of status, and immediate
  relative USCIS pages. The issue is therefore not "official USCIS content
  inaccessible"; it is Saccade browser/article ready handling for this
  government Drupal/JS page class.
- Product route: public government reference lookup may use an official HTTP
  article fallback when Saccade browser extraction hangs. Government account
  actions, immigration filings, uploads, signatures, payments, case changes,
  and legally meaningful submissions remain human-only/red.
- Follow-up: add a bounded `read-article` timeout/ready failure reason and a
  first-class official-page fallback packet so dogfood reports do not silently
  hang on public reference pages.

## DECISION_BROWSER_039 - `read-article` uses browser-first, no-cookie HTTP fallback

- Implemented `scripts/read_article_fallback.py` and wired it into
  `scripts/build_dogfood_release.sh`. The dogfood `read-article` command now
  starts with the Saccade/ServoShell browser article path, but wraps it in a
  hard timeout. If that path exceeds
  `SACCADE_READ_ARTICLE_HARD_TIMEOUT_SEC`, exits nonzero, or returns invalid
  JSON, the wrapper kills the browser process group and emits
  `route=http_article_fallback`.
- Safety boundary: the fallback is for public reference pages only. It does not
  send browser cookies and does not use the persisted Saccade profile. It must
  not be used for logged-in account data, filings, uploads, signatures,
  payments, government forms, or other legally meaningful actions.
- Evidence: current kit `dist/saccade-dogfood-20260622-171928`. Forced USCIS
  timeout returned `ok=true`, `route=http_article_fallback`,
  `fallback_reason=saccade_browser_article_hard_timeout`,
  `cookies_sent=false`, `profile_used=false`, `text_chars=5620`,
  `has_i797c=true`, and `has_biometric=true`; report:
  `dist/saccade-dogfood-current/runs/article/uscis_i797_forced_fallback3/report.json`.
  Rookies regression still used the browser path with
  `runtime=saccade-servoshell-bridge-v0`, `chars=9392`, and no fallback.

## DECISION_BROWSER_040 - AI-021 closes profile/session UX for local dogfood

- AI-021 is closed for the local dogfood product gate. Saccade now has normal,
  named, and incognito profile wrappers; `profile-status`; safe CLI
  `clear-profile`; trusted browser-chrome Profile and Copilot badges; and an
  interactive browser-chrome profile panel.
- The profile panel shows mode, name, persistence, and the agent boundary. For
  normal named Saccade profiles it can write a user-confirmed
  `clear_profile_on_quit` request. The browser itself does not delete profile
  data; the dogfood wrapper applies the request after ServoShell exits.
- Safety boundary: the wrapper applies clear-on-quit only for normal persistent
  profiles under `SACCADE_PROFILE_ROOT/<profile_name>`, refuses custom
  `--profile-dir` action paths, preserves a `.saccade-profile.json` marker, and
  writes result counts/bytes without printing raw cookies, raw storage, or
  sensitive values.
- Product boundary: full in-browser profile switching remains a future
  relaunch/picker UX. Current switching is launch-time via
  `SACCADE_PROFILE_NAME`, which is safer than attempting to hot-swap storage
  under a live browser engine.
- Evidence: final kit `dist/saccade-dogfood-ai021-profile-final-20260705/`,
  report `docs/ai021_profile_productization_report.md`, and clear-on-quit
  summary `runs/ai021_profile_finalize/clear_on_quit_cleanup_final_20260705/summary.json`.

## DECISION_DOGFOOD_041 - Public site smoke matrix is a first-class dogfood gate

- Added `scripts/run_public_site_smoke_matrix.py` and packaged it in the
  dogfood kit as `run-public-site-smoke-matrix`.
- The tool runs low-risk public URLs sequentially through the ServoShell bridge,
  collects same-WebView smoke truth, optionally extracts article text, and
  writes per-site stdout/stderr/grant/control artifacts plus an aggregate
  report.
- Boundary: this is a no-login read/article smoke gate. It does not fill forms,
  submit, post, delete, pay, sign, bypass provider controls, or claim Chrome
  visual parity.
- First run passed 4/4 public sites: example.com, Hacker News, Wikipedia's Servo
  page, and The Rookies modular environment tutorial. Evidence:
  `runs/ai023_public_site_matrix/default_20260705/report.json` and
  `docs/ai023_public_site_smoke_matrix.md`.

## DECISION_DOGFOOD_042 - Public-read matrices are reusable release artifacts

- Added `site_matrices/public_core.json` and
  `site_matrices/public_extended.json`.
- `scripts/run_public_site_smoke_matrix.py` now accepts `--matrix`, resolves
  named matrices from the repo or dogfood kit, and supports `required=false`
  exploratory sites. Required failures fail the aggregate report; optional
  failures are recorded but do not fail the required public-read gate.
- The dogfood release builder copies public matrix files into the kit so other
  sessions can run `run-public-site-smoke-matrix extended --matrix extended`.
- First extended run passed 8/8 read-only public sites, including public GitHub,
  Gist, Stack Overflow, and Reddit pages. Evidence:
  `runs/ai024_public_site_matrix/extended_20260705/report.json` and
  `docs/ai024_public_site_matrix_expansion.md`.
- Boundary: this is read-only public coverage. It does not prove logged-in
  drafting, rich-editor compatibility, posting, submitting, visual parity, or
  provider automation acceptance.

## DECISION_DOGFOOD_043 - Live draft profiles map user-facing fields to narrow bridge slots

- Added draft profiles to `scripts/run_ai020_live_draft.py`: `raw`, `gist`,
  `generic_body`, `hn_comment`, `discourse_reply`, `reddit_comment`,
  `github_issue`, and `github_discussion`.
- Profiles map user-facing fields such as `title` and `comment` onto the
  existing safe bridge slots `description`, `filename`, and `body`; they do not
  permit arbitrary form filling.
- Added `test_pages/issue_draft/index.html` as a local issue-style fixture with
  title, body, password, search, and submit controls.
- Local verification passed for `github_issue` title/body fill and
  `local_forum` comment/body regression. Both runs left submit untouched and
  found no draft values in report/replay artifacts. Evidence:
  `runs/ai025_live_draft_profiles/local_issue_fixture_20260706/report.json`,
  `runs/ai025_live_draft_profiles/local_forum_regression_20260706/report.json`,
  and `docs/ai025_live_draft_profiles.md`.
- Boundary: this is a local profile/product gate. Real logged-in GitHub
  issue/discussion, Discourse, and Reddit draft flows still need visible
  human-in-loop site measurements before being claimed.

## DECISION_DOGFOOD_044 - Live draft fill requires target-page prefill gates

- A visible GitHub dogfood attempt exposed a harness bug: if `--manual-gate`
  ran without a real interactive stdin, EOF could let the run continue, and
  `github_issue` could partially fill GitHub Dashboard's Copilot textarea while
  still reporting success.
- `scripts/run_ai020_live_draft.py` now treats manual-gate EOF as fatal before
  fill.
- `github_issue` and `github_discussion` profiles now have default URL prefill
  gates for GitHub new issue/discussion paths. Local fixture URLs are exempt.
- Those profiles also require all requested slots to be filled before the run is
  considered green.
- Evidence: `docs/ai026_live_github_issue_gate_hardening.md`,
  `runs/ai026_live_github_issue/manual_gate_eof_regression_20260706/report.json`,
  `runs/ai026_live_github_issue/example_prefill_gate_20260706/report.json`, and
  `runs/ai026_live_github_issue/local_issue_prefill_gate_positive_20260706/report.json`.
  The invalid live attempt remains recorded at
  `runs/ai026_live_github_issue/github_issue_visible_20260706/report.json`.

## DECISION_DEMO_003 - Original MouseAccuracy works through release/headed ServoShell

- Added `scripts/probe_mouseaccuracy_original_gate.py` as a narrow real-site
  diagnostic gate for MouseAccuracy original pages.
- The classic gate opened `https://mouseaccuracy.com/classic/` in a visible
  release ServoShell window, selected Epic/Tiny, started the game, clicked real
  `.target` elements through WebDriver pointer actions, and moved score from
  `0` to `8`.
- The modern gate opened `https://mouseaccuracy.com/game` in a visible release
  ServoShell window, completed countdown, observed one canvas plus DOM `.target`
  facts, clicked target rectangles through WebDriver pointer actions, and moved
  score from `0` to `12`.
- The recurring `GLD_TEXTURE_INDEX_2D` warning still appears, but it did not
  block original-site MouseAccuracy rendering or clicks in these gates.
- Boundary: this closes the original-site viability question, not the full
  30-second highest-difficulty launch benchmark. Full public benchmark evidence
  should be a separate AI-029 run.
- Evidence: `docs/ai028_mouseaccuracy_original_gate.md`,
  `runs/ai028_mouseaccuracy_original/classic_gate_headed_release_2/report.json`,
  and
  `runs/ai028_mouseaccuracy_original/modern_game_headed_release_3/report.json`.

## DECISION_DOGFOOD_045 - Hacker News submit is a human-in-loop draft target

- Hacker News submit is a useful public dogfood target because it is a real
  posting surface, but Saccade must not publish from automation.
- Added `hn_submit` to the live draft harness. It maps user-facing `title`,
  `url`, and `text`/`body` onto the existing safe draft slots:
  `description`, `filename`, and `body`.
- The profile has a default prefill gate requiring the real HN submit URL and
  requires all requested slots to be filled before returning green.
- `saccade-servoshell` now treats visible URL fields as authoring candidates for
  this draft path and lets the `filename` slot target `name=url` controls.
- Release-source and packaged dogfood wrapper runs filled all three HN submit
  fields with no rejected slots, did not click submit, and passed value-leak
  checks on report/replay artifacts.
- Evidence: `docs/ai029_hn_submit_dogfood.md`,
  `runs/ai029_hn_dogfood/hn_submit_live_draft_release/report.json`, and
  `runs/ai029_hn_dogfood/hn_submit_packaged_wrapper/report.json`.

## DECISION_BROWSER_046 - Protected-site compatibility is explicit engine routing

- Servo remains Saccade's default browser and owns the low-latency reflex proof.
- Removing WebDriver did not clear Game UI Database's Cloudflare Turnstile
  page, and the later module Blob lifetime claim did not clear it either.
- Fresh headless Chrome was also blocked. A visible Chrome session using a
  dedicated persistent compatibility profile passed twice and kept
  `navigator.webdriver=false` without UA overrides or challenge automation.
- Therefore Saccade will use an explicit compatibility engine for measured
  Servo blockers. It must not present that mode as Servo or hide the active
  engine from the user.
- Compatibility truth preserves the Saccade boundary: no cookie/storage export,
  no sensitive values, no automatic CAPTCHA action, and stale truth is removed
  during navigation or provider challenge.
- AI-030A provides live redacted truth. Full MCP grants, verified actions,
  replay, FORMMAX, and trusted engine chrome are AI-030B.
- Evidence: `docs/ai030_cloudflare_compatibility_route.md` and
  `runs/chrome_compat/gameuidatabase_follow_check/report.json`.

## DECISION_BROWSER_047 - Chrome compatibility control is an explicit current-tab grant

- The headed Chrome compatibility route now exposes an engine-neutral loopback
  current-tab endpoint only when the Human explicitly requests a grant.
- MCP routes redacted truth/actions, low-risk browser-input act, navigation,
  replay, strict agent-owned normal-field fill, and value-free inspection to
  that same visible Chrome tab.
- Fill is intentionally conservative: only visible controls marked agent-owned
  are eligible; sensitive, human-owned, file/hidden, and already-nonempty
  controls are rejected. This does not claim generic third-party form fill.
- The local fixture proved a normal agent-owned fill with no value in artifacts,
  sensitive SSN rejection with no literal in artifacts, and preservation of a
  nonempty field. The real Game UI Database page attached through the same
  bridge without an unmeasured click.
- Evidence: `docs/ai030_cloudflare_compatibility_route.md`,
  `runs/chrome_compat_mcp/ai030b_fill_note/report.json`, and
  `runs/chrome_compat_mcp/ai030b_gameuidatabase_attach/report.json`.

## DECISION_PRODUCT_048 - FORMMAX and DOCMAX are the product wedge

- Saccade's product focus is fast web/PDF form completion with user-owned
  sensitive fields, signatures, legal attestations, payments, and final submit.
- Browser UI, Servo compatibility, Chrome routing, MCP, truth, and replay are
  supporting layers. New browser work must unblock this workflow or a measured
  safety/reliability gate.
- PDF support starts with AcroForm inspect/fill/verify. Flat, scanned, encrypted,
  signed, and XFA documents must be classified and routed without false claims.
- The existing PDF feasibility artifact now records only field names, counts,
  classifications, and completion status. It no longer records even synthetic
  ordinary field values.
- Product plan: `docs/SACCADE_FORMS_PDF_FOCUS_PLAN_20260711.md`. First redacted
  gate: `runs/formmax/pdf_feasibility/result.json`.

## DECISION_PRODUCT_049 - Pain evidence orders the product backlog

- The ranked pain ledger is `docs/SACCADE_PAIN_LEDGER_20260711.md`.
- Development priority uses user cost, agent blockage, Saccade leverage, and
  evidence confidence. The score orders work; it is not a prevalence estimate.
- The first three targets are long multi-page forms, mixed sensitive/ordinary
  fields, and verified recovery from partial failure.
- Prompt-injection enforcement remains P0 because its impact is severe even
  though public prevalence data is weak.
- CAPTCHA is a measured blocker with low Saccade leverage. Route it to the user;
  do not spend the product cycle on bypass or stealth behavior.
- External statistics establish the problem. Thirty observed first-party tasks
  with at least ten users must establish that Saccade solves it.

## DECISION_FORMMAX_050 - Generic planning precedes generic writing

- The official ServoShell bridge owns the first generic FORMMAX layer. Do not
  add a parallel form crate while the existing bridge can enforce the same
  current-tab policy.
- `form_inventory` returns field identity, semantics, owner, sensitivity, and
  redacted state without values. `form_compile_plan` returns only eligible and
  rejected field metadata and performs no writes.
- Unknown-owner ordinary empty fields may enter a plan only because the caller
  explicitly assigned that field; they are reported as `explicit_plan` rather
  than implying separate user approval.
  Human-owned, sensitive, existing, ambiguous, hidden, disabled, unsupported,
  and unstable fields remain blocked.
- Page revision is a hard plan boundary. A stale plan must be recompiled.
- The 17-control adversarial gate and the 672-field regression pass. Evidence:
  `docs/ai031_generic_form_plan.md`.

## DECISION_FORMMAX_051 - Execute only an unchanged compiled plan

- `form_execute_plan` requires the original page revision, expected plan ID, and
  explicit `block_sensitive`, `preserve_existing`, and `no_submit` policy.
- Execution re-inventories and recompiles before writing. A changed page or plan
  is rejected instead of receiving best-effort writes.
- Every successful write has an internal value postcondition, but receipts expose
  only field ID, method, and status. Existing values are compared before/after
  inside the browser and reported only as `preserved_verified`.
- The engine-neutral MCP surface exposes inventory, compile, and execute only for
  an explicitly granted current ServoShell tab with advertised capabilities.
- MCP control requests now half-close their write side after the JSONL request.
  Without this framing, macOS reset compile requests before the bridge could read
  them even though smaller inventory requests succeeded.
- Local evidence is 6/6 filled, 4/4 existing values preserved, zero failed or
  repair items, verified receipt, and zero sentinel leaks. Broad third-party
  compatibility remains unclaimed.

## DECISION_FORMMAX_052 - A write attempt invalidates the plan even when verification fails

- Page scripts may normalize or reject a value after assignment. The executor
  counts attempted mutations separately from verified fills and advances the
  page revision after any write attempt.
- `postcondition_mismatch` routes to `human_review_or_remap`, not automatic
  retry. Disappeared fields recompile; invalid option/type requests require a
  corrected plan; existing-value mutation stops and hands off.
- Remote current-tab execution requires an explicit trusted browser artifact.
  Official ServoShell artifacts must have exact runtime/rendering/transport
  metadata, prove no page DOM injection or sensitive-value exposure, and point
  to a loopback same-WebView endpoint. Direct remote URL grants stay blocked.
- Local mismatch evidence and two public no-submit test-form passes are recorded
  in `docs/ai031_generic_form_plan.md`. This does not authorize arbitrary sites
  or final submission.

## DECISION_BROWSER_053 - GitHub New Issue is a compatibility canary, not a forced Servo form target

- In a logged-in release-browser dogfood run, direct navigation reached a real
  GitHub `New Issue` page, while the Saccade form truth saw 22 controls but zero
  visible authoring editors and zero eligible fields. Eight editor candidates
  had zero rectangles.
- The Dashboard `Create issue` control was also semantically different from a
  normal form route: it opened GitHub Copilot's `/create-issue` command surface,
  which showed GitHub's own send-message error. Saccade did not send a Copilot
  request, write a form field, or create an Issue.
- The generic planner correctly refused hidden tokens and backing controls. A
  selector workaround would weaken the safety boundary because it could target
  a field the user cannot see.
- Product route: keep GitHub New Issue as a P1 Servo compatibility canary and
  route currently measured issue-draft workflows to explicit Chrome
  compatibility. The Saccade current-tab grant, redacted truth, policy,
  verified actions, and value-free replay remain engine-neutral.
- Evidence: `docs/ai027_github_ui_canary.md` and `BP-024` in
  `docs/browser_compat_ledger.md`.

## DECISION_ENGINE_054 - Saccade is the truth/action contract, not one rendering engine

- A bounded Chrome/CDP proof passed three independent 100-target runs with
  300/300 hits, zero misses, zero protected-value leaks, and combined full-loop
  latency of 3.8 ms p95.
- The host motor begins dispatch in 0.024 ms p95. Rare 24-64 ms outliers occur
  primarily while CDP carries renderer truth to the host, so DevTools is not
  the intended strict reflex transport.
- Product architecture should keep the engine-neutral grant, redaction,
  policy, action, verification, and replay contract. Servo and Chromium/CEF are
  adapters behind that contract.
- Before Chromium becomes the compatibility-first human browser engine, a CEF
  renderer/browser IPC implementation must pass the same 3x100 gate without a
  CDP dependency. Servo remains the deeper engine/research path.
- Evidence: `docs/chrome_engine_truth_reflex_poc.md`.

## DECISION_ENGINE_055 - Ship direct CEF first, preserve adapters

- Saccade will use an official CEF binary distribution directly as its default
  human-facing product engine. It will not build Chromium from source or make a
  community Rust CEF wrapper a critical dependency.
- The CEF browser/helper layer stays thin C++/Objective-C++ around the official
  C/C++ API. Rust owns the versioned engine adapter schema, grants, policy,
  redaction classification, verification, replay, and MCP host contract.
- CEF facts are labeled `cef_renderer_observed` until a stronger native engine
  source is implemented. Page semantics remain untrusted; renderer facts cannot
  grant authority or confirm consequential actions.
- AI-036 is the only migration implementation mainline. Existing FORMMAX,
  safety, agreement, WebGL/game, profile, and GitHub work become parity gates.
- Servo remains an explicit research adapter. Chrome/CDP remains a reference
  and test adapter and is prohibited from the production reflex loop.
- Old builds, evidence, profiles, and source paths are retired only through the
  evidence-aware sequence in `docs/CEF_MIGRATION_AND_CLEANUP_PLAN.md`.

## DECISION_ENGINE_056 - Preserve the official CEF macOS lifecycle for Day 1

- The pinned official standard CEF `cefsimple` target launches, renders, and
  quits cleanly when its outer app bundle is branded Saccade. Several custom
  target/package variants rendered but hung in `CefShutdown()` after their
  helper processes exited.
- Day 1 therefore stages the official app target and retains upstream internal
  executable/helper names. The custom CEF host begins only when Day 2 needs the
  engine adapter, and every host change must retain the clean-shutdown gate.
- Named Chromium `user-data-dir` paths passed normal persistence and disposable
  incognito isolation. Setting CEF `cache_path` or `root_cache_path` in the
  sample triggered a macOS Keychain shutdown hang, so those experimental
  patches were removed. Ad-hoc re-signing also made shutdown hang with fresh
  and existing profiles, so signing remains a final-identity Day 5 gate. The
  remaining CEF default-root warning is explicit Day 2 work; concurrent named
  profiles are not yet claimed.
- Release evidence covers the HiDPI responsive fixture, WebGL 2, GitHub public,
  Blend or Die, get.webgl.org, normal restart, incognito cleanup, and orderly
  one-second exits. CDP was enabled only for measurement and is absent from the
  default launch path.
- Evidence: `docs/cef_day1_report.md`.

## DECISION_ENGINE_057 - Use an owner-only engine-neutral lifecycle adapter

- The browser adapter contract is version `1.0` and names capabilities, tab
  identity, origin, page revision, facts, actions, receipts, and typed errors.
  Host integrations feature-test capabilities and do not branch on CEF or
  Servo names.
- The production-shaped CEF transport is a per-session Unix socket plus a
  browser-generated 256-bit bearer capability. Socket, grant, and parent
  directory permissions are owner-only; close removes the complete session.
- CEF lifecycle control is implemented inside the browser process without CDP,
  WebDriver, an extension, or page injection. Day 2 intentionally exposes only
  ping, status, navigate, pause, and close.
- Existing Servo and Chrome-reference transports remain accepted as legacy
  adapters. They were not rewritten during the CEF migration.
- Automated CEF gates use hidden incognito state and Chromium's test-only mock
  keychain to avoid user prompts. Normal product profiles retain platform
  credential storage; distribution signing and profile-root hardening remain
  separate release gates.
- Evidence: `docs/cef_day2_engine_adapter_report.md`.

## DECISION_ENGINE_058 - Accept the CEF no-CDP truth/reflex gate

- The official pinned CEF Release build passed three independent 100-target
  runs with 300/300 hits, zero misses, zero protected-value leaks, and no CDP,
  WebDriver, screenshot, or extension path.
- Full renderer-fact-to-page-receipt p95 was 3.2 ms in all three runs. The
  largest measured sample was 5.8 ms.
- The collector is installed from `CefRenderProcessHandler::OnContextCreated`
  before page scripts. Its native emitter is captured in a closure and removed
  from the global object; it does not mutate the DOM. Browser IPC accepts only
  fixed target, control, ready, and receipt message shapes.
- Sensitive controls export kind and completion only. The host receives an
  action id and exact page revision; geometry remains inside the browser
  adapter, which dispatches native CEF pointer events and waits for a renderer
  receipt.
- This is a bounded DOM-target result. Canvas/WebGL truth, hostile pages,
  forms, replay, and cross-frame handling are not implied. Keyboard dispatch
  stays disabled until Day 4 can enforce focused-field ownership and protected
  control policy.
- Evidence: `docs/cef_day3_truth_reflex_report.md`.

## DECISION_ENGINE_059 - Require stable signing for login-bearing CEF profiles

- Normal CEF profiles use macOS Keychain-backed Chromium Safe Storage for the
  encryption key protecting cookies, login state, and saved credentials.
- Ad-hoc/linker signatures are prohibited for login-bearing dogfood builds
  because their designated requirement changes with each build and causes
  repeated Keychain authorization prompts.
- The macOS build accepts `SACCADE_CODESIGN_IDENTITY` and signs leaf framework
  libraries, the CEF framework, five helper apps, and the main Saccade bundle
  before strict verification. Mock Keychain remains test-only.
- A narrow CEF 150 quit fallback preserves orderly browser close when Chrome
  Runtime replaces the sample delegate with an `AppController` that lacks
  `tryToTerminateApplication:`.
- Local Developer ID and normal-profile Keychain gates pass. Hardened runtime,
  notarization, stapling, and clean-machine installation remain public-release
  gates.
- Evidence: `docs/cef_macos_signing_keychain_report.md`.

## DECISION_ENGINE_060 - Accept visible links and real-site actions in the CEF reflex core

- A page rendering successfully is not a Saccade product gate. The required
  chain is renderer fact, action map, native browser input, renderer receipt,
  and observable page-state change.
- The CEF collector now inventories visible buttons, links, and DOM targets
  with stable action ids, semantic labels, current geometry, and page revision.
  It refreshes after same-document navigation and isolates scan failures by
  stage instead of silently disabling the whole collector.
- The original `https://mouseaccuracy.com/` page exposed `START` as a Vue
  RouterLink rather than a button. The owner bridge discovered and clicked it,
  observed navigation to `/game`, then produced matching verified receipts for
  12 live targets. Median fact-to-receipt latency was 4.55 ms and p95 was
  6.2 ms. CDP, WebDriver, screenshots, and host-supplied coordinates were not
  used.
- This closes the CEF pointer reflex migration for visible top-frame DOM
  actions. Keyboard/form policy, cross-frame facts, action invalidation for
  hidden same-page controls, and replay remain Day 4 work.
- Evidence: `docs/cef_mouseaccuracy_live_report.md` and
  `runs/cef_mouseaccuracy_live/live_20260715-085447/report.json`.
