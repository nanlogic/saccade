# Saccade Next Plan v5 — From MOUSEMAX Proof To AI-First Playwright Alternative

Date: 2026-06-11  
Owner: Wayne / NaN Logic  
Status: post-M7/M8/M9/M10/M11 planning  
North star: **Saccade is an AI-first Playwright alternative: browser truth → verified actions.**

---

## 0. Strategic Reframe

MOUSEMAX proved the trust layer. It showed that a browser can use rendered truth, pixel readback, calibrated input, and replay verification to complete a dynamic visual task without LLM calls in the hot loop.

That is not the product.

The product is:

> **Saccade turns browser truth into verified actions for AI agents.**

The next useful beachhead is not public website browsing. It is:

> **DEVMAX: agent self-testing for websites the agent just wrote.**

This is the safest and most immediately useful Saccade wedge:
- localhost / owned code,
- no ToS risk,
- no anti-bot,
- no login wall by default,
- no spam risk,
- high developer pain,
- easy dogfooding with Claude Code / Codex / Cursor / Lovable-style workflows,
- easy benchmark construction,
- natural path to MCP.

The second immediate foundation is:

> **Trusted Tabs: tab-level ownership isolation and login handoff.**

This is the first concrete brick of the “human + AI concurrent browser” vision:
- Human tabs receive human input only.
- Agent tabs receive agent input only.
- Agent cannot inject into Human tabs.
- Agent cannot read Human tabs unless explicitly granted.
- User can hand off login session without exposing passwords/OTP.
- Every action is scoped by `tab_id`, `owner`, `page_revision`, and policy.

These two lines combine into the real product story:

```text
MOUSEMAX = trust proof
DEVMAX   = developer usefulness proof
FORMMAX  = practical workflow proof
Trusted Tabs = safety and human handoff foundation
```

---

## 1. Immediate Rule: Freeze MOUSEMAX As Evidence, Do Not Keep Expanding It

MOUSEMAX is now a release artifact, not the active product line.

Do only evidence-hardening work:

### MOUSEMAX Evidence Freeze Checklist

Done when all pass:

```bash
# Existing best artifact must validate.
scripts/validate_m9_release.sh runs/real/run_1781193985

# Pure-pixel real-site gate, 5 consecutive runs.
cargo run -q -p mousemax -- run --site real --spawn-speed Epic --target-size Tiny --duration 15 --instrumentation none --replay
# Repeat 5x or add a loop script.

# Validate every run.
cargo run -q -p mousemax -- validate-run runs/real/<run_id> --require-click-map
```

Required report fields:
- `expired_unclicked`
- `targets_seen`
- `hits`
- `misses`
- `false_positive_clicks`
- `stale_clicks`
- `unknown_verifications`
- `p95 detect_to_dispatch`
- `p95 first_visible_to_dispatch`
- `input_space`
- `calibration_max_err_css_px`
- `instrumentation`
- `llm_frame_calls`

### Latency Audit

Add one small local page:

```text
test_pages/event_latency_probe.html
```

It records:
- `mousedown performance.now()`
- `mouseup performance.now()`
- `click performance.now()`
- received `clientX/clientY`

Saccade records:
- `t_move_sent_ns`
- `t_down_sent_ns`
- `t_up_sent_ns`

Output:

```json
{
  "dispatch_return_to_page_mousedown_ms": { "p50": ..., "p95": ... },
  "dispatch_return_to_page_click_ms": { "p50": ..., "p95": ... },
  "coordinate_error_css_px": { "max": ... }
}
```

Reason: public latency claims must explain exactly what `dispatch` means.

### MOUSEMAX Public Artifacts

Create:

```text
docs/reports/mousemax_m7_real_site.md
runs/real/run_1781193985/click_map.png
runs/real/run_1781193985/result.json
runs/real/run_1781193985/replay.jsonl
runs/real/run_1781193985/validator.txt
runs/real/run_1781193985/before.png
runs/real/run_1781193985/after.png
```

Do not add new mouse gameplay features until DEVMAX and Trusted Tabs are started.

---

## 2. New Priority Order

The next work order is:

```text
N1. Trusted Tabs Runtime
N2. DEVMAX Local Agent Self-Test
N3. MCP Skeleton With Tool Namespaces
N4. FORMMAX Servo Input Runner
N5. Chrome Adapter v0 For DEVMAX Compatibility
N6. Safety Policy UI and Replay Unification
N7. Public Release Package
```

