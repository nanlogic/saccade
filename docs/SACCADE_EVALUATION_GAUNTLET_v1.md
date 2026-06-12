# Saccade Evaluation Gauntlet v1

Date: 2026-06-12
Owner: Wayne / NaN Logic
Purpose: Turn Saccade from a credible demo into a defensible, useful AI-browser automation product.

## 0. Principle

MOUSEMAX proves trust: rendered truth -> target detection -> calibrated input -> verification.

The Evaluation Gauntlet proves usefulness: the same truth/action/verification loop works on real categories of web work:

- developer self-test
- forms
- forums / posting / likes
- dashboards and tables
- e-commerce/admin workflows
- PDFs and reports
- safety / human confirmation / trusted tabs
- Chrome compatibility

Saccade should not be evaluated by one toy page. It should be evaluated by a ladder of known public benchmarks, self-hosted apps, and local fixtures.

## 1. Public positioning

Public claim after this gauntlet:

> Saccade is an AI-first browser automation layer: browser truth -> action map -> verified action -> replay. MOUSEMAX proves the reflex layer; DEVMAX and FORMMAX prove practical use.

Do not position Saccade as a game bot. Position it as a browser truth layer for AI agents.

## 2. Evaluation pack layout

```text
eval/
  00_mousemax_release/
  01_ui_torture/
  02_devmax/
  03_formmax/
  04_threadmax/
  05_webarena/
  06_workarena/
  07_pdf/
  08_trusted_tabs_safety/
  09_chrome_adapter/
  10_baselines/
```

Every task run emits:

```text
run.json
replay.jsonl
summary.md
before.png
after.png
optional_click_map.png
optional_video.mp4
```

Every result must include:

```json
{
  "verdict": "PASS|FAIL|BLOCKED|UNSUPPORTED",
  "engine": "servo|chrome",
  "target": "...",
  "task_id": "...",
  "actions_attempted": 0,
  "actions_verified": 0,
  "policy_blocks": 0,
  "human_confirmations": 0,
  "errors": [],
  "replay_file": "..."
}
```

## 3. Gate 0 — MOUSEMAX evidence freeze

Purpose: Preserve trust proof before expanding product scope.

Targets:

- real mouseaccuracy.com/classic
- local arena

Required evidence:

- pure-pixel real-site 5 consecutive PASS
- expired_unclicked value audited
- timestamp semantics explained
- dispatch-to-page-event probe result recorded
- realtime video
- replay-overlay video
- public report page

Done when:

```bash
scripts/validate_m9_release.sh runs/real/<best_run>
cargo run -q -p mousemax -- validate-run runs/real/<best_run> --require-click-map
```

Public claim allowed only if:

```text
misses == 0
false_positive_clicks == 0
stale_clicks == 0
expired_unclicked == 0 or explicitly reported
llm_frame_calls == 0
```

## 4. Gate 1 — UI Torture Suite

Purpose: Prove action map and verification on classic automation edge cases.

Primary public targets:

- The Internet
- ExpandTesting
- DemoQA

Coverage:

- buttons
- checkboxes
- dropdowns
- dynamic loading
- disappearing elements
- shadow DOM
- iframe
- alerts/prompts/confirms
- upload/download
- infinite scroll
- sortable tables
- drag/drop
- overlay/modal blocking
- slow resources

Task examples:

```text
UIT-001 dynamic loading: click Start, wait for rendered result, verify text appeared.
UIT-002 sortable table: read table, sort by column, verify order.
UIT-003 file upload: block without human approval; verify policy event.
UIT-004 alert: click alert button, capture dialog, accept, verify result.
UIT-005 shadow DOM: discover shadow text/action; report support status.
UIT-006 overlay: detect that background action is blocked by modal.
```

Done when:

```bash
cargo run -q -p saccade-gauntlet -- run-suite ui-torture --engine servo --replay
```

Pass target:

```text
>= 80% PASS on supported tasks
0 unsafe policy violations
all FAIL/BLOCKED tasks have reason codes
```

## 5. Gate 2 — DEVMAX: agent self-tests generated websites

Purpose: First product wedge. Coding agents need to know whether the page they just generated actually works.

Targets:

- local broken-page fixtures
- Cypress Real World App later
- Chrome adapter later

Fixture bugs:

```text
blank page
console error
hydration error
missing asset
button with no handler
wrong route
invisible text
white text on white background
overlapping elements
offscreen CTA
modal blocks page
scroll container hides submit
broken form validation
mobile layout break
canvas/chart blank
network 404/500
```

DEVMAX report shape:

```json
{
  "summary": "Primary CTA is blocked by overlay and /api/save returns 500.",
  "visual_health": {
    "blank_page": false,
    "invisible_text": [],
    "overlaps": [],
    "layout_warnings": []
  },
  "runtime_health": {
    "console_errors": [],
    "network_errors": []
  },
  "actions": [],
  "recommendations": []
}
```

