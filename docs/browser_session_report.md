# Browser Session Smoke Report

Date: 2026-06-13

## What Was Added

`saccade-shell selftest-browser-session` is the first explicit browser-backed session smoke.

It opens a local page in Servo, collects browser truth and an action map, dispatches one native Servo mouse click, then collects post-action truth from the same WebView path. The fixture advances `data-session-revision` from `0` to `1`, so the gate verifies a visible page-state change rather than only an input event.

The MCP path now has a live worker as well: Agent-owned local tabs spawn `saccade-shell browser-session-worker --url ...`, and `saccade.dev.audit_page(engine=servo)`, `saccade.web.truth`, `saccade.web.actions`, `saccade.web.act`, `saccade.web.fill_agent_fields`, `saccade.web.inspect_fields`, `saccade.web.fill_form`, and `saccade.tabs.close` talk to that worker over JSONL.

The worker now writes compact artifacts under `runs/browser_session_worker/worker_*/` and redacts field values before data leaves the browser process. Sensitive fields expose type and completion status, not raw values. Non-sensitive pages also receive screenshot PNG artifacts; pages with sensitive fields skip screenshots and record that policy decision in replay.

Live worker audit is intentionally compact. It converts the current live probe into action counts, screenshot policy, and findings for blank pages, offscreen actions, blocked actions, and sensitive fields that require user handling. Static or non-live audits still use DEVMAX as fallback.

The live worker is now interactive enough for Wayne-in-the-loop dogfood. It forwards real mouse, wheel, keyboard, browser back/forward/reload shortcuts, and simple native select control input into the Servo WebView. It also accepts a constrained `fill_agent_fields` JSONL request: only fields marked `data-owner="agent"` and `data-sensitive="none"` can be filled, sensitivity is recomputed in-page before writing, and replay records only field IDs plus rejection reasons. A separate `inspect_fields` request can check explicitly named fields: non-sensitive values may be returned to the agent, while sensitive fields return completion status only.

FORMMAX live fill is now connected to the same worker tab. The worker can fill the 96-row, two-page local capacity fixture inside the visible browser session, block three sensitive fields, submit the receipt, and write replay evidence without raw table values.

Focused typing is now available for real-site human-in-the-loop dogfood. The user focuses a destination field in the live browser, then the worker sends text only to the current focused element after classifying the field as non-sensitive. Password, OTP, government/tax ID, payment, signature, and legal attestation fields are blocked. Replay records field metadata and before/after lengths, not the typed text.

## Evidence

Command:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-browser-session
```

Expected shape:

```text
BROWSER_SESSION PASS run_id=... session=... tab=agent-tab-1 actions_seen=1 revision=0=>1 report=... replay=...
```

Safe fill worker probe:

```bash
printf '%s\n' \
  '{"id":1,"method":"truth"}' \
  '{"id":2,"method":"fill_agent_fields","params":{"fields":{"task-1":"agent-one","task-2":"agent-two","ssn":"SHOULD-NOT-WRITE","tax-id-empty":"SHOULD-NOT-WRITE"}}}' \
  '{"id":3,"method":"truth"}' \
  '{"id":4,"method":"close"}' \
| RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/login_handoff/user_flow.html
```

Observed safe-fill result:

```text
filled=["task-1","task-2"]
rejected=["ssn","tax-id-empty"]
sensitive_fields_seen=3
values_logged=false
```

Explicit field inspect probe:

```text
agent-page2-code -> value returned
user-quantity -> non-sensitive value returned when explicitly requested
signature -> requires_user_input, value_redacted=true
tax-id-empty -> requires_user_input, value_redacted=true
replay fields_inspected -> values_logged=false
```

Manual dogfood evidence:

```text
runs/browser_session_worker/worker_1781353129472
fill page 1: 4 filled, ssn/tax-id rejected
fill page 2: 2 filled, user-quantity/signature rejected
inspect: 3 non-sensitive values returned, 3 sensitive values redacted
artifact grep: no field values or sensitive fixture value found
```

Live FORMMAX evidence:

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-formmax-live
FORMMAX_LIVE PASS rows=96 pages=2 filled=672 blocked_sensitive=3 receipt_verified=true replay=runs/browser_session_worker/worker_1781367973334_69584/replay.jsonl
```

MCP evidence:

```text
RUST_LOG=error cargo run -q -p saccade-mcp -- selftest
MCP PASS tools_registered=19 tab_scoping=true local_dev_audit=true policy_gate=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781368050809/report.json
```

Focused typing evidence:

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-focused-type
FOCUSED_TYPE PASS chars=22 after_length=22 sensitive_blocked=true replay=runs/browser_session_worker/worker_1781387505146_27892/replay.jsonl
```

Artifacts are written under:

```text
runs/browser_session/session_*/report.json
runs/browser_session/session_*/replay.jsonl
runs/browser_session_worker/worker_*/report.json
runs/browser_session_worker/worker_*/replay.jsonl
```

## Current Scope

- Proves `open -> truth -> actions -> act -> truth_after_act` on a Servo-backed WebView.
- Proves the MCP stdio path can keep a browser worker alive across `tabs.open -> dev.audit_page -> web.actions -> web.act -> tabs.close`.
- Produces compact report/replay artifacts without echoing full page text.
- Saves screenshot PNG artifacts for pages without sensitive fields.
- Skips screenshot capture when sensitive fields are present, instead logging `screenshot_skipped_sensitive_fields`.
- Redacts arbitrary page form values from worker truth/actions. Sensitive fields expose `sensitivity.kind` and `completion_state`.
- Supports manual user input in the same live worker window.
- Supports constrained agent fill for agent-owned, non-sensitive fields.
- Rejects human-owned or sensitive fields even if a caller asks to fill them.
- Supports explicit field inspection for user review flows: non-sensitive values can be checked only when named, sensitive values stay masked.
- Supports live FORMMAX fill through `saccade.web.fill_form` when called with a live Agent tab, `basis_page_revision`, and `live_worker_only=true`.
- Supports focused text typing for real-site dogfood: the user chooses the field by focusing it, agent text is typed only if the active element is non-sensitive, and replay avoids typed-text logging.
- Worker run directories include the process ID in addition to a millisecond timestamp to avoid concurrent artifact collisions.
- Uses the existing Servo event-loop/input/evaluate APIs already recorded in `docs/servo_api_map.md`.

## Still Pending

- MCP still uses DEVMAX/FORMMAX child tools for static audit fallback, click-all verification, and bulk form workflows.
- MCP exposes `fill_agent_fields`, `inspect_fields`, and live `fill_form` as first-class live-worker tools; direct worker protocol remains useful for low-level debugging.
- The worker is one Agent tab per child process; multi-tab shared browser process remains next hardening work.
- Product UI still needs explicit Human/Agent badges and a polished handoff surface around the worker capability.