Do not implement real third-party website automation until N1–N4 are green.

---

## 3. N1 — Trusted Tabs Runtime

### Goal

Build a minimal Saccade shell with multiple WebViews and strict tab ownership.

This is not “just tabs.” This is the coarse-grained trusted-input boundary:
- Human input goes only to Human tabs.
- Agent input goes only to Agent tabs.
- Agent cannot inject into Human tabs.
- Agent cannot read Human tabs unless explicitly allowed.
- User can take over an Agent tab, which changes ownership to Human.

### Data Model

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabOwner {
    Human,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadGrant {
    None,
    VisibleSummaryOnly,
    FullTruth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub tab_id: TabId,
    pub owner: TabOwner,
    pub url: String,
    pub title: Option<String>,
    pub read_grant: ReadGrant,
    pub page_revision: u64,
    pub visual_marker: TabVisualMarker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabVisualMarker {
    pub border: bool,
    pub badge: String,      // "HUMAN" or "AGENT"
    pub color_name: String, // not security-critical; for user clarity only
}
```

### Hard Security Rules

```text
1. Agent InputPort refuses input to owner=Human tabs.
2. Human physical input is routed only to the focused tab.
3. If user focuses or types in an Agent tab, that tab is paused or converted to owner=Human.
4. Agent cannot call truth() on owner=Human tabs unless read_grant != None.
5. All actions require tab_id.
6. All actions require basis_page_revision.
7. Replay logs every event with tab_id + owner + actor.
8. Login credentials, password fields, OTP, passkeys, and payment fields are never exposed to agent truth.
9. Forum posting, social posting, email sending, destructive actions, purchases, and legal attestations require explicit human confirmation.
10. Multi-account or bulk posting tools are rejected at policy level.
```

### Must Verify

Do not assume these. Verify on the pinned Servo version:

```text
1. Multiple WebViews can exist under one Servo instance.
2. Cookie/session behavior across WebViews:
   - same process, same origin, WebView A sets cookie, WebView B reads login state.
3. localStorage/sessionStorage behavior across WebViews.
4. Private/incognito WebView option existence or absence.
5. Cookie persistence across process restart:
   - if unavailable, v1 login handoff is process-lifetime only.
6. Focus routing:
   - only active Human tab receives physical input.
7. Agent input routing:
   - notify_input_event goes only to target Agent tab.
8. Screenshots/readback are scoped to the target WebView/tab.
```

### Local Login Fixture

Create:

```text
test_pages/login_handoff/
  index.html
  login.html
  dashboard.html
  server.js or tiny_http route
```

Behavior:
- `/login` has username/password.
- successful login sets a session cookie.
- `/dashboard` shows `LOGGED_IN user=<name>` only if cookie exists.
- no real auth; local only.

### Done When

```bash
cargo run -q -p saccade-shell -- selftest-tabs
```

Must print:

```text
TABS PASS webviews=2 cookie_shared=<true|false> storage_shared=<true|false> input_isolated=true read_policy_enforced=true
```

If cookie sharing is false, do not fake it. Record:

```text
docs/decisions.md:
  "Pinned Servo WebViews do not share cookies. Login handoff v1 requires same WebView ownership transfer or Chrome adapter."
```

---

## 4. N2 — Login Handoff Protocol

### Goal

Let a human log in safely, then let an Agent tab continue with the session without receiving credentials.

### API

```json
{
  "tool": "tabs.request_user_login",
  "input": {
    "url": "http://127.0.0.1:<port>/login",
    "origin": "http://127.0.0.1:<port>",
    "reason": "Agent needs a logged-in session for local dev test."
  }
}
```

### Flow

```text
1. Agent calls request_user_login(url).
2. Shell opens Human tab with visible HUMAN badge.
3. User logs in manually.
4. User clicks “Done” in Saccade chrome.
5. Shell verifies login state using safe truth signal:
   - URL change,
   - visible logged-in marker,
   - local fixture dashboard text,
   - never password value.
6. Shell opens Agent tab at same origin.
7. Agent tab has AGENT badge and inherited session if engine supports it.
8. Agent starts work.
```

### Done When

```bash
cargo run -q -p saccade-shell -- selftest-login-handoff
```

Must print:

```text
LOGIN_HANDOFF PASS human_login=true agent_session=true password_exposed=false otp_exposed=false agent_input_to_human_tab_blocked=true
```

---

## 5. N3 — DEVMAX: Local Agent Self-Test

### Product Thesis

DEVMAX is the first “wow, I can use this” demo.

Use case:
- Agent writes a webpage.
- Agent launches Saccade against localhost.
- Saccade returns a structured report:
  - rendered truth,
  - action map,
  - console errors,
  - network errors,
  - blank page detection,
  - overlap / clipping / invisible text,
  - broken interactions,
  - form smoke results,
  - replay artifacts.
- Agent fixes code.
- Repeat.

This is much more useful than mouseaccuracy for daily work.

### DEVMAX Fixture Set

Create:

```text
test_pages/devmax/
  blank_page/
  console_error/
  hydration_error/
  missing_asset/
  invisible_text/
  overlapping_elements/
  offscreen_button/
  button_no_handler/
  broken_form_validation/
  lazy_route_error/
  scroll_container_hidden_submit/
  responsive_mobile_break/
  modal_blocks_page/
  canvas_chart_blank/
  css_zindex_overlay_bug/
  wrong_success_state/
```

Each fixture has:
- an intentional bug,
- expected detector finding,
- optional fix hint,
- deterministic output.

### Truth Report

```json
{
  "page_revision": 42,
  "url": "http://localhost:5173",
  "title": "Local App",
  "summary": "Page rendered but primary CTA is covered by an overlay.",
  "visual_health": {
    "blank_page": false,
    "large_empty_regions": [],
    "invisible_text": [
      { "text": "Submit", "reason": "low contrast or same-color text/background" }
    ],
    "overlaps": [
      { "front": "modal_overlay", "back": "submit_button", "severity": "blocking" }
    ],
    "offscreen_interactive": []
  },
  "runtime_health": {
    "console_errors": [],
    "network_errors": [],
    "uncaught_exceptions": []
  },
  "actions": [
    {
      "action_id": "act_submit",
      "label": "Submit",
      "kind": "click",
      "enabled": true,
      "blocked_by": "modal_overlay"
    }
  ],
  "recommendations": [
    {
      "kind": "fix",
      "message": "Submit button is visually present but does not receive events because modal_overlay covers it."
    }
  ]
}
```

### DEVMAX CLI

```bash
cargo run -q -p devmax -- audit --url http://127.0.0.1:5173 --replay
cargo run -q -p devmax -- selftest-fixtures
```

### Done When

```bash
cargo run -q -p devmax -- selftest-fixtures
```

Must print:

```text
DEVMAX FIXTURES PASS total=16 detected>=14 false_positives<=1
```

Also run one live local app:

```bash
cargo run -q -p devmax -- audit --url http://127.0.0.1:5173 --replay
```

Must output:

```text
DEVMAX AUDIT PASS report=... replay=... findings=<n>
```

---

## 6. N4 — MCP Skeleton

### Goal

Expose Saccade as an agent tool, with separate namespaces for developer self-test and web work.

### Tool Namespaces

```text
saccade.dev.*
  Safe by default. Localhost / owned dev servers.
  Tools for agent self-testing.

saccade.web.*
  Higher risk. Real websites, login handoff, form work.
  Requires Trusted Tabs and policy gates.

saccade.tabs.*
  Tab ownership and login handoff.

saccade.report.*
  Replay, artifacts, validation.
```

### Required MCP Tools

```text
saccade.dev.open_local(url)
saccade.dev.audit_page(tab_id)
saccade.dev.click_all_primary_actions(tab_id, policy)
saccade.dev.fill_smoke_form(tab_id, policy)
saccade.dev.get_report(run_id)

saccade.tabs.list()
saccade.tabs.open(url, owner)
saccade.tabs.request_user_login(url, reason)
saccade.tabs.takeover(tab_id)
saccade.tabs.pause_agent(tab_id)
saccade.tabs.close(tab_id)

saccade.web.truth(tab_id)
saccade.web.actions(tab_id)
saccade.web.act(tab_id, action_id, basis_page_revision)
saccade.web.fill_form(tab_id, form_id, values, policy)

saccade.report.validate_run(run_dir)
saccade.report.replay_summary(run_dir)
```

### Tool Return Rules

All tools return compact JSON and artifact paths, not giant DOM dumps.

Default token policy:
- summaries first,
- detailed report by explicit request,
- full screenshot only as artifact path,
- replay as artifact path.

### Done When

```bash
cargo run -q -p saccade-mcp -- selftest
```

Must print:

```text
MCP PASS tools_registered>=12 tab_scoping=true local_dev_audit=true policy_gate=true
```

---

## 7. N5 — FORMMAX Servo Input Runner

### Goal

Turn the existing FORMMAX fixture from a smoke test into a through-browser product demo.

Existing fixture:
- 96 deterministic capacity rows,
- two-page flow,
- lazy scroll rendering,
- text, number, date, select, checkbox,
- sensitive fields: tax ID, signature, legal attestation,
- receipt JSON.

### Required Events

Replay must include:

```text
field_discovered
field_focused
field_filled
field_verified
field_blocked_sensitive
scroll_checkpoint
page_next_clicked
validation_seen
confirmation_required
receipt_seen
form_transaction_finished
```

### Fill Policy

```json
{
  "block_sensitive": true,
  "submit": "allow_local_fixture_only",
  "echo_values": false,
  "human_confirm_required_for": [
    "tax_id",
    "signature",
    "legal_attestation",
    "payment",
    "password",
    "otp",
    "file_upload"
  ]
}
```

### Done When

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

Must print:

```text
FORMMAX RUNNER PASS rows=96 pages=2 filled=<non_sensitive_count> blocked_sensitive=3 receipt_verified=true replay=...
```

---

## 8. N6 — Safety Truth v1

### Product Principle

Saccade’s public differentiation is not only speed. It is **trusted action**.

If Saccade becomes “fast browser bot,” it loses.
If Saccade becomes “browser truth + verified actions + human control,” it wins.

### Policy Enum

```rust
pub enum RiskKind {
    Normal,
    Pii,
    Password,
    Otp,
    PaymentCard,
    MoneyMovement,
    FileUpload,
    LegalAttestation,
    Signature,
    DestructiveAction,
    OAuthAuthorization,
    PublicPost,
    PrivateMessage,
    EmailSend,
    Download,
    UnknownHighRisk,
}
```

### Default Decisions

```text
Normal:
  allow if actionability passes.

Pii:
  allow fill from local vault/profile; do not echo value.

Password / OTP / passkey:
  human-only.

Payment / money movement:
  human-only or explicit confirmation.

File upload:
  human confirmation.

Legal attestation / signature:
  human confirmation.

Public post / forum reply / social post:
  draft-only by default; explicit user confirmation required to submit.
  rate limited.
  no bulk posting.
  no multi-account automation.

Destructive action:
  explicit confirmation.

UnknownHighRisk:
  require confirmation.
```

### Safety UI

A minimal confirmation bar near the tab strip:

```text
Agent wants to:
  Fill legal attestation checkbox
Risk:
  legal_attestation
Options:
  Allow once | Human do it | Deny
```

### Done When

```bash
cargo run -q -p saccade-shell -- selftest-safety
```

Must print:

```text
SAFETY PASS password_human_only=true otp_human_only=true forum_submit_confirmation=true agent_input_to_human_tab_blocked=true legal_attestation_blocked=true replay_policy_events=true
```

---

## 9. N7 — Chrome Adapter v0

### Why Chrome Adapter Moves Earlier

Servo proved the reflex layer, but developers want to test what users see in Chrome. DEVMAX needs a Chrome path earlier than originally planned.

Do not replace Servo. Add an engine adapter.

```text
saccade_core:
  engine-neutral truth/action/replay types

saccade_browser_servo:
  Servo implementation

saccade_browser_chrome:
  Chrome implementation

saccade_browser_playwright:
  fastest v0 launcher bridge if useful
```

### v0 Strategy

Fastest path:
- Use Playwright only as a launcher / Chrome controller.
- Replace Playwright semantics with Saccade truth/action/report.
- Do not expose Playwright-style locator scripts as the primary interface.

Chrome adapter v0 gathers:
- screenshot / rendered pixels,
- accessibility tree,
- DOM/layout bounds,
- computed styles where needed,
- console errors,
- network errors,
- elementFromPoint hit checks,
- form fields,
- action map.

Chrome adapter v0 does not need to beat MOUSEMAX latency. It needs DEVMAX and FORMMAX compatibility.

### Done When

```bash
cargo run -q -p devmax -- audit --engine chrome --url http://127.0.0.1:5173 --replay
```

Must print:

```text
DEVMAX CHROME PASS report=... console_errors=<n> visual_findings=<n> actions=<n>
```

---

## 10. Playwright Alternative Benchmark

### Goal

Prove Saccade is not just faster at clicking. Prove it reduces agent loops.

Run the same DEVMAX and FORMMAX fixtures with:

```text
A. Playwright MCP / Playwright script baseline
B. screenshot + VLM loop baseline, if available
C. Saccade truth/action transaction
```

Compare:
- wall time,
- actions count,
- LLM calls,
- tokens,
- failed detections,
- false positives,
- verification quality,
- replay clarity.

### Done When

```bash
cargo run -q -p devmax -- compare --fixture-set test_pages/devmax --baseline playwright --replay
cargo run -q -p formmax -- compare --baseline playwright --replay
```

Output:

```text
COMPARISON PASS devmax_saccade_findings>=baseline_findings formmax_actions_reduced=true token_estimate_reduced=true
```

---

## 11. Public Release Shape

Do not release as “AI beats mouse game.”

Release as:

```text
Saccade — an AI-first alternative to Playwright
Browser truth → verified actions.
```

Public demos:
1. MOUSEMAX: trust proof.
2. DEVMAX: developer self-test usefulness proof.
3. FORMMAX: form transaction usefulness proof.

README first screen:

```markdown
# Saccade

An AI-first alternative to Playwright.

Playwright is great for human-written tests. Saccade is built for AI agents:
it turns browser-rendered truth into action maps, fill transactions, safety policy,
and verified results, so agents do not have to guess from stale screenshots or noisy DOM snapshots.

## Demos

- MOUSEMAX: dynamic visual target proof.
- DEVMAX: agent self-tests local web apps.
- FORMMAX: long-form transactions with sensitive-field gating.
```

Public claims must include caveats:
- MOUSEMAX result currently proven on tested macOS setup unless Linux/X11 also passes.
- Servo compatibility is not Chrome compatibility.
- Chrome adapter is for compatibility, not low-latency proof.
- No CAPTCHA bypass.
- No anti-detection work.
- No bulk posting / spam automation.
- Sensitive actions require human confirmation.

---

## 12. Codex Execution Prompt

Use this as the next Codex instruction:

```text
Read SACCADE_BUILD_SPEC.md and this NEXT_PLAN.md.

We are past MOUSEMAX M7/M9. Do not keep expanding MOUSEMAX except evidence hardening.

Current priority:
1. Implement Trusted Tabs Runtime as an independent feature/bin `saccade-shell`.
2. Then implement DEVMAX fixture selftest.
3. Then implement MCP skeleton.
4. Then implement FORMMAX Servo input runner.

Rules:
- One milestone per session.
- No Servo API guessing. Read local pinned docs before calling Servo APIs.
- Do not touch existing mousemax benchmark hot path unless a test requires it.
- All tools/actions must be tab_id scoped.
- Agent input must be refused for Human-owned tabs.
- Agent read access to Human tabs is denied unless explicitly granted.
- Sensitive fields/actions require policy gates.
- End every milestone with Done When command output and a short report.

Start with N1 only:
- Multi-WebView tab shell.
- Verify cookie/session sharing behavior on pinned Servo.
- Implement owner model Human|Agent.
- Enforce input isolation.
- Add local login_handoff fixture.
- Produce docs/tabs_runtime_profile.md.
- Done when `cargo run -q -p saccade-shell -- selftest-tabs` passes.
```

---

## 13. Decision Summary

The two discussion results are mostly right, with one adjustment:

- DEVMAX should be the first product beachhead.
- Trusted Tabs should not wait; it is the safety foundation and login handoff foundation.
- FORMMAX remains important, but after tab safety and DEVMAX skeleton.
- PDF stays a side path.
- Chrome adapter moves earlier, but only after DEVMAX proves the local workflow shape.
- MOUSEMAX is now trust evidence, not the main product workstream.

Final next milestone:

```text
N1: Trusted Tabs Runtime
```

Second:

```text
N2: DEVMAX Local Agent Self-Test
```

Third:

```text
N3: MCP Skeleton
```

Fourth:

```text
N4: FORMMAX Servo Input Runner
```
