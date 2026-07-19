# Saccade Advantages And Improvement Checklist

Date: 2026-07-13
Status: canonical product, evidence, and release scorecard

Use this document to decide what to build, what to measure, and what Saccade can
claim publicly. The execution queue remains `docs/CURRENT_ACTION_ITEMS.md`.

## Product Definition

Technical definition:

> Saccade is a policy-enforced browser fact and transaction runtime for AI
> agents. It turns a user-granted visible tab into redacted facts, revision-bound
> actions, verified outcomes, and value-free replay.

Initial user-facing product:

> A human-supervised form and document copilot. It handles ordinary fields,
> leaves protected fields and final commitments to the user, and shows what was
> completed, preserved, blocked, or needs attention.

`Browser fact` means the state the browser observed or rendered at a specific
revision. It does not mean that a claim made by a website is true.

## Status Rules

| Status | Meaning |
| --- | --- |
| Proven | A repeatable gate and saved artifact support a scoped claim. |
| Partial | The path works, but coverage, comparison, or product UX is incomplete. |
| Planned | The design exists, but the required product gate has not passed. |
| Routed | A measured limitation sends the task to a declared compatibility path. |

An item is not `Proven` because it worked once by hand. It needs a command,
artifact, measured scope, and release-safe wording.

## Advantage Scorecard

