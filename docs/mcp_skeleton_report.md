# MCP Skeleton Report

Date: 2026-06-12

## What Was Added

`saccade-mcp` is the first agent-facing Saccade tool surface.

It now has a minimal stdio JSON-RPC server for MCP-style clients. It locks down the tool names, namespaces, compact JSON return policy, tab-scoping expectations, and sensitive-field policy gate.

Implemented v0 tools:

- `saccade.dev.open_local`
- `saccade.dev.audit_page`
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
MCP PASS tools_registered=17 tab_scoping=true local_dev_audit=true policy_gate=true report=...
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
- Routes `saccade.dev.audit_page` to DEVMAX and records the DEVMAX report path.
- Maintains persistent in-memory tabs across stdio requests.
- Exposes `saccade.web.truth` and `saccade.web.actions` from DEVMAX report state.
- Runs `saccade.web.act` v0 through a Servo-backed DEVMAX verification pass for the first enabled action in the current action map.
- Creates Human-owned login tabs through `saccade.tabs.request_user_login` without exposing credentials to agent truth.
- Loads compact reports through `saccade.dev.get_report` without returning full artifacts.
- Validates generic run directories and FORMMAX run directories through `saccade.report.validate_run`.
- Summarizes replay JSONL through `saccade.report.replay_summary`, including event counts and value-like field detection.
- Verifies normal fields are agent-fillable while sensitive payment fields require user input.

## Next

Complete MCP protocol polish, move tab state from in-memory v0 to a browser-backed tab session, and route the remaining form/report tools through Trusted Tabs, safety truth, replay, and policy gates.
