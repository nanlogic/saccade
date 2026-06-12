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