| ID | Advantage | Status | Why it matters | Current evidence | Next proof |
| --- | --- | --- | --- | --- | --- |
| A-01 | Structured-first browser facts with optional screenshots | Proven | The agent can use current page structure, geometry, controls, and actions without inferring every interaction from pixels, then request visual evidence when structure and visible behavior disagree. | `docs/integration_contract_v1.md`; `docs/browser_fact_stream.md`; `docs/ai028_mouseaccuracy_original_gate.md` | Compare the same tasks against screenshot-only automation. |
| A-02 | Field-level protected-value boundary | Proven in the current contract and fixtures | The agent may know that a protected field exists and whether it needs the user without receiving its value. | `docs/ai033_agent_safety.md`; `docs/safety_truth_profile.md` | Expand coverage to shadow DOM, custom widgets, and visual/semantic evasion. |
| A-03 | Explicit current-tab grant with per-session capability | Proven | Access is bound to a user-granted visible tab instead of silently exposing the whole browser profile. | `docs/ai033_agent_safety.md`; `docs/integration_contract_v1.md` | Product-test grant, revoke, expiry, tab close, and recovery with nontechnical users. |
| A-04 | Revision-bound action map and stale-action rejection | Proven | An action prepared for an old page state cannot silently run after navigation or mutation. | `docs/ai033_agent_safety.md`; `docs/integration_contract_v1.md` | Add mutation-heavy and multi-frame adversarial cases. |
| A-05 | Inventory, compile, execute, and postcondition verification for forms | Proven on fixtures and two public test forms | Filling is treated as a checked transaction, not a sequence of optimistic clicks. | `docs/ai031_generic_form_plan.md` | Complete two real user-granted drafts, then a ten-workflow pilot. |
| A-06 | Preserve-existing and no-submit defaults | Proven | The agent can add ordinary data without overwriting user work or making the final commitment. | `docs/ai031_generic_form_plan.md`; `docs/integration_contract_v1.md` | Measure false preserves, false blocks, and user corrections on real forms. |
| A-07 | Value-free receipts and replay | Proven on current gates | Developers and users can inspect what happened without copying protected values into logs. | `docs/integration_contract_v1.md`; `docs/m8_replay_visualization_report.md` | Run an independent leak audit over all report and replay formats. |
| A-08 | Page content is marked untrusted; policy decisions stay outside page prose | Proven on current adversarial fixture | A page cannot authorize a side effect merely by instructing the model to do it. | `docs/ai033_agent_safety.md` | Run AgentDojo and publish utility, attack success, false-block, and leak rates. |
| A-09 | Human and agent can work in the same visible, persistent session | Partial | The user can log in, inspect agent-filled fields, finish protected fields, and keep control of the tab. | `docs/browser_session_report.md`; `docs/profile_persistence_report.md`; `docs/login_handoff_profile.md` | Test login persistence and handoff across ten common workflows and failure states. |
| A-10 | Engine-neutral contract with measured compatibility routing | Partial | Servo can remain the fact/reflex engine while an explicit Chrome route handles measured compatibility blockers under the same grant and policy model. | `docs/integration_contract_v1.md`; `docs/ai030_cloudflare_compatibility_route.md` | Make routing automatic, visible, measurable, and indistinguishable at the host API. |
| A-11 | Large-form inventory with compact and paged output | Proven locally | The bridge can handle forms larger than one model response without exposing values. | `docs/ai031_generic_form_plan.md`; 96 rows, 672 ordinary fields, 3 protected fields blocked | Measure model tokens and completion time against Playwright MCP and screenshot automation. |
| A-12 | Millisecond local reflex path | Proven for the scoped local game gate | Browser facts can drive deterministic actions without placing an LLM in the hot loop. | `docs/local_game_reflex_gate.md`; dispatch p95 `0.091 ms` in the recorded gate | Repeat on release hardware and publish end-to-end perception-to-action latency, not dispatch alone. |
| A-13 | Browser preflight before collecting task values | Partial | The host can detect an incompatible route before asking the user or model for data that cannot be used safely. | `docs/render_preflight.md`; Chrome compatibility route | Productize route explanations and prove no task values enter a failed route. |
| A-14 | Versioned MCP integration contract and typed errors | Proven locally | Host developers get capability discovery, stable boundaries, and examples instead of coupling to one browser engine. | `docs/integration_contract_v1.md`; `docs/integration_examples/`; `docs/release_inventory.md` | Run external TypeScript and Python integrations from a clean machine. |
| A-15 | Evidence-led compatibility ledger | Proven as an engineering practice | Unsupported sites and rendering paths are measured and routed instead of hidden behind a universal claim. | `docs/browser_compat_ledger.md`; `docs/CURRENT_ACTION_ITEMS.md` | Generate a public compatibility matrix from repeatable gates. |
| A-16 | Article and YouTube source packets | Partial, adjacent | Specialized readers can reduce noisy page/video input into auditable text, headings, timestamps, and selected frames. | Article and YouTube dogfood reports under `runs/` | Package the reader, test more sources, and keep it separate from the initial form/document wedge. |
| A-17 | Selective visual capture correlated with fact geometry | Proven in scoped gates; partial as a general product feature | Saccade can capture the user-visible page, translate screenshot/device pixels to CSS coordinates, and compare selected visual regions with browser facts. This gives the agent a second observation channel without making screenshots mandatory or automatically sharing every screenshot with it. | `scripts/probe_mouseaccuracy_original_gate.py`; `scripts/check_human_agent_agreement.py`; `docs/ai028_mouseaccuracy_original_gate.md`; `runs/agreement_gate/` | Measure capture latency and run the generic overlay/disagreement gate on the fixed GitHub canaries. |
| A-18 | Browser-pushed layout invalidation with local semantic rebase | Proven for DOM and stable Canvas-surface targets | After resize, scroll, zoom or target movement, the browser advances a layout epoch and refreshes geometry immediately before input. If the same stable semantic action survives, Saccade rebases locally and verifies the native-input receipt without another screenshot or LLM turn; if it disappears or is covered, input is never sent. | Build 57 source/package matrices and live SimpleMMO: `runs/dogfood/df_build57_layout_epoch_source_20260718/report.json`, `runs/dogfood/df_build57_layout_epoch_packaged_20260718/report.json`, `runs/dogfood/df_build57_resize_live_simplemmo_20260718/report.json`; `docs/ai044_playwright_parity_benchmark.md` | Repeat on a multi-site responsive corpus and add browser-native semantic/pixel truth for targets inside arbitrary Canvas/WebGL scenes. |

## What Is Not Unique By Itself

Saccade should not market common browser-agent features as inventions. Existing
tools already provide parts of this stack:

| Common capability | Existing examples | Saccade must prove beyond it |
| --- | --- | --- |
| Accessibility/DOM snapshots and referenced actions | [Playwright MCP](https://playwright.dev/mcp/introduction) | Redaction, revision binding, verification, receipts, and human ownership in one contract. |
| Persistent profiles and current browser tabs | [Playwright MCP repository](https://github.com/microsoft/playwright-mcp) | Explicit per-tab grants and value-level privacy boundaries. |
| Observe, act, and extract browser workflows | [Stagehand](https://www.stagehand.dev/) | Deterministic policy, stale rejection, and value-free audit evidence. |
| Human takeover and confirmation | [OpenAI agent mode](https://help.openai.com/en/articles/11752874-agent) | Continuous field-level sharing rules, not only takeover at a sensitive step. |
| Human validation of document extraction | [UiPath Human in the Loop](https://docs.uipath.com/coding-agents/standalone/latest/user-guide/human-in-the-loop) | A lighter browser-native workflow with the same fact/action/replay boundary. |
| Website-declared structured agent tools | [WebMCP](https://developer.chrome.com/docs/ai/webmcp) | Consume declared tools as untrusted facts, then add Saccade policy, confirmation, receipts, and legacy-page fallback. |

The defensible advantage is the combination and the measured behavior, not any
single primitive.

## Competitive Position Versus Playwright MCP

Saccade competes with Playwright MCP as an Agent browser interface, not with
Playwright Test as a cross-browser test framework. The product target is one
excellent Chromium-compatible browser where a human and an Agent share a
visible session. Multi-engine coverage, mobile emulation, locator generation,
network mocking, arbitrary JavaScript execution, and CI trace/video breadth do
not count as product advantages for this target.

Evaluate capability breadth in two groups:

1. **Agent task primitives:** reading, navigation, tabs, forms, uploads,
   downloads, dialogs, screenshots, rich controls, Canvas/WebGL, and verified
   outcomes. Saccade must implement or explicitly route every primitive needed
   by the published task corpus.
2. **Test-framework authority:** cookie/storage mutation, network interception,
   arbitrary evaluation, generated locators, multi-engine matrices, and test
   runner artifacts. These are not goals for the default Agent contract. Adding
   them without a task requirement would increase tool-selection burden,
   context cost, authority, and attack surface.

Current scoped verdict:

- Saccade has the stronger native contract for Human-controlled per-tab access,
  model-invisible protected values, revision-bound input, verified outcomes,
  and value-free receipts/replay.
- The matched AI-044 public-page open/read wedge now proves lower warm wall time
  and lower model-facing token use than the official optimized Playwright MCP
  configuration on that one task: 162.755 vs 654.004 ms warm p50, 132 vs 224
  median task-result tokens, and 2,120 vs 4,242 first-task tokens including all
  advertised tool schemas.
- A separate Playwright screenshot observation charged 920 GPT-5.6 image tokens
  plus 158 non-image result-metadata tokens. Playwright does not require a
  screenshot for every task, so this cost is not included in the primary
  structured comparison.
- Signed Build 57 adds a measured resize/Canvas wedge. Browser-pushed layout
  epochs, just-in-time action refresh, local semantic rebase and a verified
  native-input receipt completed the packaged DOM and stable Canvas-surface
  cases in 5.551 ms and 2.717 ms after invalidation, with no screenshot or
  additional LLM turn. A target removed at the responsive breakpoint was
  rejected before native input.
- This is not a claim that Playwright DOM locators are stale: official
  Playwright locators resolve an up-to-date DOM element for every action. The
  contrast is Playwright MCP's vision lane, where coordinate mouse tools are
  explicitly screenshot-driven. After layout changes, an old pixel coordinate
  has no semantic rebase or outcome receipt and must be observed again by the
  host/model to remain reliable.
- The current NaNMesh Playwright record is directional, not statistically
  conclusive: two structured reports contain one failure and one partial result
  and are marked `insufficient_evidence`.
- Therefore Saccade currently leads on Agent-browser architecture and scoped
  safety/transaction evidence and has one published efficiency win. Overall
  task-success, universal speed, and universal token wins remain claims to
  unlock with the broader `I-106` corpus.

Safe positioning now:

> Saccade is an AI-first alternative to Playwright for user-visible,
> privacy-bounded browser collaboration.

Do not score Saccade down for deliberately excluding unrelated test-runner
breadth. Do score every missing primitive that blocks a user task.

## P0: Must Finish Before A Public Product Claim

| ID | Improvement | Acceptance gate | Evidence to publish |
| --- | --- | --- | --- |
| I-001 | Real FORMMAX workflow matrix | Complete at least two real user-granted drafts before alpha and ten distinct workflows before a broad utility claim. Preserve user/protected values and never submit. | Per-run success, time, tokens, corrections, interventions, rejected fields, leaks, route, and replay. |
| I-002 | Agent safety benchmark | Add AgentDojo plus shadow DOM, custom-control, semantic/visual evasion, stale-frame, and prompt-injection cases. | Utility, attack success, false-block, protected-value leak, and unauthorized-side-effect rates. |
| I-003 | DOCMAX product path | Inspect, fill, and verify a local AcroForm and one public blank AcroForm. Classify flat, scanned, and XFA PDFs without pretending they are fillable. | Output PDF, redacted field diff, value-free replay, protected-field handoff, and classification reason. |
| I-004 | Automatic compatibility preflight | Detect a measured Servo blocker before requesting task values, route to compatibility mode, and retain the same grant/policy/receipt contract. | Route reason, engine indicator, time to ready, and no hidden fallback. |
| I-005 | Human-facing copilot workflow | Make grant, agent-active status, protected fields, draft completion, pause/revoke, recovery, and final user action clear without reading developer logs. | Five usability sessions with observed errors and corrected UX. |
| I-006 | Release packaging | Decide license; produce signed/notarized macOS artifact, checksums, SBOM/license inventory, clean install instructions, and reproducible version metadata. | Clean-machine install and uninstall report; signed artifact inventory. |
| I-007 | Independent privacy audit | Search MCP output, reports, replay, errors, crash logs, and screenshots for protected sentinels. | Zero protected-value leaks is a hard release gate. |
| I-008 | Stable failure and recovery | Tab close, browser crash, navigation, expired session, partial fill, postcondition mismatch, and auth expiry must block or recover with an explanation. | Every tested failure ends in verified, repaired, or explicitly blocked state. |
| I-009 | Human/agent agreement gate | At fixed viewport and revision, correlate the visible screenshot with truth inventory, action rectangles, hit-test targets, and post-action state. Reject or route pages with missing visible controls, hidden/duplicate facts, unsafe geometry, or visual layers the engine did not render. | Publish per-page disagreement classes and overlays. GitHub Dashboard, account menu, and New Issue are the first complex canaries. |

## Human/Agent Agreement Checklist

Run this gate before treating a complex page as actionable. Screenshots are
optional evidence for ordinary green pages and required when facts, geometry,
or observed behavior disagree. Visual evidence may reach the agent only before
task/protected values are present or after protected regions are reliably
redacted; otherwise it remains user-only evidence.

### Same Observation Base

- [ ] Screenshot, facts, actions, and hit-test use the same tab, revision,
  viewport, scroll position, device-pixel ratio, and engine route.
- [ ] Page readiness is explicit; loading, hydration, and mutation activity are
  not mistaken for a stable page.
- [ ] The screenshot is nonblank and includes the visual layers relevant to the
  proposed action.

### Truth Inventory

- [ ] Every sampled visible interactive control has one corresponding fact.
- [ ] Hidden, zero-rect, backing, disabled, and offscreen controls do not appear
  as ordinary actionable controls.
- [ ] One logical control is not emitted as several competing actions unless the
  relationship is explicit.
- [ ] Labels, roles, states, ownership, and supported operations match what a
  user can perceive and do.
- [ ] Protected controls expose classification and handoff status without value.

### Geometry And Hit-Test

- [ ] CSS rectangles are valid for the current viewport and scroll position.
- [ ] Screenshot pixels and CSS coordinates use a recorded, verified scale.
- [ ] The proposed click point lies in the painted control and native hit-test
  resolves to that control or a declared descendant.
- [ ] Menus, dialogs, popovers, sticky layers, and z-order occlusion are reflected
  in the action map.

### Freshness And Outcome

- [ ] Relevant DOM, layout, overlay, and navigation changes create a new
  revision or invalidate affected facts.
- [ ] An action prepared against an old revision is rejected.
- [ ] The resulting state is verified from a fresh observation, not inferred
  from a successful input dispatch.
- [ ] A disagreement produces a typed reason and block, repair, or compatibility
  route before task values are collected.

### Required Metrics

| Metric | Definition |
| --- | --- |
| Visible-control recall | Sampled user-visible controls represented by correct facts. |
| Actionable precision | Exported ordinary actions that are actually visible, enabled, stable, and targetable. |
| Duplicate contamination | Extra facts/actions representing the same logical control. |
| Hidden contamination | Hidden, zero-rect, backing, or offscreen controls exported as actionable. |
| Geometry error | Difference between fact rectangles, painted regions, and reference geometry at the same viewport. |
| Hit-test accuracy | Proposed action points resolving to the intended control. |
| Revision freshness | Relevant mutations that invalidate or refresh affected facts before action. |
| Postcondition accuracy | Attempted actions whose reported result matches a fresh observed state. |
| Protected-value leaks | Protected values found outside the user-visible page. Release target: zero. |

## P1: Reliability, Coverage, And Competitive Proof

| ID | Improvement | Acceptance gate |
| --- | --- | --- |
| I-101 | Complex control coverage | Measure shadow DOM, iframes, contenteditable, ARIA widgets, virtualized rows, dates, dropdowns, file inputs, uploads, dialogs, and downloads. Do not broaden claims until each class has evidence. |
| I-102 | Browser rendering and interaction parity | Maintain GitHub as a complex canary; continue viewport, grid, textarea, dropdown, focus/caret, overlay, and Canvas/WebGL reductions. Classify each failure as rendering, fact extraction, geometry, hit-test, application lifecycle, or site blocking before choosing a fix or route. |
| I-103 | Authentication and profile UX | Persist normal profiles like a browser, isolate private profiles, recover expired login, and keep cookies/storage hidden from the agent. Test Google login and common providers without claiming universal support. |
| I-104 | Save, resume, and repair | Resume a partially completed workflow after navigation or restart without replaying successful writes or overwriting user edits. |
| I-105 | WebMCP adapter | Discover native WebMCP tools, map read-only and untrusted-content hints, bind calls to origin/revision, and pass them through Saccade confirmation and receipt policy. Fall back to inferred legacy-page facts. |
| I-106 | Fair Agent-browser A/B benchmark | Run identical user tasks through Saccade, Playwright MCP, Stagehand or equivalent, and screenshot-driven browser control with the same model, Chromium environment, starting state, task instruction, and success criteria. Report success, time, model tokens, actions, retries, incorrect actions, corrections, interventions, verification quality, and leaks. Do not count multi-engine or test-runner-only breadth. |
| I-107 | Cross-platform distribution | Define support and packaging for macOS first; add Windows/Linux only with clean install and runtime evidence. |
| I-108 | Accessibility and keyboard quality | Verify focus, caret, selection, zoom, keyboard navigation, screen-reader labels, and visible status for browser chrome and copilot controls. |
| I-109 | External host validation | Have an independent TypeScript host and Python host complete grant, facts, safe fill, stale rejection, and replay from the published contract. |
| I-110 | Compatibility matrix | Test a fixed set of public, authenticated, government, forum, commerce, document, and Canvas/WebGL sites. Record facts, not color labels based on assumptions. |

## P2: Expansion After The Core Wedge Works

| ID | Improvement | Guardrail |
| --- | --- | --- |
| I-201 | YouTube/video learning packets | Use transcripts, timestamps, metadata, and selected frames. Do not present automatic captions or inferred UI operations as strong evidence. |
| I-202 | Broader game and WebGL support | Continue only when it improves a measured customer workflow or the reflex benchmark. Do not build a second renderer inside Saccade. |
| I-203 | Social and forum draft matrix | Test drafting on Hacker News, Reddit, LinkedIn, forums, and issue trackers. Keep final publication user-controlled unless product policy explicitly changes. |
| I-204 | Public demo and long-form report | Show the same real workflow in the user view, fact view, action receipt, and replay. Include failures and compatibility routing. |
| I-205 | Vendor integration package | Provide local MCP, computer-use companion, and embedded-runtime paths only after the same safety and evidence gates pass. |
| I-206 | Remote deployment | Add standard authentication, tenant isolation, revocation, audit retention, and threat modeling before any hosted control surface. |

## Release Candidate Gates

### Safety

- [ ] Protected-value leaks are zero across MCP, reports, replay, errors, crash
  logs, and default screenshots.
- [ ] Unauthorized side effects are zero in the release corpus.
- [ ] Stale actions are rejected after navigation and relevant page mutation.
- [ ] Page-controlled text cannot create trusted confirmation or policy state.
- [ ] The user can pause, revoke, and close access without restarting the host.

### Utility

- [ ] Ten real workflows have user-reviewed outcomes and redacted evidence.
- [ ] Eligible ordinary-field verification reaches the declared target on the
  published corpus; failures are listed by control class and site.
- [ ] Partial failures preserve completed work and produce a repair or block
  reason.
- [ ] Model token, wall-time, action-count, correction, and intervention data are
  captured for every comparison run.
- [ ] DOCMAX passes one local and one public blank AcroForm without protected
  values in evidence.

### Product

- [ ] The visible engine/mode and agent-active state are understandable.
- [ ] Grant, protected-field handoff, completion review, and final user action
  work without command-line interpretation.
- [ ] Profile persistence, private mode, login expiry, tab close, and crash
  recovery have explicit behavior.
- [ ] Compatibility routing happens before collecting task values and never
  masquerades as the default engine.
- [ ] On the published complex-page corpus, the screenshot, truth inventory,
  action geometry, and hit-test result agree or the page is explicitly routed.
- [ ] Browser chrome supports reliable URL editing, focus/caret, navigation,
  resizing, and keyboard use.

### Distribution

- [ ] License and third-party obligations are decided and documented.
- [ ] macOS artifact is signed and notarized.
- [ ] SBOM, license inventory, checksums, version, source commit, and build
  instructions ship with the artifact.
- [ ] A clean machine can install, run the integration selftest, and uninstall.
- [ ] Public compatibility and limitation pages match the tested release.

## Claim Library

### Safe Now, With Scope

- "Saccade exposes redacted browser facts and verified actions from a
  user-granted visible tab."
- "The current generic-form gate filled 672 ordinary fixture fields while three
  protected fields remained blocked."
- "Two public automation test forms completed at 5/5 verified fields with no
  submit."
- "Current safety gates reject missing or cross-tab capability, stale action
  bases, and page-authored confirmation attempts."
- "The recorded local reflex gate measured `0.091 ms` p95 command dispatch. This
  is dispatch latency in the scoped local game, not model or internet latency."
- "Saccade records value-free receipts and replay for its current action paths."
- "Saccade supports optional screenshot capture and correlates selected visual
  evidence with CSS-space browser facts in its current scoped gates."
- "In the published macOS `example.com` open-and-read benchmark, Saccade used
  41% fewer per-task model-facing tokens and 75% less warm wall time than the
  optimized official Playwright MCP configuration. Including all advertised
  tool schemas, Saccade used 50% fewer first-task context tokens."
- "In the benchmark's separate visual lane, a Playwright 1280x720 screenshot
  added 920 GPT-5.6 image tokens plus 158 result-metadata tokens; Saccade's
  structured page read required no screenshot."
- "In signed Build 57's native resize gates and live SimpleMMO rerun, Saccade
  detected browser layout changes, locally rebased the same surviving semantic
  action and verified its native-input receipt without another screenshot or
  LLM turn; actions removed by the new layout were rejected before input. This
  is a coordinate/Canvas-surface workflow result, not a claim against
  Playwright's up-to-date DOM locators or arbitrary Canvas game semantics."

### Requires More Evidence

- "Saccade is universally faster than Playwright, Stagehand, or screenshot
  automation."
- "Saccade universally uses fewer model tokens."
- "Saccade safely fills real forms better than existing agents."
- "Saccade can fill PDFs end to end."
- "Saccade prevents prompt injection."
- "Saccade works reliably across authenticated sites."

These broad statements require the P0/P1 comparisons above. The narrower
AI-044 wording is allowed only with its measured corpus and metric definition.

## Playwright Head-To-Head Exit Gate

Publicly use “Saccade beats Playwright MCP for Agent browsing” only when all of
the following are true:

- `I-106` publishes a matched corpus and Saccade wins the named metrics. State
  each win by metric and tested scope; never imply universal superiority.
- The installed dogfood basics close `DF-001`, `DF-003`, `DF-005`, `DF-006`,
  `DF-008`, `DF-009`, `DF-010`, and `DF-011`, followed by one clean pass of
  `DF-R01` through `DF-R09` in `docs/dogfood_punch_list_20260716.md`.
- `I-001`, `I-008`, `I-101`, `I-102`, `I-103`, and `I-110` establish the task,
  recovery, control, authenticated-session, and compatibility scope being
  compared.
- `I-002`, `I-007`, and `I-009` pass with zero protected-value leaks and zero
  unauthorized browser access in the release corpus.
- `I-005` and `I-006` prove that the result works as an installed product, not
  only from a repository checkout.

After this gate, the preferred scoped claim is:

> On the published Chromium Agent-browser corpus, Saccade completed more tasks
> with fewer model tokens and lower wall time than Playwright MCP while keeping
> protected values outside model context and evidence.

Replace “more”, “fewer”, and “lower” with the measured results. If one metric
does not win, omit it rather than blending the score into an unsupported
“overall” victory.

### Do Not Claim

- "Works on every website."
- "A drop-in replacement for Chrome."
- "The only browser that gives agents web truth."
- "The agent can never see sensitive information."
- "Complete Chrome rendering or WebGL parity."
- "All logins persist."
- "Public, signed, production-ready release" before the distribution gates pass.

## Per-Run Measurement Record

Every publishable workflow run should record:

| Category | Fields |
| --- | --- |
| Identity | release version, source commit, date, machine, route/engine |
| Task | site category, URL or redacted origin, requested outcome, side-effect class |
| Access | visible tab, grant time, capability scope, profile/private mode |
| Readiness | navigation time, ready time, preflight result, compatibility reason |
| Facts | revision, control counts, protected/human-owned/preserved/unsupported counts |
| Actions | planned, attempted, verified, repaired, rejected, stale, failed |
| Human work | corrections, interventions, protected fields, final action |
| Efficiency | wall time, model tokens, bridge bytes, action count, retries |
| Safety | protected leaks, unauthorized side effects, prompt-injection result |
| Evidence | report, replay, optional screenshot, final verdict, known limitation |

## Recommended Build Order

1. Build the human/agent agreement gate and run it on GitHub plus controlled
   render, truth, and hit-test reductions.
2. Finish AI-033 security measurement and independent leak audit.
3. Run AI-031 on two real drafts, then expand to the ten-workflow matrix.
4. Spike the WebMCP adapter while the form model is still being stabilized.
5. Complete AI-032 AcroForm inspect, fill, verify, and classify.
6. Productize compatibility preflight and the human copilot workflow.
7. Run the competitive A/B benchmark and five usability sessions.
8. Finish signing, SBOM, clean-machine install, public matrix, and release copy.

## Maintenance Rules

- Update this scorecard when evidence changes, not when a demo merely looks
  promising.
- Link every `Proven` row to a stable report or gate.
- Keep site and engine scope in every claim.
- Record negative results and routes in `docs/browser_compat_ledger.md`.
- Keep execution ordering in `docs/CURRENT_ACTION_ITEMS.md`.
- Keep distribution truth in `docs/release_inventory.md`.
- Keep security details in `docs/ai033_agent_safety.md`.
- Keep generic-form evidence in `docs/ai031_generic_form_plan.md`.
- Review the claim library before release notes, demos, vendor decks, or posts.
