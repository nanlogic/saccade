# Saccade / MOUSEMAX — One-Shot Build Spec v4

**Date:** 2026-06-11 · **Owner:** Wayne (NaN Logic LLC) · **Executor:** Codex with the global `global-codex-supervisor` skill or the project-local `saccade-supervisor` skill; frontier supervisor model preferred, bounded cheap/Spark workers optional
**Supersedes:** plan v3. v3's architecture and build gates are preserved; v4 renames the flagship project to **Saccade**, keeps **MOUSEMAX** as the first benchmark, and adds a mandatory **M-1 Browser Viability Chat / Kill Gate** before any code. If Saccade's browser route is not viable, Codex must stop instead of producing fake progress.

---

## 0. Agent operating rules — READ FIRST, FOLLOW ALWAYS

These rules exist because the Servo API churns monthly and this project mixes a huge dependency with a latency-critical loop. Violating them is how this project fails.

1. **M-1 happens before code.** The first Codex session is a structured viability conversation, not implementation. It may write only `docs/viability_review.md`. It must decide whether the Saccade browser route is worth attempting. If the verdict is `KILL` or unresolved `PIVOT_ENGINE`, stop. Do not scaffold, pin Servo, or write Rust before M-1 is signed off by Wayne.
2. **Never guess a Servo API signature.** Before writing any code that calls the `servo` crate, run `cargo doc -p servo --no-deps` and read the generated docs for the *pinned* version in `Cargo.lock`. Online docs (doc.servo.org) track `main` and WILL drift from our pin. If a symbol in this spec doesn't exist under our pin, find the equivalent in the local docs and record the mapping in `docs/servo_api_map.md`. Do not upgrade `servo` to make a symbol appear.
3. **Pin once, never bump.** `servo` is pinned to one exact version for the life of this project (see §6). Commit `Cargo.lock`. If a Servo bug blocks you, document it in `docs/blockers.md` and work around it; do not upgrade mid-project.
4. **One milestone per session.** Complete the milestone's "Done when" gate (a literal command that must pass) before touching the next milestone. Never start M(n+1) with M(n) red.
5. **All Servo types stay inside `saccade_browser`.** No other crate may import anything from the `servo` crate. Cross-crate communication uses `saccade_core` types only. This contains API churn to one crate.
6. **Fast loop discipline.** Run `cargo test` (workspace default-members exclude the Servo-dependent crates — see §6) constantly. Build `saccade_browser`/`mousemax` only at integration points; first build takes 30–60+ minutes and that is normal.
7. **The hot loop allocates nothing per frame** after warmup: preallocated buffers, no `format!`, no `println!`, no JSON serialization. Logging goes through a bounded channel to a logger thread.
8. **Honest measurement.** Every latency number must come from recorded monotonic timestamps in the replay log, never from estimates. If a number can't be measured yet, report "unmeasured", not a guess.
9. **When the live site misbehaves** (won't load, renders wrong, consent wall), do NOT hack around it silently. Follow the playbook in §14, record findings in `docs/site_profile.md`, and continue development against the local arena (§12).
10. **Don't refactor across crates "while you're in there."** Each crate has a spec below; match it. Deviations require a note in `docs/decisions.md` with one-line rationale.
11. **Report format:** end every milestone with the report template in §16.

---

## 1. Mission and success criteria

Build an AI-native browser runner on the **Servo** engine (Rust) that defeats **https://mouseaccuracy.com/classic/** at maximum difficulty — **Target Spawn Speed = Epic, Target Size = Tiny, 15 s** — using only information the rendering engine legitimately has (pixels it painted, layout it computed, input pipeline it owns), with **zero LLM calls in the real-time loop**.

The benchmark is the proof. The product is the architecture: a reusable *reflex layer* (frame truth → target detection → motor control → verified input) that any LLM agent can drive through a one-shot high-level API.

**Naming:** Saccade is the flagship project and repository. MOUSEMAX is the first benchmark and CLI harness. Future demos such as FORMMAX and THREADMAX sit under Saccade; they do not dilute the MOUSEMAX acceptance gate.

### Acceptance (the project's single end-to-end test)

A run is **PASS** when, on the real site at Epic+Tiny for the full 15 s:

| Metric | Requirement | Measured from |
|---|---|---|
| `misses` (site's "misclicked" counter) | == 0 | site result screen (authoritative) |
| `hits` | == `targets_seen` by our tracker | site result screen vs replay log |
| `false_positive_clicks` | == 0 | replay log (click with no matching target) |
| `stale_clicks` | == 0 | replay log |
| p95 `detect → input_dispatched` | ≤ 5 ms | replay timestamps |
| p95 `target_first_visible → input_dispatched` | ≤ 1 frame + 5 ms | replay timestamps |
| LLM calls during the 15 s window | == 0 | by construction; assert in code |
| Stability | 5 consecutive PASS runs | run harness |

Do **not** hard-code an expected hit count (v1's "187 hits" was fiction). Epic's real spawn rate is unknown until M1 measures it; success is relative to `targets_seen`.

### 1.1 Why the circle benchmark is the right first proof

MOUSEMAX is not a game bot project. The circle game is the smallest brutal proof of the browser capability we actually need for dynamic webpages and forms:

```text
Browser-rendered truth
  -> identify the actionable visual object
  -> map it into calibrated coordinates
  -> inject real browser input
  -> verify the page reacted correctly
  -> repeat while the page is changing
```

If Epic+Tiny can be solved with zero misses and no LLM calls in the loop, then the browser has proven the substrate that ordinary web work currently lacks: it can see the live rendered state, localize targets, click accurately, and verify results faster than a screenshot/DOM agent can even finish one observe-think-act cycle. Ordinary form filling is slower than MOUSEMAX in time pressure, but harder in semantics; therefore MOUSEMAX proves the **vision/action substrate**, and FORMMAX later proves the **field semantics substrate**.

The public claim must be precise:

```text
MOUSEMAX proves low-latency dynamic-page visual/action grounding.
It does not by itself prove every form semantic, login flow, CAPTCHA, payment, or anti-bot case.
```

### 1.2 Generalization target: from circles to forms and dynamic pages

The downstream product is a Browser Truth Layer:

```text
MOUSEMAX:
  FrameTruth -> TargetStream -> MotorAction -> VerifiedClick

FORMMAX:
  RenderedFieldTruth -> FieldMap -> FillTransaction -> ValidationDiff

THREADMAX:
  ViewportTruth -> Post/CommentMap -> SharedViewLedger -> Human/AI Sync
```

The form problem is not mainly typing speed. The real failure is that AI often guesses where fields are, what each field means, whether the page changed, and whether the user and AI are looking at the same state. MOUSEMAX forces the browser to solve the lower-level truth problem first: what is on screen, where it is, whether it can be clicked, and whether the click worked.


---

## 2. Ground truth (verified 2026-06-10 — this section corrects v1's assumptions)

These were verified by web research on the date above. Re-verify only items marked ⚠ during M0/M1; treat the rest as settled.

### 2.1 Servo is now an ordinary Cargo dependency — forking is NOT required to start
- Servo published `servo` **v0.1.0 to crates.io on 2026-04-13** (first crates.io release; GitHub binary releases since Oct 2025). Monthly releases follow; **breaking changes between monthlies are expected and announced**; an **LTS line** exists for embedders. Source: servo.org blog "Servo is now available on crates.io" (2026-04-13); LWN article 1067467.
- Default engine resources are **baked into the crate** (`servo-default-resources`, servo/servo PR #43182) — no `resources/` directory copying like the old embedding tutorials required.
- ⚠ At M0, check crates.io for the newest `servo` release (docs at doc.servo.org currently show **0.3.0**, consistent with monthly releases after 0.1.0). Pin the newest stable monthly available at start; record it in §6.

### 2.2 The current embedding API already provides what v1 wanted to fork Servo for
Verified against doc.servo.org (servo 0.3.0) and merged PRs:

| Need (v1's "Servo hook") | Exists today, zero fork | Evidence |
|---|---|---|
| Create/drive a webview | `Servo::new(...)`, `WebViewBuilder::new(&servo, rendering_context).url(..).delegate(..).build()`, `WebViewDelegate`, `ServoDelegate` | doc.servo.org `WebView`/`WebViewBuilder`; PRs #35196, #43787 |
| Inject input at engine level | `WebView::notify_input_event(InputEvent)` — single entry point for mouse/keyboard/wheel. **Servo derives DOM `click` events from a MouseButton Down+Up pair** (PR #39705); scroll is derived from wheel events (Oct 2025 update, PR #40269) | PR #35430; servo.org blog 2025-11-14 |
| Read what was painted (pixel truth) | `RenderingContext::read_to_image(source_rectangle: Box2D<i32, DevicePixel>) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>>` — reads the **back buffer** (valid as soon as Servo renders, before `present()`), supports **sub-rectangles** (read only the game area) | doc.servo.org `RenderingContext` |
| Stable repaint signal | `WebViewDelegate::notify_new_frame_ready` → embedder calls `WebView::paint()` → optional `present()` | doc.servo.org `WebView` "Rendering Model" |
| Reference screenshots | `WebView::take_screenshot(rect?, callback)` — **waits for render-stable state**; use for debug/before-after evidence, NOT in the hot loop | PR #39583 |
| Run JS read-only probes | `WebView::evaluate_javascript(script, callback)` (async, callback-based) | PR #35720; doc.servo.org `WebView` |
| Persistent page instrumentation | `WebView::user_content_manager()` → user scripts injected into pages | doc.servo.org `WebView` method list |
| DPI control | `WebView::set_hidpi_scale_factor`, `device_pixels_per_css_pixel`, `hidpi_scale_factor`, `viewport_details`, `set_page_zoom` (absolute, idempotent) | doc.servo.org; servo.org blog 2025-11-14 |
| Headless / windowed contexts | `RenderingContext` implementors in the crate (expected names: `WindowRenderingContext`, `OffscreenRenderingContext`, `SoftwareRenderingContext`) ⚠ verify exact names in local `cargo doc` at M0 | doc.servo.org trait page |
| Minimal embedder reference | `components/servo/examples/winit_minimal.rs` in the servo repo — read it at the pinned tag and copy its event-loop shape | servo repo |

**Consequence:** v1's `servo_patches/` (5 patches, render tap, canvas tap, hit-test export, input broker, report stream) is **deleted from the critical path**. Engine-internal taps move to **Phase E** (§15) as the post-benchmark moat, built against a fork only after the benchmark is green.

### 2.3 Servo internals relevant to Phase E (so we design Tier-2 interfaces that Phase E can slot into)
- 2D canvas: script-side state machine lives in `components/script/dom/canvasrenderingcontext2d.rs` (+ canvas state module); drawing commands are sent over IPC to a **canvas paint thread** with pluggable backends. New **Vello GPU** (`--features vello`, pref `dom_canvas_vello_enabled`) and **Vello CPU** (`--features vello_cpu`, pref `dom_canvas_vello_cpu_enabled`) backends landed mid-2025 and have been getting faster (servo.org blogs 2025-08-22, 2025-09-25; issue #38345). ⚠ Default backend at our pin: check release notes.
  - **Design note:** by the time commands reach the paint thread, an `arc()` may already be flattened to cubic Béziers. A Phase E canvas tap should hook the **script-side** semantic methods (`arc`, `ellipse`, `fill`, `stroke`, `drawImage`), or else detect circles from Bézier control points (a full circle ≈ 4 cubics with control-point ratio κ ≈ 0.5522847).
- The Servo team itself flagged that "many recent bugs have been related to viewport/window/screen coordinate spaces" (blog 2025-08-22). This *validates* v1's strict coordinate typing AND mandates the runtime calibration in §10 — unit tests alone are insufficient.
- Servo has a WebDriver server and growing conformance (2025 work); we don't use it (our embedder API is lower-latency and sufficient), but it's a fallback orchestration path if `evaluate_javascript` proves unreliable.

### 2.4 mouseaccuracy.com/classic — what is confirmed vs unknown
Confirmed by fetching the live page (2026-06-10):
- Page text/controls exactly as v1 assumed: options **Slow/Normal/Fast/Epic**, **Tiny/Small/Medium/Large**, **"Start!"**; result strings **"Time is up!"**, **"You clicked __ targets."**, **"You misclicked __ times."**; status **"Time Remaining: 15"**; an **"Ad:"** slot exists in-page; footer links to nerdordie.com (creator). These strings are the anchors for the Action Map and Result Reader.
- **Unknown until M1 (could not extract the page's JS through the research tooling):** whether targets are DOM elements, SVG, or `<canvas>`-drawn; whether hits register on `mousedown`, `click`, or pointer events; target lifetime/animation curve; Epic spawn interval; whether multiple targets coexist; consent banner behavior; whether the ad slot loads an iframe.
- **Therefore M1 is a mandatory empirical recon step** (§7, §11.1) and the detector layer is built fusion-style so either answer (DOM or canvas) is covered — which was v1's instinct, kept.

### 2.5 Tooling constraint discovered during research
The site's game JS could not be pulled via the doc-extraction fetcher (it strips `<script>`); archive CDX enumeration was blocked. M1's probe (Appendix B) settles every unknown in §2.4 in minutes by asking the *rendered page itself*, which is more reliable than reading source anyway (the site may change).

---

## 3. What changed from plan v1, and why (summary table)

### 3.1 v4 changes: Saccade name + browser kill gate

- **Project name:** the flagship is now **Saccade**. `mousemax` remains the first benchmark binary. Crate prefixes are `saccade_*`, not `asvc_*`, and the repo root is `saccade/`.
- **First action:** Codex must perform **M-1 Browser Viability Chat / Kill Gate** before writing code. This deliberately allows high-level debate with Wayne: does Servo/WebView/readback/input look viable enough to attempt? If not, stop cleanly or pivot engines deliberately.
- **No sunk-cost coding:** if the browser cannot plausibly load, render, capture, inject input, and verify dynamic pages, Saccade is not started. The right failure mode is `docs/viability_review.md` plus a stop decision, not a half-built repo.

| # | v1 | v2 | Why |
|---|---|---|---|
| 1 | Fork Servo first; 5 patches before any loop runs | **Zero-fork core** on crates.io `servo`; fork deferred to Phase E | §2.2: public API already covers pixel truth, input, JS probes, DPI. Forking first = highest risk, slowest compile loop, worst possible terrain for an agent. |
| 2 | No go/no-go gate on Servo compatibility | **M1 = mandatory recon + go/no-go**: does the page load, animate, and accept clicks in Servo at all? | Servo web-compat is incomplete; if the site doesn't run, everything else is moot. v1 had no Phase 0. |
| 3 | 7-thread hot path with lock-free ring buffers | **Single-threaded reflex loop** on the embedder thread (+ one async logger thread) | Detection on a ~1280×600 region is 1–3 ms; one click decision per frame at 60 Hz needs no concurrency. Threads + `Rc<RefCell<WebView>>` (WebView is `!Send`) is a correctness trap for an agent. Ring buffers return only if measured budget demands it. |
| 4 | Global click freeze until last click verified | **Per-target pending verification**; conservative mode only on a *confirmed* miss counter increase | Epic likely runs multiple simultaneous targets; a global freeze throttles throughput and risks letting targets expire. |
| 5 | `u128` nanosecond wall timestamps everywhere | `u64` nanoseconds from a process-start `Instant` monotonic epoch (+ one wall-clock anchor per run) | Monotonic, serde-friendly, sufficient range (584 years), immune to NTP jumps mid-run. |
| 6 | Hit-test via forked `PaintHitTestResult` export | Preflight via geometry (point inside target bbox ∧ inside game area) + **runtime calibration** (§10); engine hit-test export returns in Phase E | Calibration empirically proves the full coords→input→page chain, which is strictly stronger than a hit-test on possibly-mis-mapped coordinates. |
| 7 | Canvas command tap (engine) as primary detector | **Pixel readback detector as primary** (engine-native, works for DOM/SVG/canvas/WebGL uniformly); DOM-rect probe as semantic accelerant when M1 shows DOM targets; engine canvas tap = Phase E | `read_to_image` on the back buffer IS engine truth. One detector that works regardless of page tech beats five detectors of which the critical one needs a fork. |
| 8 | 11 crates incl. HTTP/WebSocket agent API in core path | **6 lib crates + 1 binary**; HTTP status API is M8 (optional) | Smaller surface for the agent; the LLM-facing API for the benchmark is the CLI + result JSON anyway. |
| 9 | Acceptance: "187 hits" style absolute numbers | Acceptance relative to `targets_seen` + p95 latency + 5-run stability (§1) | 187 was invented; Epic's spawn rate is an M1 measurement. |
| 10 | Real site used for all testing | **Local arena replica for CI/iteration**; real site for recon + headline runs only | Determinism, politeness to the site's servers, and CI that doesn't depend on third-party uptime. Same mechanics, configurable difficulty, seeded RNG. |
| 11 | Allowed/forbidden rules left one gray zone (platform-API observation) | Gray zone resolved explicitly in §4 with a purity switch (`instrumentation = "none" \| "observe_only"`) | The headline run must be defensible. Pixel-only mode exists so the claim "no page instrumentation at all" can also be demonstrated. |

Everything else from v1 survives: the layered architecture, strict coordinate types, GameFrameReport/RenderedTarget/MotorAction/ClickReceipt data model, JSONL replay, detector fusion, target lock, conservative mode, synthetic test pages, TOML config, CLI shape, "LLM never in the loop", "don't read game variables", "don't hard-code coordinates or selectors".

---

## 4. Non-negotiable rules (refined from v1; enforce in code review and in the e2e test)

### Allowed
- Reading **pixels Servo painted** (`read_to_image`, `take_screenshot`).
- Reading **layout geometry of rendered elements** via read-only JS (`getBoundingClientRect`, computed styles, `elementFromPoint`) — this is layout truth the engine computed, equivalent to v1's layout tap, just accessed without a fork.
- Reading **publicly displayed page text** (score, timer, results) — same channel a human's eyes use.
- Sending input **only** through `WebView::notify_input_event` (the engine's real input pipeline).
- Clicking the options UI (Epic/Tiny/Start) by **discovering** their on-screen rects at runtime (text-anchored), never hard-coded coordinates.

### Forbidden (any of these voids the benchmark)
- Reading game-internal JS state: `window.game.*`, target arrays, closures, hidden coordinates.
- Patching or monkeypatching game logic, `Math.random`, score/hit functions, spawn timers.
- Dispatching synthetic DOM events directly on elements (`el.click()`, `dispatchEvent(new MouseEvent(...))`) — clicks must enter through the engine input pipeline.
- Modifying page code/CSS to slow the game, enlarge targets, or alter difficulty.
- Screenshots → vision-LLM → coordinates. No LLM, no remote call, no ML inference in the loop.

### Gray zone — resolved
A read-only **observe-only userscript** MAY wrap platform APIs to *record* what the page draws or mounts (e.g., wrap `CanvasRenderingContext2D.prototype.arc/fill/drawImage` to log draw geometry; `MutationObserver` to notice target elements appearing). Constraints: must call through to the original API unchanged, must never read game variables, must never block or throw into page code, and must be disabled by `instrumentation = "none"` config. Rationale: it observes exactly "what the browser is about to show the user" (v1's own allowed category) and is the zero-fork stand-in for Phase E's engine canvas tap. **The headline PASS must be reproduced once with `instrumentation = "none"` (pure pixel mode)** so the strongest claim is also demonstrated.

### Operational ethics
- Real-site runs are for recon and headline benchmarks: keep volume modest (≤ ~30 runs/day), never parallel-hammer the site, set a UA string identifying the project. All bulk iteration happens on the local arena (§12). If the site exposes any human leaderboard, do not submit scores to it.

---

## 5. Architecture (v4)

```
┌─────────────────────────────────────────────────────────────────┐
│ LLM / Claude Code / human                                        │
│   mousemax run --spawn-speed Epic --target-size Tiny ...         │
│   reads BenchmarkResult JSON + replay.jsonl    (never per-frame) │
└──────────────────────────────┬───────────────────────────────────┘
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│ mousemax (bin)                                                   │
│   Orchestrator state machine: Load → Consent? → Options →        │
│   Calibrate? → Start → REFLEX → Results → Report                 │
└──────────────────────────────┬───────────────────────────────────┘
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│ saccade_browser  (ONLY crate that imports `servo`)                  │
│   AgentBrowser: winit event loop + Servo + WebView               │
│   FrameTruth:   paint() + read_to_image(game_area)  [pixels]     │
│   JsProbe:      evaluate_javascript (rect/score/recon, async)    │
│   InputPort:    notify_input_event (move, down, up) + receipts   │
│   Userscripts:  user_content_manager (observe-only, optional)    │
└───────┬───────────────────────────────┬──────────────────────────┘
        ▼ FrameObservation (saccade_core)  ▼ InputReceipt
┌──────────────────────┐   ┌────────────────────┐   ┌──────────────┐
│ saccade_detect           │→ │ saccade_motor          │→ │ saccade_verify   │
│  pixel detector       │  │  target lock        │  │  per-target   │
│  dom-rect detector    │  │  scheduler (multi-  │  │  disappearance│
│  (canvas-observe det.)│  │  target, deadline)  │  │  + score poll │
│  fusion + tracker     │  │  preflight + stale  │  │  + latency    │
└──────────────────────┘   └────────────────────┘   └──────────────┘
                 all three: pure Rust, no servo dep, fully unit-tested
        ▼
┌──────────────────────┐        ┌─────────────────────────────────┐
│ saccade_core             │        │ saccade_replay                      │
│  coord types, frames, │        │  JSONL writer (async thread),    │
│  targets, actions,    │        │  reader, replay analyzer CLI     │
│  receipts, metrics    │        └─────────────────────────────────┘
└──────────────────────┘
```

**Hot path (single thread, per frame):** `notify_new_frame_ready` → `paint()` → `read_to_image(game_area)` → `detect` → `track` → `motor.decide` → `notify_input_event ×3 (move,down,up)` → enqueue log event. Budget table in §9.

**Tiers:**
- **Tier -1 (M-1):** structured browser viability conversation and explicit GO/PIVOT/KILL verdict.
- **Tier 0 (M0–M1):** boot Servo as a dependency; recon the real site; go/no-go.
- **Tier 1 (M2–M3):** all pure-Rust crates, unit-tested on synthetic data. No Servo needed.
- **Tier 2 (M4–M7):** zero-fork integration; calibration; arena PASS; real-site PASS. ← the benchmark lives here.
- **Phase E (§15):** Servo fork with engine taps (canvas command observer, display-list export, frame lifecycle hook, engine hit-test, input receipts with in-pipeline timestamps). The moat; optional for the benchmark.

---

## 6. Repository layout and workspace

```
saccade/
├── Cargo.toml                  # workspace (below)
├── Cargo.lock                  # COMMITTED
├── AGENTS.md                   # Appendix A, verbatim
├── rust-toolchain.toml         # pin stable toolchain (record version at M0)
├── docs/
│   ├── viability_review.md     # M-1 browser route verdict: GO/PIVOT/KILL
│   ├── decisions.md            # running log of deviations + rationale
│   ├── servo_api_map.md        # spec-symbol → actual-symbol at our pin
│   ├── site_profile.md         # M1 recon findings (page tech, timings, consent)
│   └── blockers.md
├── crates/
│   ├── saccade_core/              # types: geometry, frames, targets, actions, receipts, metrics
│   ├── saccade_detect/            # pixel detector, dom-rect detector, fusion, tracker
│   ├── saccade_motor/             # scheduler, target lock, preflight, conservative mode
│   ├── saccade_verify/            # pending-click verification, score state, latency metrics
│   ├── saccade_replay/            # JSONL log writer/reader + `replay` analyzer
│   └── saccade_browser/           # ALL servo usage. AgentBrowser, FrameTruth, JsProbe, InputPort
├── bins/
│   └── mousemax/               # CLI + orchestrator + calibration + score reader + arena server
├── test_pages/
│   ├── calibration.html        # §10
│   ├── arena/                  # §12 local replica (index.html + arena.js, seeded RNG)
│   ├── dom_targets.html  svg_targets.html  canvas_arc_targets.html
│   ├── canvas_sprite_targets.html  webgl_targets.html
│   ├── overlay_interference.html  high_dpi_grid.html
└── runs/                       # replay logs (gitignored except .gitkeep)
```

### Workspace `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
  "crates/saccade_core", "crates/saccade_detect", "crates/saccade_motor",
  "crates/saccade_verify", "crates/saccade_replay", "crates/saccade_browser",
  "bins/mousemax",
]
# Fast inner loop: `cargo test` / `cargo check` skip the Servo-heavy crates.
default-members = [
  "crates/saccade_core", "crates/saccade_detect", "crates/saccade_motor",
  "crates/saccade_verify", "crates/saccade_replay",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
euclid = "0.22"          # match the euclid major used by our servo pin (check cargo tree)
anyhow = "1"
thiserror = "2"
crossbeam-channel = "0.5" # logger channel only — NOT in the hot path decision chain
clap = { version = "4", features = ["derive"] }
toml = "0.8"
image = "0.25"            # match servo pin's image major for ImageBuffer interop
tiny_http = "0.12"        # serves test_pages + arena locally
```

`crates/saccade_browser/Cargo.toml` additionally (M0 fills exact versions):

```toml
[dependencies]
servo = "=0.X.Y"   # M0: newest crates.io monthly at project start; record here and in docs/decisions.md.
                   # If a needed API is missing from the crates.io release, fall back to:
                   # servo = { git = "https://github.com/servo/servo", tag = "vX.Y.Z" }  (a release TAG, never a moving branch)
winit = "0.30"     # match the winit major used by the pinned servo's winit_minimal example
url = "2"
```

**M0 must:** run `cargo tree -p servo | grep -E "euclid|image|winit"` and align the workspace versions above to the pin, recording results in `docs/servo_api_map.md`. Version skew between our `euclid`/`image` and Servo's is the most common silent integration failure.

### System prerequisites (Ubuntu/Debian; record actual list in docs/decisions.md at M0)
Install per the Servo book "Building Servo" prerequisites for Linux — at minimum: `build-essential`, `cmake`, `python3`, `pkg-config`, `libssl-dev`, font/GL/X11 dev packages, `clang`. If the `servo` crate build fails on a missing system lib, the error names it; install and append it to the recorded list. First clean build of `saccade_browser`: expect **30–60+ min**; later builds are incremental. Optional: `sccache` (`RUSTC_WRAPPER=sccache`).

### Platform decision (ratified)
v2 targets **Linux + X11** for the benchmark machine. Reasons: `WINIT_X11_SCALE_FACTOR=1` forces DPR=1 at the windowing layer (belt) on top of `set_hidpi_scale_factor(1.0)` (suspenders); X11 has no compositor input restrictions; CI parity. macOS/Wayland later. Headless (`SoftwareRenderingContext`-style context, exact name verified at M0) is used for CI against the arena once the windowed path works.

---

## 7. Milestones (build order with binary gates)

Each milestone ends with a literal "Done when" command/check. Do not reorder. **M-1 is mandatory and must happen before any repo scaffolding or code.** M2/M3 can run before M1 finishes if the live site is temporarily unreachable, but M4+ requires M1's findings.

### M-1 — Browser viability chat / kill gate  ★ first Codex session, no code

**Scope:** before writing any source code, Codex and Wayne deliberately argue about whether Saccade's browser route is viable. This is allowed to be conversational ("闲扯"), but the output must be concrete. The point is to kill the project early if the browser substrate is implausible.

**Hard rule:** no Rust files, no Cargo workspace, no Servo pinning, no scaffolding, no search-worker sprawl. Codex may read this spec, read current official docs/release notes as needed, inspect local notes if present, and write exactly one file: `docs/viability_review.md`.

**Questions Codex must answer:**
1. Does the stock Servo/WebView route plausibly expose the four things Saccade needs: rendered frame readback, stable frame readiness, browser-level input, and enough JS/probe support for recon?
2. What are the top five browser-kill risks, and which ones are existential versus merely annoying?
3. What evidence will M0 and M1 produce to settle those risks quickly?
4. If Servo is shaky, is the fallback **arena-only**, **CEF/Chromium prototype**, or **kill**?
5. Is the mouseaccuracy real-site headline still worth attempting, or should the project only claim local dynamic-page arena until Servo catches up?

**Allowed verdicts:**
- `GO_SERVO`: proceed to M0 with Servo.
- `GO_SERVO_WITH_BACKUP`: proceed to M0, but record a CEF/Chromium fallback trigger in `docs/blockers.md`.
- `ARENA_ONLY`: build Saccade only against local arena for now; do not promise real-site M7.
- `PIVOT_ENGINE`: stop Servo work and ask Wayne before rewriting the plan around CEF/Chromium.
- `KILL`: stop. Do not write code. The project is not worth the opportunity cost right now.

**Done when:** `docs/viability_review.md` exists and ends with exactly one line: `SACCADE_BROWSER_VERDICT: <GO_SERVO|GO_SERVO_WITH_BACKUP|ARENA_ONLY|PIVOT_ENGINE|KILL>`. Wayne must explicitly approve any verdict except `KILL` before Codex starts M0.

### M0 — Toolchain + pinned Servo boots a blank webview
**Scope:** workspace skeleton (§6); pin `servo`; `saccade_browser::AgentBrowser::new()` opens a 1280×800 window (DPR forced to 1), loads `about:blank` then `test_pages/calibration.html` served by a built-in `tiny_http` server on `127.0.0.1:0`; clean shutdown.
**Tasks:** read the pinned tag's `components/servo/examples/winit_minimal.rs` and mirror its event-loop shape; implement `WebViewDelegate` minimally (`notify_new_frame_ready` sets a repaint flag); generate local docs (`cargo doc -p servo --no-deps`) and write `docs/servo_api_map.md` covering: rendering-context constructor used, `InputEvent`/`MouseButtonEvent`/`MouseMoveEvent` exact constructors and **the unit type of their point parameter**, `evaluate_javascript` callback signature, `read_to_image` rect type.
**Done when:** `cargo run -p mousemax -- selftest-boot` prints `BOOT OK title="Calibration"` and exits 0. (Implement `selftest-boot`: load page, wait for `LoadStatus` complete, read `page_title()`, exit.)

### M1 — Real-site recon + GO/NO-GO  ★ the gate v1 lacked
**Scope:** load `https://mouseaccuracy.com/classic/` in our shell. Produce `docs/site_profile.md` answering every §2.4 unknown.
**Tasks:**
1. Screenshot the loaded page (`take_screenshot`) → save to `runs/recon/`. Confirm options UI renders and matches §2.4 strings.
2. Run the recon probe (Appendix C) via `evaluate_javascript`: classifies game tech (canvas vs DOM/SVG), finds the game container rect, locates option/start controls and score/timer text nodes, detects consent/ad iframes. **The probe is read-only and runs only on the options/results screens, never during the 15 s game.**
3. Click Epic → Tiny → Start via discovered rects + real input events; let one game run **without clicking targets**; screenshot mid-game; run a passive observation script after the run to read the results text. Record: countdown-before-start? targets concurrent count, apparent size range (Tiny), spawn cadence, lifetime, whether the page reacted to our option clicks at all (proves the input pipeline works on this site).
4. If anything fails (page blank, options unresponsive, JS errors): follow §14.1 playbook; the project continues on the arena and M7 is re-attempted after documenting the compat gap.
**Done when:** `docs/site_profile.md` exists with all §2.4 fields filled (or explicitly marked BLOCKED with evidence), plus ≥3 screenshots, plus a one-line verdict: `SERVO_COMPAT: GO` or `NO-GO(<reason>)`.

### M2 — saccade_core + saccade_replay (pure Rust)
**Done when:** `cargo test -p saccade_core -p saccade_replay` passes; includes round-trip serde tests and coordinate-mapping tests at DPR 1.0/1.5/2.0/3.0.

### M3 — saccade_detect + saccade_motor + saccade_verify on synthetic data
**Scope:** §8.2–8.4 implemented; a `synthetic` test module renders fake frames (plain background + drawn discs, multi-target, spawn/despawn, animated radius) into RGBA buffers and drives the full detect→track→motor→verify chain in-memory.
**Done when:** `cargo test` (workspace default-members) passes, including these named tests: `detects_single_disc_center_within_half_px`, `tracks_growing_disc_as_same_target`, `one_click_per_target`, `multi_target_oldest_first`, `stale_frame_rejected`, `no_click_outside_game_area`, `miss_counter_triggers_conservative_mode`, `verifies_hit_by_disappearance`. Plus `cargo bench`-style timing test proving detect ≤ 3 ms for a 1280×600 frame on the dev machine (plain `Instant` timing in a test is fine).

### M4 — Calibration green (the coordinate chain is proven end-to-end)
**Scope:** §10 implemented. `mousemax calibrate` loads `calibration.html`, clicks 5 known dots via the full InputPort path, page JS records received `mousedown` coordinates, probe reads them back, mapping error computed.
**Done when:** `cargo run -p mousemax -- calibrate` prints `CALIBRATION OK max_err_css_px=<x>` with x ≤ 0.5, and persists the resolved input coordinate convention (`InputSpace::CssLogical` or `DevicePhysical`) into the run config.

### M5 — Synthetic pages cleared
**Scope:** the reflex loop (§9) wired end-to-end against `test_pages/dom_targets.html`, `svg_targets.html`, `canvas_arc_targets.html`, `canvas_sprite_targets.html`, `overlay_interference.html`, `high_dpi_grid.html`. Each page self-scores in its own JS and exposes the score as visible text (read via probe between rounds).
**Done when:** `cargo run -p mousemax -- selftest-pages` reports PASS for every page (overlay page passes by NOT clicking; high-dpi page passes at DPR 1 and, with `WINIT_X11_SCALE_FACTOR=2`, after calibration re-resolves the mapping).

### M6 — Arena defeated (CI-grade)
**Done when:** `cargo run -p mousemax -- run --site arena --spawn-speed epic --target-size tiny --duration 15 --seed 42 --replay` PASSes the §1 table (arena's own counters as ground truth) **5 consecutive times**, and `mousemax replay <log> --summary` shows p95 detect→dispatch ≤ 5 ms. Wire this as the repo's e2e test (headless if the headless context works; windowed otherwise).

### M7 — Real site defeated  ★ the headline
**Done when:** §1 acceptance table PASSes on https://mouseaccuracy.com/classic/ Epic+Tiny, 5 consecutive runs, replay logs + before/after screenshots archived in `runs/`. Then reproduce **one** PASS with `instrumentation = "none"` (pure pixel mode) and archive it separately — this is the strongest-claim artifact.

### M8 (optional) — Agent API + polish
`/bench/mouseaccuracy/{start,status,result}` thin HTTP layer over the orchestrator; `mousemax replay --show-targets --show-clicks --show-latency` renders an annotated overlay (PNG sequence or single summary image per click).

### Phase E (post-benchmark, separate effort) — §15.

---

## 8. Crate specifications

Code below is the contract. Where a body is `todo!()`, the adjacent comment is the behavior spec — implement exactly that. All public types: `#[derive(Debug, Clone, Serialize, Deserialize)]` unless noted; `Copy` where cheap.

### 8.1 `saccade_core`

```rust
// geometry.rs — strict coordinate spaces (v1's design, kept; this prevents the #1 failure class)
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)] pub struct CssPx(pub f32);
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)] pub struct DevicePx(pub f32);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssPoint { pub x: f32, pub y: f32 }          // unit: CSS px, origin = webview top-left
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DevicePoint { pub x: f32, pub y: f32 }       // unit: device px, origin = webview top-left
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl CssRect {
    pub fn contains(&self, p: CssPoint) -> bool { /* inclusive on min edge, exclusive max */ todo!() }
    pub fn center(&self) -> CssPoint { todo!() }
    pub fn inside(&self, outer: &CssRect) -> bool { todo!() }
    pub fn intersect(&self, o: &CssRect) -> Option<CssRect> { todo!() }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ViewportInfo {
    pub width_css: f32, pub height_css: f32,
    pub device_scale_factor: f32,   // asserted == 1.0 for benchmark runs; mapping still general
    pub page_zoom: f32,
}

/// The ONLY place css↔device conversion happens. No naked `* dsf` anywhere else.
#[derive(Debug, Clone, Copy)]
pub struct CoordinateMapper { pub viewport: ViewportInfo }
impl CoordinateMapper {
    pub fn css_to_device(&self, p: CssPoint) -> DevicePoint { todo!() } // x * dsf * page_zoom
    pub fn device_to_css(&self, p: DevicePoint) -> CssPoint { todo!() }
    pub fn css_rect_to_device_box(&self, r: CssRect) -> (i32, i32, i32, i32) { todo!() } // floor/ceil to cover
}

/// Resolved by calibration (§10): which space `notify_input_event` points are interpreted in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InputSpace { CssLogical, DevicePhysical }
```

```rust
// time.rs
/// Nanoseconds since process-start monotonic epoch. One wall-clock anchor is logged per run.
pub type Ns = u64;
pub struct Clock(std::time::Instant);
impl Clock { pub fn now_ns(&self) -> Ns { self.0.elapsed().as_nanos() as u64 } }
```

```rust
// frame.rs / target.rs / action.rs / receipt.rs — the data model (v1's, trimmed)

pub struct FrameObservation {            // what saccade_browser hands to saccade_detect each tick
    pub frame_id: u64,
    pub t_paint_ns: Ns,                  // when paint() returned
    pub t_readback_ns: Ns,               // when read_to_image returned
    pub viewport: ViewportInfo,
    pub game_area_css: CssRect,          // crop that was read
    pub pixels: PixelRegion,             // RGBA8, tightly packed, device px of the crop
    pub dom_rects: Option<Vec<DomRectObs>>, // latest async dom-rect probe result, if fresh (≤1 frame old)
}
pub struct PixelRegion { pub w: u32, pub h: u32, pub rgba: std::sync::Arc<Vec<u8>> }
pub struct DomRectObs { pub label: String, pub rect_css: CssRect, pub t_obs_ns: Ns }

#[derive(PartialEq, Eq, Hash)] pub struct TargetId(pub u64);   // monotonically assigned by the tracker

pub enum TargetSource { PixelDetector, DomRect, CanvasObserve, Fused }

pub struct TargetCandidate {             // raw detector output, pre-tracking
    pub center_css: CssPoint, pub bbox_css: CssRect, pub radius_css: f32,
    pub source: TargetSource, pub confidence: f32,   // 0..1
    pub evidence: TargetEvidence,
}
pub enum TargetEvidence {
    PixelComponent { area_px: u32, fill_ratio: f32, contrast: f32, temporal_delta: f32 },
    DomBox { label: String },
    CanvasDraw { kind: String },
}

pub struct RenderedTarget {              // tracked, fused, ready for the motor
    pub id: TargetId,
    pub frame_id: u64,
    pub first_seen_ns: Ns, pub last_seen_ns: Ns,
    pub center_css: CssPoint, pub bbox_css: CssRect, pub radius_css: f32,
    pub confidence: f32, pub source: TargetSource,
    pub clicked: bool,                   // set by motor via tracker handle
}

pub struct GameFrameReport {
    pub frame_id: u64, pub t_report_ns: Ns,
    pub game_area_css: CssRect,
    pub targets: Vec<RenderedTarget>,
    pub detector_ms: f32,
}

pub enum MotorAction {
    Click { target_id: TargetId, point_css: CssPoint, frame_id: u64 },
    Noop  { reason: &'static str },
}

pub enum InputBackendKind { ServoInternal /* Phase E adds: OsMouse, VirtualHid (ties into the Pico-HID work) */ }

pub struct ClickReceipt {
    pub click_id: u64, pub target_id: TargetId,
    pub point_css: CssPoint, pub frame_id: u64,
    pub t_target_first_seen_ns: Ns, pub t_decided_ns: Ns,
    pub t_move_sent_ns: Ns, pub t_down_sent_ns: Ns, pub t_up_sent_ns: Ns,
    pub backend: InputBackendKind,
}
```

```rust
// metrics.rs — fixed-bucket histogram (no deps), and the BenchmarkResult JSON (schema in §13)
pub struct Histogram { /* log-spaced buckets 0.1ms..1s */ }
impl Histogram { pub fn record_ns(&mut self, ns: Ns) {} pub fn p50_ms(&self)->f32{todo!()} pub fn p95_ms(&self)->f32{todo!()} }
```

### 8.2 `saccade_detect`

```rust
pub trait TargetDetector {
    fn detect(&mut self, obs: &FrameObservation, cfg: &DetectConfig) -> Vec<TargetCandidate>;
    fn name(&self) -> &'static str;
}
```

**PixelDetector (primary; must hit the §9 budget):**
Algorithm — implement exactly; every threshold comes from `DetectConfig` with the defaults shown:
1. **Background model:** on entering reflex mode, take the first frame (post-countdown if M1 found one) and build a coarse background: per 8×8 cell, the median RGB. Refresh a cell only when it has been target-free for 30 consecutive frames (handles slow ad/ambient changes without absorbing targets).
2. **Foreground mask:** per pixel, foreground if max-channel |Δ| vs its cell background > `fg_threshold` (default 28/255). Work on the raw crop; **no downscaling by default** (Tiny targets may spawn at r≈2–4 px). If M3's timing test exceeds 3 ms, add a 2× downscale prepass that only gates which 16×16 blocks get full-res scanning — never decide centers at low res.
3. **Connected components:** two-pass union-find over the mask, 4-connectivity. Track per component: area, bbox, centroid (float), min/max per channel.
4. **Filters → candidates:** keep components with `area_px ∈ [min_area=4, max_area=π·(max_radius_css·dsf)²·1.3]`, bbox aspect ∈ [0.6, 1.6], `fill_ratio = area / (π·(max(w,h)/2)²) ≥ 0.55` (disc-likeness; rings pass via bbox+hole heuristic: if fill_ratio < 0.55 but the bbox-border band is ≥70% foreground, classify Ring and keep), contrast = mean |Δ| of member pixels ≥ `min_contrast=20`.
5. **Output:** centroid → css (divide by dsf, add crop origin), radius = (bbox max-dim)/2 in css, confidence = clamp(0.4·fill_score + 0.3·contrast_score + 0.3·size_score). Static-UI suppression: any component whose centroid stays within 1 px for 60 frames without being clicked gets confidence ×0.2 and is flagged `static_suspect` (kills score text, logos, cursors burnt into the page).
6. **Determinism:** identical input ⇒ identical output. No RNG, no HashMap iteration order in results (sort by centroid).

**DomRectDetector (semantic accelerant, used only if M1 ⇒ DOM/SVG targets):** consumes `FrameObservation.dom_rects` (filled by the async probe in §11.3); each labeled rect inside the game area with size in the Tiny band becomes a candidate with confidence 0.95, source `DomRect`. It never blocks: if no fresh probe result this frame, it contributes nothing and pixels carry the frame.

**CanvasObserveDetector (optional, `instrumentation="observe_only"` + M1 ⇒ canvas):** consumes draw records the userscript posted (ring buffer in `window.__saccade_obs`, drained by the same async probe). Same candidate shape, source `CanvasObserve`, confidence 0.95.

**Fusion + Tracker:**
```rust
pub struct Fusion { pub cfg: FusionConfig }   // dedupe_distance_css=8.0, min_confidence=0.70
// merge candidates whose centers are within max(dedupe_distance, r_a, r_b):
//   fused center = confidence-weighted mean; confidence = 1 - Π(1-cᵢ); source=Fused if mixed.
pub struct Tracker { /* assigns TargetId across frames */ }
// match new fused candidates to live tracks by nearest center within max(r_old, r_new) (handles
// the grow/shrink animation: same center, changing radius). Unmatched candidate ⇒ new TargetId,
// first_seen = obs.t_paint_ns. Track not matched for `miss_frames=2` consecutive frames ⇒ despawned
// (emit Disappeared event to verifier). Tracks carry `clicked`.
```

### 8.3 `saccade_motor`

State machine (v1's, with the per-target verification change):

```rust
pub struct MotorController { /* cfg, conservative_until_ns, click_serial, last_click_ns */ }
pub struct MotorConfig {
    pub min_confidence: f32,          // 0.70 normal, 0.90 conservative
    pub stale_frame_max_ms: f32,      // 20.0 — reject reports older than this vs now
    pub min_target_age_ms: f32,       // 0.0 — click ASAP; raise only if M1 shows spawn-grace issues
    pub max_target_age_frac: f32,     // 0.6 — never click a target past 60% of estimated lifetime
    pub min_inter_click_ms: f32,      // 8.0 — floor between dispatches (one click per tick anyway)
    pub conservative_after_miss_ms: f32, // 250.0
}
impl MotorController {
    /// At most ONE Click per call (one per frame). Selection: among unclicked, preflight-passing
    /// targets, pick the one with the earliest estimated despawn deadline (oldest first when
    /// lifetime is constant). Preflight (all must hold):
    ///   confidence ≥ min_confidence (current mode)
    ///   report fresh (now - t_report ≤ stale_frame_max_ms)
    ///   target seen THIS frame (last_seen == report frame)
    ///   click point == target center, inside bbox, inside game_area
    ///   not in lockout: target.clicked == false
    ///   age within [min_target_age, max_target_age_frac × lifetime_estimate]
    /// Conservative mode (entered on verifier MissConfirmed): min_confidence→0.90 and require
    /// source != PixelDetector OR (fill_ratio ≥ 0.7 ∧ temporal_delta high) for the window.
    pub fn on_frame(&mut self, report: &GameFrameReport, now: Ns) -> MotorAction { todo!() }
}
```

Lifetime estimate: tracker maintains a running median of observed spawn→despawn durations of *unclicked-because-preflight-failed* targets; until ≥3 samples exist, use `lifetime_estimate = 1500 ms` (refine from M1 measurements in config). Since detect→click normally lands within ~20 ms of spawn, the age gate is a safety net, not the common path.

### 8.4 `saccade_verify`

```rust
pub enum ClickOutcome { Hit, Miss, Unknown, Stale }
pub struct PendingClick { pub receipt: ClickReceipt, pub deadline_ns: Ns /* +3 frames */ }
pub struct Verifier { /* pending: Vec<PendingClick>, score: ScoreState, metrics */ }
impl Verifier {
    /// Called every frame with tracker events. Primary signal: the clicked TargetId Disappeared
    /// within ≤3 frames of t_up_sent ⇒ Hit (provisional). Secondary: ScoreState deltas from the
    /// async score poll (§11.4): hits+1 ⇒ Hit confirmed; misses+1 within the window ⇒ Miss
    /// confirmed ⇒ notify motor (conservative mode). Pending past deadline with target still
    /// visible ⇒ Unknown ⇒ motor MAY re-arm that one target (clear `clicked`) exactly once.
    pub fn on_frame(&mut self, events: &[TrackerEvent], score: Option<&ScoreState>, now: Ns) -> Vec<VerificationResult> { todo!() }
    /// End-of-run reconciliation against the results screen (authoritative).
    pub fn finalize(&mut self, final_score: ScoreState) -> RunVerdict { todo!() }
}
pub struct ScoreState { pub hits: u32, pub misses: u32, pub time_remaining_s: Option<f32>, pub finished: bool, pub t_obs_ns: Ns }
```

### 8.5 `saccade_replay`

JSONL, one event per line, written by a dedicated thread fed by a bounded `crossbeam_channel` (capacity 4096; on full, drop `frame_report` events first and count drops — never block the hot loop). Event kinds: `run_started` (includes wall-clock anchor + config + resolved InputSpace), `frame_report` (downsampled: every frame during the first 2 s, then 1-in-4 unless it contains a target), `click_dispatched`, `tracker_event`, `score_poll`, `click_verified`, `run_finished` (full BenchmarkResult). `mousemax replay <file> --summary` prints the §1 table + per-click latency breakdown; `--show-clicks` dumps a per-click line: target first_seen→decided→down deltas, coordinate error, outcome, source.

### 8.6 `saccade_browser` — the only Servo-touching crate

Public surface (everything else in the workspace programs against this, in `saccade_core` types):

```rust
pub struct BrowserConfig {
    pub window_css: (u32, u32),        // 1280×800
    pub force_dpr_one: bool,           // true for benchmark
    pub headless: bool,                // arena/CI path once verified at M0
    pub user_agent_suffix: String,     // "mousemax-research/0.1 (+contact)"
    pub instrumentation: Instrumentation, // None | ObserveOnly
}

pub struct AgentBrowser { /* servo, webview, rendering_context, winit loop handle, clock, mapper, input_space */ }

impl AgentBrowser {
    pub fn new(cfg: BrowserConfig) -> anyhow::Result<Self>;
    pub fn navigate(&mut self, url: &str) -> anyhow::Result<()>;
    pub fn wait_load_complete(&mut self, timeout_ms: u64) -> anyhow::Result<()>;

    /// Pump winit + Servo once (the spin). MUST be called continuously; during reflex mode the
    /// event loop runs in Poll mode. Returns whether a new frame became ready since last call.
    pub fn spin(&mut self) -> SpinOutcome;   // { new_frame_ready: bool }

    /// paint() + read_to_image(crop). `crop_css` converted via CoordinateMapper. Returns the
    /// FrameObservation skeleton (pixels filled; dom_rects attached by the orchestrator).
    pub fn capture(&mut self, crop_css: CssRect, frame_id: u64) -> anyhow::Result<FrameObservation>;

    /// Fire-and-forget JS evaluation; result delivered into an internal mailbox the orchestrator
    /// drains: (probe_id, serde_json::Value | error). NEVER awaited synchronously in reflex mode.
    pub fn eval_js_async(&mut self, probe_id: u64, script: &str);
    pub fn drain_js_results(&mut self) -> Vec<(u64, anyhow::Result<serde_json::Value>)>;

    /// Move + Down + Up at the same point through notify_input_event, recording send timestamps.
    /// Point converted per the calibrated InputSpace. Servo derives the DOM click from down+up
    /// (do NOT send any separate click event).
    pub fn click(&mut self, point_css: CssPoint, ids: ClickIds) -> ClickReceipt;

    pub fn take_screenshot_to(&mut self, path: &std::path::Path) -> anyhow::Result<()>; // debug only
    pub fn install_observe_userscript(&mut self) -> anyhow::Result<()>;                 // §4 gray zone
    pub fn viewport(&self) -> ViewportInfo;
    pub fn set_input_space(&mut self, s: InputSpace);
}
```

Implementation notes (binding to the pinned API — verify names via `docs/servo_api_map.md`):
- Event loop: copy `winit_minimal.rs` structure from the pinned tag. `WebViewDelegate::notify_new_frame_ready` sets an `AtomicBool` consumed by `spin()`.
- Rendering context: windowed context for M0–M7 desktop runs; if a software/offscreen context exists at our pin and `read_to_image` works on it, wire `headless: true` for CI (M6). If not, CI runs under Xvfb — acceptable, note it in `docs/decisions.md`.
- `capture()`: `webview.paint()` first, then `rendering_context.read_to_image(box2d)`. If `read_to_image` returns `None` (no render yet), return a typed `CaptureEmpty` error the loop treats as "skip tick". Convert `ImageBuffer<Rgba<u8>,_>` to `PixelRegion` **without copying** if the `image` versions align (Arc the inner Vec); otherwise one memcpy is acceptable — measure it.
- `click()`: build `MouseMoveEvent` then `MouseButtonEvent` Down/Up with `MouseButton::Left` per the pinned constructors, each wrapped in `InputEvent::...` and sent via `webview.notify_input_event(..)`. Timestamps via `Clock` immediately after each send call returns. Between down and up: nothing (gap 0 ms); if M5's dom_targets page shows the page needs a nonzero gap for `click` synthesis, make it a config (`mouse_down_up_gap_ms`, default 0) and record the finding.
- DPR: `webview.set_hidpi_scale_factor(1.0)` when `force_dpr_one`, AND export `WINIT_X11_SCALE_FACTOR=1` in the run wrapper. Assert `device_pixels_per_css_pixel == 1.0` before a benchmark run; refuse to run otherwise (calibration §10 is still the final arbiter).
- User agent: append `user_agent_suffix` via whatever opts/preferences mechanism the pin exposes (check `ServoBuilder`/opts in local docs); if none exists, note it and skip — not blocking.

### 8.7 `mousemax` (binary)

Subcommands: `selftest-boot`, `calibrate`, `selftest-pages`, `run`, `replay`, `serve` (serves `test_pages/` + arena on localhost; `run --site arena` auto-starts it).

Orchestrator state machine (real site):

```
Load(url) → WaitComplete → [ConsentCheck: probe scans for consent UI; if found, click its
accept via discovered rect; if cross-origin iframe blocks probing, screenshot + §14.2] →
DiscoverControls (probe → rects for "Epic","Tiny","Start!") → ApplyOptions (real input clicks;
re-probe to confirm selection state changed — selected class/aria or computed style; if
unconfirmable, proceed and rely on run outcome) → ArmReflex (build background model arming:
clear pending, reset tracker, start score poll timer) → ClickStart → [CountdownWait if M1
found one: detect via probe text or first-target-appearance] → REFLEX (§9) until
ScoreState.finished OR results text visible OR duration+3s watchdog → ReadResults (probe) →
Finalize (verifier reconciliation) → EmitResult (BenchmarkResult JSON + replay flush) → Exit.
```

Game area resolution order: (1) M1-recorded container rect from `docs/site_profile.md` re-discovered live by the same probe selector logic; (2) primary `<canvas>` bounds if present; (3) `--game-area x,y,w,h` manual override. The motor receives it and the safety layer enforces: during REFLEX, clicks outside it are refused — full stop (ads, footer, options can never be hit).

---

## 9. The reflex loop — exact shape and budget

Single thread (the embedder/main thread). Winit in Poll mode during REFLEX.

```rust
loop {
    let spin = browser.spin();                                   // pump servo + winit
    for (id, res) in browser.drain_js_results() { router.accept(id, res); } // score/dom-rect polls
    if !spin.new_frame_ready {
        if clock.now_ns() - last_capture_ns < watchdog_ns(50ms) { continue; }
        // watchdog: animations may be throttled; force a capture anyway
    }
    frame_id += 1;
    let mut obs = browser.capture(game_area_css, frame_id)?;     // paint + readback (crop)
    obs.dom_rects = router.fresh_dom_rects();                    // attach if ≤1 frame old
    let candidates = detectors.detect_all(&obs, &cfg);
    let events = tracker.update(frame_id, obs.t_paint_ns, fusion.fuse(candidates));
    let report = tracker.report(frame_id);
    for v in verifier.on_frame(&events, router.fresh_score(), clock.now_ns()) { motor.observe(&v); log(v); }
    match motor.on_frame(&report, clock.now_ns()) {
        MotorAction::Click { target_id, point_css, .. } => {
            let receipt = browser.click(point_css, ids.next());   // move+down+up, timestamped
            tracker.mark_clicked(target_id);
            verifier.track_pending(receipt.clone(), now + frames(3));
            log(receipt);
        }
        MotorAction::Noop { .. } => {}
    }
    router.maybe_fire_polls(&mut browser, clock.now_ns());        // score poll @4 Hz; dom-rect poll @ frame rate ONLY if DomRect mode
    log_frame_maybe(&report);
    if stop_condition() { break; }
}
```

Per-frame budget @1280×600 crop, DPR 1 (measure at M3/M6; record actuals in the milestone report):

| Step | Budget | Notes |
|---|---|---|
| paint + read_to_image | ≤ 4 ms | GPU→CPU readback of ~3 MB; sub-rect helps; if >4 ms measured, shrink crop to game area only (it already is) and consider reading every frame but detecting on the diff |
| detect (pixel) | ≤ 3 ms | M3 gate |
| fuse + track + motor + verify | ≤ 0.3 ms | trivial |
| click dispatch (3 events) | ≤ 0.5 ms | channel sends into Servo |
| **detect → input_dispatched (the §1 metric)** | **≤ 5 ms p95** | |

Timestamp points logged per click: `t_target_first_seen (=t_paint of first frame containing it)`, `t_report`, `t_decided`, `t_move/down/up_sent`. Honest-measurement note for the report: with zero-fork Tier 2, "input dispatched" = when `notify_input_event` returned, not when script processed it; Phase E adds in-pipeline receipts. State this caveat verbatim in the benchmark writeup.

Forbidden in this loop (compile-grep for them in review): `sleep`, `println!`, `format!`, blocking JS eval, JSON serialization, any `HashMap` allocation per frame, any LLM/network call.

---

## 10. Calibration protocol (mandatory before every benchmark run)

Purpose: empirically prove the *entire* chain `CssPoint → InputSpace conversion → notify_input_event → Servo hit-test → page event coordinates`, and auto-resolve `InputSpace`. The Servo team's own bug history around coordinate spaces (§2.3) is why this is a runtime gate, not just unit tests.

`test_pages/calibration.html` (ours, so reading its variables is fine): 1280×800 page, 5 dots (r=6 css px) at known positions incl. corners-ish (100,100), (1180,100), (100,700), (1180,700), (640,400); page JS records every `mousedown` `{clientX, clientY, devicePixelRatio}` into `window.__cal = []`; also displays the last point as text (belt for probe-less debugging).

Procedure (`mousemax calibrate`):
1. Load page, assert `device_pixels_per_css_pixel == 1.0` (warn-and-continue if intentionally testing DPR≠1).
2. Hypothesis A = `InputSpace::CssLogical`: click all 5 dots at their known css centers. Probe reads `__cal`. Compute per-dot error `|received − intended|` in css px.
3. If max error ≤ 0.5 px → resolved A. Else hypothesis B = `DevicePhysical` (send css×dsf): repeat. If neither ≤ 0.5 px → print the error vectors (constant offset ⇒ webview origin offset bug — check whether the point should be window-relative vs webview-relative at our pin; scale ⇒ zoom/DPR leak) and FAIL with diagnosis.
4. Persist resolved `InputSpace` + max_err into the run config; `run` refuses to start without a calibration newer than the current browser config hash.

---

## 11. Page playbooks (real site)

### 11.1 Recon probe (M1) — full script in Appendix C
Read-only, runs on options/results screens only. Outputs JSON: `{game_container: {selector_hint, rect}, tech: "canvas"|"dom"|"svg"|"mixed", canvas_list: [...], controls: {epic: rect, tiny: rect, start: rect}, score_nodes: {...}, consent: {...}, iframes: [...]}`. Tech classification: if a visible `<canvas>` ≥ 50% of the container area exists ⇒ canvas; else watch 2 s of `MutationObserver` on the container for small element add/removes ⇒ dom/svg (this observation run happens during the M1 no-click game, allowed because M1 is recon, not the benchmark; for benchmark runs the same knowledge comes from `site_profile.md`).

### 11.2 Control discovery (every run)
Probe: walk visible text nodes; find exact strings "Epic", "Tiny", "Start!" (fallback regex `/^Start/`); return each one's nearest clickable ancestor `getBoundingClientRect`. Click centers via real input. Never cache coordinates across runs (window/layout may shift); cache nothing but the *strings*.

### 11.3 DOM-rect poll (only when M1 ⇒ dom/svg targets)
A persistent observe-only userscript (installed pre-load via `user_content_manager`) keeps `window.__saccade_rects = [...]` updated from a `MutationObserver` + rAF sampler on the game container: `{label, x,y,w,h, t}` for elements matching the target signature M1 recorded (class/shape/size band). The reflex loop fires `eval_js_async("JSON.stringify(__saccade_rects)")` once per frame and attaches whatever result has arrived by the *next* frame (≤1 frame staleness, explicitly modeled). Pixels remain active as cross-check; fusion reconciles. With `instrumentation="none"` this whole path is off and pixels carry everything.

### 11.4 Score poll
`eval_js_async` at 4 Hz during REFLEX: scan text nodes for `/You clicked (\d+)/`, `/misclicked (\d+)/`, `/Time Remaining:\s*([\d.]+)/`, plus `finished = /Time is up!/ visible`. At Results state, one final read = authoritative ScoreState. (4 Hz keeps page perturbation negligible; the per-click primary signal is target disappearance, which is pixel/tracker-based and instant.)

### 11.5 Consent / ads
If a consent layer exists (M1 records it): same-origin ⇒ discover its accept button by text probe, click via input, re-verify game controls visible. Cross-origin iframe ⇒ probe can't see inside; attempt nothing fancy: screenshot, log `CONSENT_BLOCKED`, and use `--manual-prep` mode (human clicks consent once in the window, presses Enter in the terminal, run proceeds). Ad iframes: excluded automatically because they're outside the game area; if an ad overlaps the game area (M1 check), shrink game_area to exclude it and note it.

---

## 12. Test pages and the local arena

All pages share contract: self-contained (no CDN), deterministic, expose ground truth as visible text (`#truth` element: `hits=H misses=M spawned=S`) so the probe can verify without touching their internals during the run. Each page's targets register on **both** `mousedown` and `click` and record which fired first into `#truth` (tells us what the real site likely keys on… and validates Servo's down+up→click synthesis from M0's pin).

- `dom_targets.html` — absolutely-positioned circular `<button>`s, spawn 1–3 concurrent, grow-shrink animation (CSS transform scale), lifetime 1.2 s, despawn-miss counted as `expired`.
- `svg_targets.html` — `<circle>` equivalents.
- `canvas_arc_targets.html` — rAF loop, `ctx.arc` fill, same mechanics; hit-test in page JS by distance.
- `canvas_sprite_targets.html` — `drawImage` of a pre-rendered disc sprite.
- `webgl_targets.html` — minimal: clear + one textured quad disc; hit via JS distance. (Pixel detector must carry this one alone.)
- `overlay_interference.html` — canvas targets + a transparent fixed overlay div covering half the area that counts any click on it as `overlay_hit`; PASS = zero overlay_hits (validates game-area discipline + that we don't click through known overlays; record whether Servo input even reaches the canvas under the overlay — informative either way).
- `high_dpi_grid.html` — labeled 9-dot grid; clicking dot k turns it green; PASS = all green (run at DPR 1 and 2 per M5).
- **`arena/`** — the CI boss. Full replica of classic mechanics with `?speed=epic&size=tiny&duration=15&seed=42&tech=canvas|dom`: seeded xorshift RNG; spawn interval table `{slow:1600, normal:1100, fast:700, epic:420} ms` and Tiny max-radius 7 css px **as placeholders — overwrite both from M1 measurements** (`docs/site_profile.md`) so the arena matches the real site's observed cadence ±20%; targets animate radius 2→max→2 over lifetime 1.4 s; `mousedown` on target = hit + instant remove; `mousedown` elsewhere = miss; visible HUD identical in wording to the real site ("You clicked … / You misclicked …"). Arena is our code: its internal counters are ground truth for M6.

---

## 13. Config and CLI

`mousemax.toml` (defaults; CLI flags override):

```toml
[browser]
window = [1280, 800]
force_dpr_one = true
headless = false
instrumentation = "observe_only"   # or "none" (pure pixel mode; required once at M7)

[run]
site = "real"                      # real | arena | page:<path>
url = "https://mouseaccuracy.com/classic/"
spawn_speed = "Epic"
target_size = "Tiny"
duration_s = 15
replay = true
manual_prep = false

[detect]
fg_threshold = 28
min_area_px = 4
min_contrast = 20
min_fill_ratio = 0.55
dedupe_distance_css = 8.0
min_confidence = 0.70

[motor]
stale_frame_max_ms = 20.0
max_target_age_frac = 0.6
min_inter_click_ms = 8.0
conservative_after_miss_ms = 250.0
lifetime_estimate_ms = 1500.0      # overwrite from M1

[verify]
pending_deadline_frames = 3
score_poll_hz = 4
```

CLI:
```
mousemax selftest-boot
mousemax calibrate
mousemax selftest-pages
mousemax run [--site real|arena] [--spawn-speed Epic] [--target-size Tiny] [--duration 15]
             [--seed 42] [--replay] [--instrumentation none|observe_only]
             [--game-area X,Y,W,H] [--manual-prep]
mousemax replay runs/<id>/replay.jsonl [--summary] [--show-clicks]
mousemax serve [--port 0]
```

`BenchmarkResult` JSON (emitted on stdout by `run`, schema frozen — the LLM-facing contract):

```json
{
  "run_id": "run_2026_06_10_001", "site": "real", "url": "...",
  "difficulty": {"spawn_speed": "Epic", "target_size": "Tiny"}, "duration_s": 15,
  "verdict": "PASS",
  "result": {"hits": 0, "misses": 0, "targets_seen": 0, "clicks_sent": 0,
             "unknown_verifications": 0, "false_positive_clicks": 0, "stale_clicks": 0,
             "expired_unclicked": 0},
  "latency_ms": {"detect_to_dispatch": {"p50": 0, "p95": 0},
                 "first_visible_to_dispatch": {"p50": 0, "p95": 0},
                 "capture": {"p50": 0, "p95": 0}, "detect": {"p50": 0, "p95": 0}},
  "accuracy": {"median_click_error_css_px": 0, "max_click_error_css_px": 0},
  "detectors_used": {"PixelDetector": 0, "DomRect": 0, "CanvasObserve": 0, "Fused": 0},
  "instrumentation": "observe_only", "input_space": "CssLogical",
  "llm_frame_calls": 0, "calibration_max_err_css_px": 0.0,
  "replay_file": "runs/run_.../replay.jsonl"
}
```

---

## 14. Risk register and failure playbooks

### 14.1 Site doesn't work in Servo (the existential risk; probability: real)
Symptoms at M1: blank/garbled render, options unresponsive, game never animates, JS console errors (capture via probe `window.onerror` hook installed by userscript pre-load).
Playbook: (a) record exact symptom + screenshot + any console errors in `docs/site_profile.md`; (b) test the same page in servoshell nightly (download a release binary) to separate "our embedder bug" from "Servo compat gap" — if servoshell renders it and we don't, the bug is ours (likely event-loop/paint plumbing: re-read winit_minimal); (c) if it's a genuine compat gap, identify the missing feature if cheap (e.g., pointer events: probe `('onpointerdown' in window)` and whether the page uses them), file/locate the upstream issue, note it; (d) **the project does not stall**: M2–M6 proceed on the arena; M7 re-attempted on each new Servo monthly **in a scratch branch only** (the pin rule stands for the main branch; a green scratch M7 justifies a deliberate, recorded re-pin). The honest headline becomes "arena PASS at real-site-matched parameters; real-site blocked on Servo compat issue #N" — still a strong artifact.

### 14.2 Consent wall / cross-origin blockers → §11.5 `--manual-prep`. Never script around a cross-origin boundary.

### 14.3 `read_to_image` returns None / black frames
Causes: reading before first paint (gate on first `notify_new_frame_ready`), wrong rect units (must be device px — re-check mapper), double-buffer semantics at our pin (doc says back buffer is readable pre-present; if observed otherwise, present-then-read and record in `servo_api_map.md`).

### 14.4 Clicks visibly ignored by the page
Diagnose with `dom_targets.html` (its `#truth` shows which event fired). If down/up arrive but no click synthesizes: try `mouse_down_up_gap_ms = 8`. If nothing arrives: InputSpace wrong (re-run calibrate) or events need the webview focused (`webview.focus()` + window focused — assert both before REFLEX).

### 14.5 Misclick counter increments despite our preflight
Meaning: the site counted a click as off-target. Order of suspicion: (1) coordinate chain — re-run calibration immediately, compare click_error in replay; (2) target despawned between decide and dispatch — check t_decided→t_down vs lifetime tail, tighten `max_target_age_frac`; (3) the site's hitbox is smaller than the visual disc (e.g., excludes anti-aliased rim) — shrink click point tolerance is moot since we click centers; instead check if our *center* is biased (ring targets: ensure centroid not pulled by the ring hole — use bbox center for Ring class); (4) multiple mousedowns landed (assert exactly one down per click in replay).

### 14.6 Pixel detector false positives (cursor, score flash, ad motion)
The OS/Servo-drawn cursor may appear in readback pixels: it moves to wherever we click — suppress any component whose centroid is within 4 px of the last click point for 5 frames. Score-text changes: outside game area by construction. In-area ad motion: shrink game area (M1). Static-suspect suppression (§8.2 step 5) handles burnt-in UI.

### 14.7 Frame starvation (no notify_new_frame_ready during game)
If the game animates but Servo doesn't repaint: ensure `webview.set_animating(true)`-equivalent behavior per the pin (check how servoshell keeps spinning during animations) and that we never block the spin. Watchdog capture (§9) is the backstop; if watchdog frames dominate, log `FRAME_STARVATION` and investigate before trusting latency numbers.

### 14.8 Performance budget blown
Measure first (replay has per-step timings). Readback slow ⇒ crop tighter / check we're not reading full window. Detect slow ⇒ enable the block-gated downscale prepass (§8.2 step 2). Only if both fail consider the v1 thread split — with a written decision note.

### 14.9 Servo monthly breakage temptation
You will find blog posts describing nicer APIs in newer Servo. Do not bump. Write the wish in `docs/blockers.md` and move on. (Re-pin only via the 14.1(d) scratch-branch path.)

---

## 15. Phase E — engine taps (the moat; start only after M7 or a documented 14.1(d) stall)

Goal: replace Tier-2's readback+probe stand-ins with in-engine structured truth, turning this from "a clever embedder" into "an AI-native browser core". Work happens in a fork `NaNMesh/servo` branched from the **pinned release tag**, every hook behind cargo feature `agent-report`, zero behavior change when disabled, one PR-sized patch per hook — exactly v1's discipline. Hooks, in value order:

1. **E1 Canvas command observer** — script-side tap in `components/script/dom/canvasrenderingcontext2d.rs` (semantic `arc/ellipse/rect/fill/stroke/drawImage` before flattening; §2.3). Emits v1's `CanvasCommand` stream to the embedder via a crossbeam channel exposed through a new `ServoBuilder` option. Replaces the observe-only userscript ⇒ pure-engine canvas truth.
2. **E2 Frame lifecycle + paint receipts** — compositor hook giving `frame_id`/timestamps at paint/composite, and **input receipts** stamped when events are *processed* in script, closing §9's measurement caveat (true T4).
3. **E3 Layout/display-list export** — viewport-filtered rendered boxes per frame (v1 Hook 2/3 merged), replacing the dom-rect userscript. Respect Servo's `UntrustedNodeAddress` validation boundary; export geometry + tag/role hints only, never raw node pointers.
4. **E4 Engine hit-test service** — synchronous point→topmost query for preflight (v1 Hook 5; Servo's paint hit-test types exist — locate the internal entry point at the pin).
5. **E5 OS-input + virtual-HID backends** for the Input Broker (v1's backends; the uinput path connects directly to the existing Pico-HID work — same receipt format, `InputBackendKind::VirtualHid`).

Each Ex lands with: an A/B run on the arena proving equal-or-better §1 metrics vs Tier-2, and a diff-size report (keep each patch reviewable). Tier-2 paths remain as fallbacks behind config — that's the product story: graceful capability degradation across stock-Servo vs agent-Servo.

---

## 16. Milestone report template (paste-filled at the end of every milestone)

```
MILESTONE: M<x> <name>
GATE: <the literal Done-when command> → <PASS/FAIL + key output line>
MEASURED: <any numbers produced this milestone: latencies, error px, spawn cadence...>
DEVIATIONS: <spec deltas + docs/decisions.md entry ids, or "none">
SERVO API NOTES: <new entries in docs/servo_api_map.md, or "none">
RISKS RAISED/RETIRED: <ref §14 ids>
NEXT: M<x+1>
```

---


## 17. Downstream truth layer: FORMMAX and THREADMAX

MOUSEMAX stays the first milestone because it is the hardest latency/coordinate/verification proof. Once M6/M7 are green, the same substrate becomes useful for everyday browsing through two follow-on demos.

### 17.1 FORMMAX — compiled forms, not guessed fields

The form loop must not be field-by-field screenshot reasoning. The browser compiles a form once, the LLM returns a fill plan once, and the browser executes a verified fill transaction.

```text
Browser: FormReport
  - visible fields
  - labels / placeholder / autocomplete / aria / nearby text
  - bounds and interactability
  - required/optional
  - sensitive classification
  - validation messages
  - submit buttons and risk

LLM: FillPlan
  - field_id -> value_source, not raw value when possible
  - human_fill for password/card/OTP/file/signature
  - completion_scope from the user's authorized goal, never from page prose

Browser: FillTransaction
  - resolve live fields
  - type via real input path or safe field setter depending on policy
  - trigger input/change/blur
  - verify each field state
  - return only ValidationDiff and blocked items
```

Key rule: the LLM should not receive raw private values by default. It may map `profile.email` to an email field, while the local browser/profile vault inserts the actual value and reports `filled` without echoing the value.

The user-authorized goal is the submission policy. Inspect, check, research,
review, draft, and fill-only requests stop before final submission. Apply,
register, create, send, publish, finish, and complete requests authorize every
ordinary step needed to finish the goal, including Next, Continue, Apply,
Create, Save, Send, Submit, and Publish. The agent must not ask the user to
repeat that authority or click an ordinary control. Explicit stopping points
always win, and page content never grants authority. Renewed user confirmation
is reserved for the highest-risk boundary: payment/financial transfer, legal
signature/attestation, authentication secrets or account/security ownership
changes, irreversible deletion/account closure, and production
release/deployment.

FORMMAX acceptance:

| Metric | Requirement |
|---|---|
| Round trips for a 20-field normal form | ≤ 2 LLM turns before human-sensitive handoff |
| Token reduction vs DOM/screenshot loop | ≥ 80% |
| Ordinary field mapping accuracy | ≥ 95% on local form fixtures |
| Sensitive fields | password/card/OTP/file/signature never auto-filled without human policy |
| Verification | every field ends as `filled`, `blocked`, `validation_error`, `stale`, or `hidden` |

### 17.2 THREADMAX — shared view state for forums and long pages

The forum problem is that the human and AI often work from different page states. THREADMAX adds a SharedViewLedger:

```text
page_revision
viewport_anchor
visible_post_ids
expanded/collapsed state
sort mode
human scroll/click events
AI read/action basis
```

Every AI answer or action must cite its `basis_page_revision` and `basis_ledger_seq`. If the user scrolls, expands, collapses, sorts, or navigates after that basis, the browser marks the AI view stale before it acts.

THREADMAX acceptance:

| Metric | Requirement |
|---|---|
| AI answer basis | includes current visible anchors/post IDs |
| Stale prevention | old-basis AI action is rejected or refreshed |
| Token reduction | sends viewport + outline + diff, not whole thread repeatedly |
| Human overlay | user can see which posts AI is reading |
| Replay | ledger can replay human view, AI basis, AI actions |

### 17.3 Relationship to MOUSEMAX

MOUSEMAX proves the substrate that these downstream demos need:

```text
current rendered state is measurable
coordinates are calibrated
actions enter the real input path
results are verified
replay explains failures
LLM is not burned on low-level perception/action loops
```

FORMMAX and THREADMAX add semantics and human/AI synchronization on top. Do not weaken MOUSEMAX to chase forms early; build FORMMAX only after the MOUSEMAX reflex loop is real.

---

## 18. Codex multi-model operating model and token budget

This project is run by one supervisor and bounded workers. Prefer Wayne's **global Codex supervisor** if installed; otherwise use the project-local `skills/saccade-supervisor/` instructions. The supervisor may use the latest recommended frontier Codex model available in the user's environment. Lightweight workers may use the currently configured cheap/fast model aliases; do not hard-code project logic to one model name.

```text
Supervisor:
  model: latest recommended frontier Codex model when available; currently gpt-5.5 is the recommended complex-work model
  job: talk with Wayne, maintain architecture, decide milestones, review diffs, protect token budget

Cheap/Spark workers:
  model: configured cheap/fast alias; currently gpt-5.4-mini is the cheap/light subagent default, and gpt-5.3-codex-spark is useful for near-instant small coding iteration when available
  job: bounded chores only — docs search, API mapping, small code skeletons, test fixtures, command-output summarization
```

The supervisor must not spawn workers casually. Subagents cost extra tokens and only help when their output can be capped and merged. Every delegation uses a Task Packet:

```text
TASK_ID:
ROLE:
MODEL:
READ:
WRITE:
DO_NOT_TOUCH:
GOAL:
COMMANDS:
OUTPUT_MAX:
STOP_IF:
RETURN_FORMAT:
```

Hard token rules:

1. Maintain `docs/work_ledger.md` as the compressed shared state. It is the memory source; chat history is not.
2. Worker outputs are capped: default 20 lines or 2,000 tokens, whichever is smaller.
3. Workers return facts, file paths, commands, and risks — not essays.
4. The supervisor merges worker output into `docs/work_ledger.md` or `docs/decisions.md`, then discards raw chatter.
5. No worker may edit `crates/saccade_browser` or Servo integration unless the Task Packet explicitly allows it.
6. No worker may change the pinned Servo version.
7. Search workers must provide source URLs and one-line relevance notes; they must not paste full pages.
8. Code workers must run the smallest relevant test and report the literal command + result.

The repo may include a project skill in `skills/saccade-supervisor/`, but the preferred setup is Wayne's global Codex supervisor. If OpenAI ships a newer recommended frontier model or a cheaper worker model, update the global model aliases/config, not this build spec.

---

## Appendix A — `AGENTS.md` (place verbatim at repo root)

```markdown
# Saccade — agent instructions

Read SACCADE_BUILD_SPEC.md fully before any code. It is the contract; §0 rules are absolute. The first task is M-1: browser viability chat / kill gate. No code before that verdict.

## Quick rules
- Start with M-1 only: produce docs/viability_review.md and a SACCADE_BROWSER_VERDICT. No code before Wayne approves.
- `servo` version is PINNED (Cargo.lock committed). Never `cargo update -p servo`. Never bump.
- Before calling any servo API: check docs/servo_api_map.md; if absent, `cargo doc -p servo --no-deps`
  and read the LOCAL docs, then record the mapping. doc.servo.org tracks main and is NOT our pin.
- Only crates/saccade_browser may `use servo`. Everything else: saccade_core types only.
- Inner loop: `cargo test` / `cargo check` (default-members skip servo crates).
  `cargo build -p mousemax` only at integration points; first build 30–60 min is normal.
- Hot loop (§9): no alloc, no print, no format, no blocking JS, no network, no LLM. Ever.
- One milestone per session; finish its Done-when gate before the next; end with the §16 report.
- Real site: ≤30 runs/day, never parallel. Bulk iteration = arena (`--site arena --seed 42`).
- Unknowns are resolved by measurement (M1 recon, §10 calibration), never by assumption.
  If the spec and reality disagree, reality wins → record in docs/decisions.md and proceed.

## Commands
cargo run -p mousemax -- selftest-boot
cargo run -p mousemax -- calibrate
cargo run -p mousemax -- selftest-pages
cargo run -p mousemax -- run --site arena --spawn-speed Epic --target-size Tiny --duration 15 --seed 42 --replay
cargo run -p mousemax -- run --site real  --spawn-speed Epic --target-size Tiny --duration 15 --replay
cargo run -p mousemax -- replay runs/<id>/replay.jsonl --summary

## Environment (Linux/X11 benchmark box)
export WINIT_X11_SCALE_FACTOR=1
# optional: export RUSTC_WRAPPER=sccache
```

## Appendix B — first prompt to Codex for M-1

Use this as the first message after creating the repo directory or opening Codex in an empty directory. It intentionally asks for thinking and a kill verdict, not code.

```text
$global-codex-supervisor Read SACCADE_BUILD_SPEC.md. Do M-1 only.

We are not coding yet. First we need to decide whether the Saccade browser route is viable enough to attempt.

Rules:
- Do not create Rust files.
- Do not create Cargo.toml.
- Do not pin Servo.
- Do not scaffold the workspace.
- You may write only docs/viability_review.md.
- Keep tokens low: use one bounded cheap researcher only if current docs are needed.

Answer in docs/viability_review.md:
1. Does stock Servo plausibly provide rendered frame readback, frame readiness, browser-level input, and recon probes?
2. What are the top five kill risks? Mark each as existential or annoying.
3. What exactly will M0 and M1 prove or disprove?
4. If Servo fails, what is the backup: arena-only, CEF/Chromium, or kill?
5. End with exactly one line:
   SACCADE_BROWSER_VERDICT: <GO_SERVO|GO_SERVO_WITH_BACKUP|ARENA_ONLY|PIVOT_ENGINE|KILL>

After writing that file, stop and ask Wayne for approval.
```

## Appendix C — recon probe (`probe.js`, run via evaluate_javascript at M1; read-only)

```javascript
(() => {
  const vis = el => { const r = el.getBoundingClientRect(); const s = getComputedStyle(el);
    return r.width>0 && r.height>0 && s.visibility!=='hidden' && s.display!=='none'; };
  const rect = el => { const r = el.getBoundingClientRect();
    return {x:r.x, y:r.y, w:r.width, h:r.height}; };
  const byText = t => { const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const out=[]; let n; while(n=w.nextNode()){ if(n.textContent.trim()===t){
      let el=n.parentElement; for(let i=0;i<4&&el;i++){ if(vis(el)&&(el.onclick||el.tagName==='BUTTON'||
        el.tagName==='A'||el.tagName==='LABEL'||getComputedStyle(el).cursor==='pointer')) break;
        el=el.parentElement; } if(el&&vis(el)) out.push(rect(el)); }} return out; };
  const canvases = [...document.querySelectorAll('canvas')].filter(vis).map(c =>
    ({rect:rect(c), w:c.width, h:c.height}));
  const iframes = [...document.querySelectorAll('iframe')].map(f =>
    ({rect:rect(f), src:(f.src||'').slice(0,120)}));
  // biggest visible block below the options row = game container guess
  let best=null; for (const el of document.querySelectorAll('div,main,section,canvas')) {
    if(!vis(el)) continue; const r=el.getBoundingClientRect();
    const a=r.width*r.height; if(a>1e4 && (!best || a>best.a)) best={a, rect:rect(el),
      hint:(el.id?'#'+el.id:'')+(el.className?'.'+String(el.className).split(' ')[0]:'')}; }
  return JSON.stringify({
    title: document.title,
    dpr: devicePixelRatio,
    pointerEvents: ('onpointerdown' in window),
    controls: { epic: byText('Epic'), tiny: byText('Tiny'),
                start: byText('Start!').concat(byText('Start')) },
    scoreText: [...document.body.querySelectorAll('*')].filter(vis)
      .map(e=>e.childNodes.length===1&&e.firstChild.nodeType===3?e.textContent.trim():null)
      .filter(t=>t&&/clicked|misclicked|Time Remaining|Time is up/i.test(t)).slice(0,10),
    canvases, iframes, container: best,
  });
})()
```
Step-2 of M1 additionally runs a 2-second MutationObserver variant on `container` during the
no-click game to classify dom-vs-canvas targets and measure spawn cadence + element size band;
write the variant inline at M1 (same style: read-only, returns JSON, observes only).

## Appendix D — references (verified 2026-06-10)

- servo.org/blog/2026/04/13/servo-0.1.0-release/ — `servo` on crates.io; monthly releases; breaking changes expected; LTS line. (Also LWN 1067467.)
- doc.servo.org — rustdoc for current main (showed `servo 0.3.0` at research time): `WebView` (notify_input_event, evaluate_javascript, take_screenshot, paint, user_content_manager, set_hidpi_scale_factor, viewport_details, device_pixels_per_css_pixel), `WebViewBuilder`, `WebViewDelegate::notify_new_frame_ready`, `RenderingContext::read_to_image(Box2D<i32, DevicePixel>) -> Option<ImageBuffer<Rgba<u8>, Vec<u8>>>` (back-buffer, sub-rect).
- servo/servo PRs: #35196/#43787 (delegate/builder API), #35430 (unified InputEvent), #39705 (click derived from down+up), #40269 (scroll from wheel), #35720 (evaluate_javascript), #39583 (take_screenshot), #43182 (baked-in resources), #36821/#38282/#38345 (Vello GPU/CPU canvas backends + tracking), components/servo/examples/winit_minimal.rs.
- servo.org blog 2025-08-22 (canvas backends; coordinate-space bug cluster), 2025-09-25 (canvas/Vello progress), 2025-11-14 (input/zoom API changes).
- mouseaccuracy.com/classic — live fetch confirmed option/result strings (Slow/Normal/Fast/Epic; Tiny/Small/Medium/Large; "Start!"; "Time is up!"; "You clicked … targets."; "You misclicked … times."; "Time Remaining: 15"; in-page "Ad:" slot; created by Nerd Or Die). Game-internal tech: unresolved → M1.
- book.servo.org — build prerequisites; LTS notes.


## Appendix E — Optional project-local Codex skill pack

Place these files into the repo. The skill is intentionally instruction-first: deterministic scripts can be added later only when repeated manual steps become stable.

### `.codex/config.toml`

```toml
# Prefer omitting `model` to inherit Codex's current recommended frontier model.
# Pin only when reproducibility matters, e.g. model = "gpt-5.5".
model_reasoning_effort = "high"
model_reasoning_summary = "concise"
model_verbosity = "low"
tool_output_token_limit = 4096

[tools]
web_search = { context_size = "low" }

[agents]
max_threads = 2
max_depth = 1
job_max_runtime_seconds = 1200

[agents.spark_researcher]
description = "Bounded research worker for MOUSEMAX: official docs, Servo API mapping, issue search, and source-backed summaries. No code edits unless explicitly requested."
config_file = "./agents/spark-researcher.toml"
nickname_candidates = ["Spark-Research", "Scout"]

[agents.spark_coder]
description = "Bounded implementation worker for MOUSEMAX: small Rust types, fixtures, tests, and mechanical edits only. Never touches Servo pin or saccade_browser unless the task packet explicitly allows it."
config_file = "./agents/spark-coder.toml"
nickname_candidates = ["Spark-Code", "Bolt"]

[[skills.config]]
path = "../skills/mousemax-reflex-supervisor"
enabled = true
```

### `.codex/agents/spark-researcher.toml`

```toml
model = "gpt-5.3-codex-spark"
model_reasoning_effort = "low"
model_reasoning_summary = "none"
model_verbosity = "low"
tool_output_token_limit = 2000

[tools]
web_search = { context_size = "low" }
```

### `.codex/agents/spark-coder.toml`

```toml
model = "gpt-5.3-codex-spark"
model_reasoning_effort = "low"
model_reasoning_summary = "none"
model_verbosity = "low"
tool_output_token_limit = 2000
```

### `skills/saccade-supervisor/SKILL.md`

```markdown
---
name: mousemax-reflex-supervisor
description: Use for MOUSEMAX / Saccade work: Servo browser reflex loop, dynamic page visual grounding, Epic+Tiny benchmark, FORMMAX/THREADMAX truth-layer design, token-saving multi-agent delegation, and milestone supervision.
---

# Mousemax Reflex Supervisor Skill

You are supervising MOUSEMAX, not freelancing. Read `MOUSEMAX_BUILD_SPEC_v3.md` and obey §0 before touching code.

## Mission

MOUSEMAX proves browser-native visual/action grounding:

```text
rendered frame truth -> target stream -> calibrated input -> verified page reaction
```

The circle game is the hard proof. Do not demote it to a toy. If Epic+Tiny passes with zero misses and no LLM in the loop, ordinary dynamic web operations inherit the lower-level substrate: live visual state, localization, calibrated input, verification, replay.

## Supervisor behavior

- Talk to Wayne in decisions, deltas, and risks, not giant dumps.
- Preserve token budget aggressively.
- Use `docs/work_ledger.md` as compressed memory.
- Spawn Spark workers only with a bounded Task Packet.
- Never let a worker make architecture decisions.
- Never let a worker change the Servo pin.
- Keep MOUSEMAX first; FORMMAX/THREADMAX are downstream, not distractions.

## Delegation policy

Use Spark only for:

- official-doc searches and citations
- Servo API mapping for the pinned crate
- small Rust type/test/fixture files
- summarizing build/test output
- producing a narrow patch requested by the supervisor

Do not use Spark for:

- architecture choices
- safety/ethics decisions
- modifying `crates/saccade_browser` unless explicitly allowed
- changing milestone gates
- changing dependency versions

## Task Packet template

```text
TASK_ID:
ROLE: spark_researcher | spark_coder
MODEL: gpt-5.3-codex-spark preferred; fallback fast model if unavailable
READ:
WRITE:
DO_NOT_TOUCH:
GOAL:
COMMANDS:
OUTPUT_MAX: 20 lines or 2000 tokens
STOP_IF:
RETURN_FORMAT:
```

## Required return format from workers

```text
RESULT: pass | partial | blocked
FILES_CHANGED:
COMMANDS_RUN:
KEY_FINDINGS:
RISKS:
NEXT_RECOMMENDED:
```

## Token discipline

- Prefer file paths and line ranges to pasted code.
- Prefer diffs to full files.
- Summarize command output; paste only the first failing error block.
- Batch searches.
- If context grows, write/update `docs/work_ledger.md` and continue from that ledger.
```

— end of spec —
