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
- Pinned Servo `0.2.0` emits `keydown`, `keypress`, `input`, and `keyup` for this path, but not `beforeinput`. `InputEventResult::Consumed` stays false despite successful DOM input, so verification should rely on DOM state, replay evidence, and dispatch-failure checks.
- The gate passed three consecutive local runs. This proves native keyboard text entry is available; FORMMAX now uses it for one real text field and still needs broader control coverage.

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
- This is not yet the Chrome adapter; it does not include browser chrome, CDP action maps, click verification, redacted truth, or replay integration.

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
