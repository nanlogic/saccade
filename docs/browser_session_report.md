# Browser Session Smoke Report

Date: 2026-06-12

## What Was Added

`saccade-shell selftest-browser-session` is the first explicit browser-backed session smoke.

It opens a local page in Servo, collects browser truth and an action map, dispatches one native Servo mouse click, then collects post-action truth from the same WebView path. The fixture advances `data-session-revision` from `0` to `1`, so the gate verifies a visible page-state change rather than only an input event.

## Evidence

Command:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-browser-session
```

Expected shape:

```text
BROWSER_SESSION PASS run_id=... session=... tab=agent-tab-1 actions_seen=1 revision=0=>1 report=... replay=...
```

Artifacts are written under:

```text
runs/browser_session/session_*/report.json
runs/browser_session/session_*/replay.jsonl
```

## Current Scope

- Proves `open -> truth -> actions -> act -> truth_after_act` on a Servo-backed WebView.
- Produces compact report/replay artifacts without echoing full page text.
- Uses the existing DEVMAX probe path, so no new Servo API surface was introduced.

## Still Pending

- MCP tabs are still in-memory/report-backed v0.
- The next step is a long-lived browser session actor that MCP can send tab commands to without recreating a WebView per action.
