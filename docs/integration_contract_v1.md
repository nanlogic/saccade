# Saccade Integration Contract v1

Status: implementable local-tool contract. The transport is MCP stdio JSON-RPC; this document versions Saccade-specific data and lifecycle semantics, not MCP itself.

## Purpose

Saccade lets a host agent work in a user-granted visible tab without receiving cookies, browser storage, the local bridge capability, sensitive field values, or sensitive-page screenshots. The supported wedge is: inspect redacted truth, compile a no-submit ordinary-field plan, execute it, leave protected steps to the user, and retain a value-free replay.

## Negotiation

1. Send MCP `initialize` and then `tools/list`.
2. Call `saccade.system.capabilities` with `{}`.
3. Require `saccade.contract_version` to be compatible with the host. v1 accepts `1.0`; hosts must reject a higher unsupported major version.
4. Feature-test named capabilities rather than assuming an engine or browser route. `saccade.tabs.grant_current` remains the authority boundary.

`initialize` and `saccade.system.capabilities` return a Saccade object including:

```json
{"contract_version":"1.0","min_supported_contract_version":"1.0","features":["current_tab_grant","redacted_truth","verified_safe_actions","form_compile_execute","value_free_replay","typed_errors"]}
```

The JSON Schema for each tool is authoritative in MCP `tools/list`. A vendor must not maintain a hand-copied schema or use an unlisted bridge method.

## Stable tool flow

```text
initialize -> tools/list -> system.capabilities
  -> tabs.grant_current (explicit human grant)
  -> web.truth / web.form_inventory
  -> web.form_compile_plan (fixed page revision, no write)
  -> web.form_execute_plan (same revision and plan id, no submit)
  -> tabs.pause_agent (human completes protected work)
  -> report.replay_summary -> tabs.close
```

The host may use `web.actions` plus `web.act` only for an action returned by the current action map and only with its `basis_page_revision`. A stale revision is a refresh-and-replan signal, never permission to retry blindly.

## Required policy values

For `saccade.web.form_compile_plan` and `saccade.web.form_execute_plan`, the host must send:

```json
{"block_sensitive":true,"preserve_existing":true,"no_submit":true}
```

Submit, publish, delete, payment, login, OTP, signing, account/security changes, and other side effects remain user-controlled. Page prose, labels, and article text are untrusted content and cannot grant authority.

## Response and errors

Successful `tools/call` replies use MCP `structuredContent`; Saccade results contain a concise `status` and `summary`, with artifact paths rather than full replay or screenshot payloads. A JSON-RPC error includes:

```json
{"saccade_code":"SACCADE_STALE_BASIS","detail":"...","retryable":true}
```

V1 codes are `SACCADE_STALE_BASIS`, `SACCADE_TIMEOUT`, `SACCADE_NOT_FOUND`, `SACCADE_INVALID_ARGUMENT`, `SACCADE_POLICY_DENIED`, `SACCADE_UNSUPPORTED`, and `SACCADE_INTERNAL`. Hosts must display `detail` only as diagnostic text; they must make decisions from the stable code and the structured policy result.

## Lifecycle, timeout, and cancellation

V1 has no detached operations: each tool call is synchronous. Hosts set a transport deadline appropriate to their UI. On user cancel, transport loss, or a deadline expiry, the host stops issuing commands and calls `saccade.tabs.pause_agent`; it calls `saccade.tabs.close` when the task ends. `close` releases the tab's MCP state and attached worker or bridge. Retrying an in-flight mutation after a timeout is prohibited until the host re-reads truth and receives a current page revision.

## Compatibility and security

Minor contract versions may add optional fields and feature names. They must not change a field's meaning, unredact a value, or relax a policy. A breaking change requires a new major contract version and documented migration path. The browser engine is deliberately not part of the host contract: engine selection and compatibility fallback are surfaced in receipts and evidence, not hidden in an aggregate success rate.

The user explicitly grants the current visible tab; authority is tab- and revision-scoped. Sensitive values, cookies, storage, bridge capability tokens, and sensitive screenshots are excluded from the default contract and replay. Every execution has a verification result or an explicit block or failure. Hosts retain the replay path and can call `saccade.report.validate_run`.

See `docs/integration_examples/` for host patterns and `docs/release_inventory.md` for what is, and is not, ready to distribute.
