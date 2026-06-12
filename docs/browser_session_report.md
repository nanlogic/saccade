# Browser Session Smoke Report

Date: 2026-06-12

## What Was Added

`saccade-shell selftest-browser-session` is the first explicit browser-backed session smoke.

It opens a local page in Servo, collects browser truth and an action map, dispatches one native Servo mouse click, then collects post-action truth from the same WebView path. The fixture advances `data-session-revision` from `0` to `1`, so the gate verifies a visible page-state change rather than only an input event.

The MCP path now has a live worker as well: Agent-owned local tabs spawn `saccade-shell browser-session-worker --url ...`, and `saccade.web.truth`, `saccade.web.actions`, `saccade.web.act`, and `saccade.tabs.close` talk to that worker over JSONL.

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
- Proves the MCP stdio path can keep a browser worker alive across `tabs.open -> web.actions -> web.act -> tabs.close`.
- Produces compact report/replay artifacts without echoing full page text.
- Uses the existing Servo event-loop/input/evaluate APIs already recorded in `docs/servo_api_map.md`.

## Still Pending

- MCP still uses DEVMAX/FORMMAX child tools for dedicated audit and form workflows.
- The worker is one Agent tab per child process; multi-tab shared browser process, screenshots, replay artifacts, and sensitive-value redaction inside arbitrary pages remain next hardening steps.