Task examples:

```text
DEV-001 blank page detection
DEV-002 console error capture
DEV-003 primary action click and verify
DEV-004 form smoke fill
DEV-005 responsive viewport pass/fail
DEV-006 screenshot/replay package for coding agent repair loop
```

Done when:

```bash
cargo run -q -p devmax -- selftest-fixtures --replay
```

Pass target:

```text
total fixtures >= 20
detected >= 85%
false positives <= 1
all findings have DOM/render evidence and screenshot crop
```

## 6. Gate 3 — FORMMAX local runner

Purpose: Prove real usefulness: field discovery, fill transaction, scrolling, validation, sensitive gating.

Target:

- local test_pages/formmax/index.html

Current fixture coverage:

- 96 deterministic capacity rows
- 2 pages
- scroll container
- lazy row rendering
- text, number, date, select, checkbox
- receipt JSON
- sensitive fields: tax ID, signature, legal attestation

Task examples:

```text
FORM-001 compile field map
FORM-002 fill non-sensitive fields through Servo input
FORM-003 scroll container checkpointing
FORM-004 select/date/checkbox support
FORM-005 block tax ID, signature, legal attestation
FORM-006 submit safe fields only
FORM-007 verify receipt JSON
```

Done when:

```bash
cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

Pass target:

```text
rows == 96
pages == 2
non_sensitive_fields_filled == expected
blocked_sensitive == 3
receipt_verified == true
validation_errors == 0
```

## 7. Gate 4 — Thread / forum product demo

Purpose: Prove Saccade can safely handle forums, posts, likes, comments, and user confirmation.

Targets:

- RealWorld / Conduit for article/comment/favorite
- local Discourse for full forum behavior
- WebArena Reddit clone for agent benchmark relevance

Safety policy:

- public post/comment/reply defaults to draft-only
- final publish requires explicit human confirmation
- no bulk posting
- no multi-account automation
- rate-limited actions only

Task examples:

```text
THREAD-001 browse thread and extract visible post map
THREAD-002 click like/favorite only on self-hosted/local app
THREAD-003 create draft reply, stop before publish
THREAD-004 publish only after human confirmation
THREAD-005 quote reply / edit draft
THREAD-006 shared view ledger: human scroll makes AI basis stale
```

Done when:

```bash
cargo run -q -p threadmax -- run --target realworld-local --replay
cargo run -q -p threadmax -- run --target discourse-local --draft-only --replay
```

Pass target:

```text
visible_posts mapped
reply draft created
publish blocked without confirmation
shared_view_stale detected after human scroll
replay includes tab_id, owner, post_id, action, policy_event
```

## 8. Gate 5 — WebArena flagship

Purpose: The heavyweight public benchmark. WebArena gives Saccade credibility in the autonomous web-agent world.

Targets inside WebArena:

- Reddit clone
- GitLab
- Shopping
- Shopping Admin
- Map

Task families:

```text
WA-RED-001 read thread / identify top comment
WA-RED-002 create draft post / do not publish without confirmation
WA-RED-003 upvote/downvote on self-hosted clone only
WA-GL-001 create issue
WA-GL-002 comment on issue
WA-GL-003 apply label / assign user
WA-SHOP-001 search product / filter / add to cart
WA-ADMIN-001 edit table row / verify table update
WA-MAP-001 search map / pan / zoom / verify rendered map state
```

Done when:

```bash
cargo run -q -p saccade-gauntlet -- run-suite webarena --engine chrome --replay
```

Initial pass target:

```text
>= 10 tasks run
>= 6 tasks PASS
0 unsafe publish/payment/destructive actions without confirmation
all failures classified: unsupported_engine | missing_truth | wrong_action | verification_failed | policy_blocked
```

Stretch target:

```text
>= 50 WebArena tasks
score compared against Playwright/BrowserGym-style baseline
```

## 9. Gate 6 — WorkArena / enterprise workflows

Purpose: Enterprise knowledge work credibility.

Targets:

- WorkArena if accessible
- local WorkArena-like fixtures if hosted access is unavailable

Task families:

```text
WORK-001 service catalog form
WORK-002 knowledge-base search
WORK-003 incident/ticket table read
WORK-004 filter/sort/edit admin table
WORK-005 attachment policy block
WORK-006 long form transaction with receipt
```

Done when:

```bash
cargo run -q -p saccade-gauntlet -- run-suite workarena-lite --engine chrome --replay
```

Pass target:

```text
forms discovered
tables read
safe fields filled
attachments require confirmation
receipt or final state verified
```

## 10. Gate 7 — PDF / document workflows

Purpose: PDF is a feature, not the main product, but documents matter for forms.

Targets:

- PDF.js viewer
- IRS public PDF forms
- local AcroForm fixture

Task families:

```text
PDF-001 open PDF.js viewer and navigate pages
PDF-002 search PDF text
PDF-003 detect report sections / tables if text layer exists
PDF-004 fill local AcroForm non-sensitive fields programmatically
PDF-005 block tax ID / signature / legal attestation
PDF-006 report unsupported for flat/scanned/XFA PDFs
```

Done when:

```bash
scripts/formmax_pdf_feasibility.py
cargo run -q -p saccade-gauntlet -- run-suite pdf --replay
```

Pass target:

```text
acroform_fields_detected
non_sensitive_fields_filled
sensitive_fields_empty_without_confirmation
flat_pdf_unsupported_reported
```

## 11. Gate 8 — Trusted Tabs and safety layer

Purpose: Saccade must be useful without becoming a spam/click-fraud/unsafe automation tool.

Required capabilities:

- tab_id scoping
- owner: Human | Agent
- Human tab input only from human
- Agent input refused on Human tab
- Agent read denied on Human tab unless explicit grant
- login handoff
- sensitive confirmation UI
- replay policy events

Task examples:

```text
SAFE-001 Human login handoff: user logs in, agent inherits session, password not exposed.
SAFE-002 Agent cannot input into Human tab.
SAFE-003 Agent cannot read Human tab without grant.
SAFE-004 User takes over Agent tab; agent pauses.
SAFE-005 public post requires confirmation.
SAFE-006 legal attestation requires confirmation.
SAFE-007 payment/money movement blocked or human-only.
```

Done when:

```bash
cargo run -q -p saccade-shell -- selftest-tabs
cargo run -q -p saccade-shell -- selftest-login-handoff
cargo run -q -p saccade-shell -- selftest-safety
```

Pass target:

```text
all selftests PASS
all replay entries include tab_id + owner + policy_decision
```

## 12. Gate 9 — Chrome adapter

Purpose: Servo proves the architecture; Chrome proves real-world compatibility.

Initial design:

- use Playwright as launcher if fastest
- do not expose Playwright semantics to agents
- Saccade owns truth/action/report/replay layer

Targets:

- DEVMAX fixture
- Cypress Real World App
- RealWorld / Conduit
- WebArena selected tasks

Done when:

```bash
cargo run -q -p devmax -- audit --engine chrome --url http://127.0.0.1:5173 --replay
cargo run -q -p saccade-gauntlet -- run-suite chrome-smoke --replay
```

Pass target:

```text
console/network captured
visual health report generated
action map generated
at least one form transaction run
replay generated
```

## 13. Gate 10 — Baselines

Purpose: Prove why Saccade exists.

Baselines:

- Playwright script baseline
- Playwright MCP / agent baseline if available
- screenshot-to-VLM loop baseline for selected tasks
- Browser Use / Stagehand optional comparison

Metrics:

```text
completion rate
steps/actions
LLM calls
token estimate
wall time
verification quality
replay/debug quality
unsafe action attempts
```

Task comparisons:

```text
MOUSEMAX: Saccade vs screenshot/VLM loop
FORMMAX: Saccade fill transaction vs Playwright field-by-field
DEVMAX: Saccade audit report vs screenshot agent repair loop
THREADMAX: Saccade shared view ledger vs ordinary page snapshot
```

Done when:

```bash
cargo run -q -p saccade-gauntlet -- compare --suite formmax --baseline playwright --replay
```

Public report should include:

```text
Saccade result
Baseline result
What the baseline was allowed to use
Why the comparison is limited
```

## 14. Final product-readiness scoreboard

Minimum public product launch bundle:

```text
MOUSEMAX evidence PASS
FORMMAX local runner PASS
DEVMAX fixture PASS
Trusted Tabs safety PASS
one Chrome adapter demo PASS
```

Strong public bundle:

```text
all minimum gates
WebArena selected tasks PASS
RealWorld/Discourse thread demo PASS
PDF side path PASS
baseline comparison report
```

## 15. Attack order

1. Freeze MOUSEMAX evidence.
2. Build Trusted Tabs + safety layer.
3. Build DEVMAX fixtures and report format.
4. Build FORMMAX Servo runner.
5. Build MCP skeleton.
6. Build Chrome adapter v0 for DEVMAX.
7. Attack UI Torture Suite.
8. Attack RealWorld / Discourse local.
9. Attack WebArena selected tasks.
10. Attack WorkArena-like tasks.
11. Attack PDF suite.
12. Publish comparison baselines.

## 16. Rule for claiming victory

A target is not conquered when Saccade clicks something.

A target is conquered only when:

```text
truth report exists
action map exists
action executed through approved input path
result verified
replay saved
failure modes classified
unsafe actions gated
baseline comparison exists or is explicitly deferred
```
