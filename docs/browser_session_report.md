# Browser Session Smoke Report

Date: 2026-06-13

## What Was Added

`saccade-shell selftest-browser-session` is the first explicit browser-backed session smoke.

It opens a local page in Servo, collects browser truth and an action map, dispatches one native Servo mouse click, then collects post-action truth from the same WebView path. The fixture advances `data-session-revision` from `0` to `1`, so the gate verifies a visible page-state change rather than only an input event.

The MCP path now has a live worker as well: Agent-owned local tabs spawn `saccade-shell browser-session-worker --url ...`, and `saccade.dev.audit_page(engine=servo)`, `saccade.web.truth`, `saccade.web.actions`, `saccade.web.act`, and `saccade.tabs.close` talk to that worker over JSONL.

The worker now writes compact artifacts under `runs/browser_session_worker/worker_*/` and redacts field values before data leaves the browser process. Sensitive fields expose type and completion status, not raw values. Non-sensitive pages also receive screenshot PNG artifacts; pages with sensitive fields skip screenshots and record that policy decision in replay.

Live worker audit is intentionally compact. It converts the current live probe into action counts, screenshot policy, and findings for blank pages, offscreen actions, blocked actions, and sensitive fields that require user handling. Static or non-live audits still use DEVMAX as fallback.

The live worker is now interactive enough for Wayne-in-the-loop dogfood. It forwards real mouse, wheel, keyboard, browser back/forward/reload shortcuts, and simple native select control input into the Servo WebView. It also accepts a constrained `fill_agent_fields` JSONL request: only fields marked `data-owner="agent"` and `data-sensitive="none"` can be filled, sensitivity is recomputed in-page before writing, and replay records only field IDs plus rejection reasons.

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
- Uses the existing Servo event-loop/input/evaluate APIs already recorded in `docs/servo_api_map.md`.

## Still Pending

- MCP still uses DEVMAX/FORMMAX child tools for static audit fallback, click-all verification, and bulk form workflows.
- The worker is one Agent tab per child process; multi-tab shared browser process and FORMMAX live-tab integration remain next hardening steps.
- Product UI still needs explicit Human/Agent badges and a polished handoff surface around the worker capability.
