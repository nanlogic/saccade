# MCP Skeleton Report

Date: 2026-06-13

## What Was Added

`saccade-mcp` is the first agent-facing Saccade tool surface.

It now has a minimal stdio JSON-RPC server for MCP-style clients. It locks down the tool names, namespaces, compact JSON return policy, tab-scoping expectations, and sensitive-field policy gate.

Implemented v0 tools:

- `saccade.dev.open_local`
- `saccade.dev.audit_page`
- `saccade.dev.click_all_primary_actions`
- `saccade.dev.fill_smoke_form`
- `saccade.dev.get_report`
- `saccade.tabs.list`
- `saccade.tabs.open`
- `saccade.tabs.request_user_login`
- `saccade.tabs.takeover`
- `saccade.tabs.pause_agent`
- `saccade.tabs.close`
- `saccade.web.truth`
- `saccade.web.actions`
- `saccade.web.act`
- `saccade.web.fill_agent_fields`
- `saccade.web.inspect_fields`
- `saccade.web.fill_form`
- `saccade.report.validate_run`
- `saccade.report.replay_summary`

## Commands

List registered tools:

```bash
cargo run -q -p saccade-mcp -- tools
```

Run the skeleton gate:

```bash
cargo run -q -p saccade-mcp -- selftest
```

Expected shape:

```text
MCP PASS tools_registered=19 tab_scoping=true local_dev_audit=true policy_gate=true report=...
```

Run the stdio server:

```bash
cargo run -q -p saccade-mcp -- serve-stdio
```

Call `saccade.dev.audit_page` through the stdio handler with a loopback URL:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"saccade.dev.audit_page","arguments":{"url":"http://127.0.0.1:5173/","engine":"static","replay":true}}}
```

## Current Coverage

- Registers the required `saccade.dev.*`, `saccade.tabs.*`, `saccade.web.*`, and `saccade.report.*` tools.
- Verifies Agent input is denied on Human tabs.
- Verifies Agent truth is denied on Human tabs without a read grant.
- Verifies Agent-owned tabs allow agent input and truth.
- Verifies Human tabs can expose summary truth only when explicitly granted.
- Verifies local dev audit accepts loopback URLs and returns compact JSON.
- Verifies local dev audit rejects public web URLs.
- Verifies `initialize`, `tools/list`, and `tools/call` over the JSON-RPC handler.
- Routes `saccade.dev.audit_page(engine=servo)` to the live browser worker when an Agent-owned tab has one; otherwise routes static/local audits to DEVMAX and records report paths. `engine=chrome` is available for local/file URLs and returns Chrome screenshot/truth/network artifact paths through the DEVMAX report.
- Maintains persistent tab state across stdio requests.
- Exposes `saccade.web.truth` and `saccade.web.actions` from the live browser worker when available, with DEVMAX report state as fallback.
- Runs `saccade.web.act` v0 through the live browser worker when available, with Servo-backed DEVMAX verification as fallback.
- Runs `saccade.web.fill_agent_fields` through the live browser worker with a required `basis_page_revision`; only Agent-owned non-sensitive fields are filled, and sensitive/Human-owned fields are rejected by worker policy.
- Runs `saccade.web.inspect_fields` through the live browser worker for explicitly named fields; non-sensitive values can be returned to the agent, while sensitive fields return status only.
- Runs `saccade.web.fill_form` v0 against the local FORMMAX fixture, blocks sensitive fields, validates the result, and returns result/replay/screenshot artifact paths.
- Runs `saccade.dev.click_all_primary_actions` v0 through Servo-backed DEVMAX verification when the local page has at most one primary action.
- Routes `saccade.dev.fill_smoke_form` to the same FORMMAX local fixture workflow.
- Creates Human-owned login tabs through `saccade.tabs.request_user_login` without exposing credentials to agent truth.
- Loads compact reports through `saccade.dev.get_report` without returning full artifacts.
- Validates generic run directories and FORMMAX run directories through `saccade.report.validate_run`.
- Summarizes replay JSONL through `saccade.report.replay_summary`, including event counts and value-like field detection.
- Appends generated DEVMAX/FORMMAX artifact paths to `runs/mcp/artifacts.jsonl` so later agents can find evidence without relying on chat history.
- Verifies normal fields are agent-fillable while sensitive payment fields require user input.
- Agent-owned local tabs now spawn a live `browser_session_worker_v0` child process. `saccade.dev.audit_page(engine=servo)`, `saccade.web.truth`, `saccade.web.actions`, `saccade.web.act`, `saccade.web.fill_agent_fields`, and `saccade.web.inspect_fields` use that worker before falling back to report-backed DEVMAX where fallback is safe.
- Human takeover closes the live worker before changing ownership.
- The live worker returns artifact paths and writes compact `report.json` plus `replay.jsonl` under `runs/browser_session_worker/worker_*/`.
- The live worker redacts form values before truth/actions/audit leave the browser process. Sensitive controls expose kind and completion status only.
- The live worker saves screenshot PNG artifacts only when no sensitive fields are detected. Sensitive pages log a skip event instead.
- `saccade.report.validate_run` accepts `kind=browser_session_worker` and verifies worker report/replay shape, screenshot references, and replay raw-value leak checks.
- Browser-session smoke remains available outside MCP: `saccade-shell selftest-browser-session` proves open, truth, action map, native act, and truth-after-act on one Servo WebView path.
- Latest selftest evidence: `/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781363828594/report.json`.

## Next

Harden the browser worker with a shared multi-tab process, FORMMAX live-tab integration, Chrome-side click verification, richer DEVMAX findings that reuse live worker or Chrome screenshots, and UI controls around Human/Agent ownership.
