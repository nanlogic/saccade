# MCP Skeleton Report

Date: 2026-06-12

## What Was Added

`saccade-mcp` is the first agent-facing Saccade tool surface.

It does not yet run a full MCP transport server. It locks down the tool names, namespaces, compact JSON return policy, tab-scoping expectations, and sensitive-field policy gate that a real MCP server will expose.

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

## Current Coverage

- Registers the required `saccade.dev.*`, `saccade.tabs.*`, `saccade.web.*`, and `saccade.report.*` tools.
- Verifies Agent input is denied on Human tabs.
- Verifies Agent truth is denied on Human tabs without a read grant.
- Verifies Agent-owned tabs allow agent input and truth.
- Verifies Human tabs can expose summary truth only when explicitly granted.
- Verifies local dev audit accepts loopback URLs and returns compact JSON.
- Verifies local dev audit rejects public web URLs.
- Verifies normal fields are agent-fillable while sensitive payment fields require user input.

## Next

Turn this skeleton into a real MCP stdio or HTTP server that routes `saccade.dev.audit_page` to DEVMAX and routes web/form tools through Trusted Tabs, safety truth, replay, and policy gates.
