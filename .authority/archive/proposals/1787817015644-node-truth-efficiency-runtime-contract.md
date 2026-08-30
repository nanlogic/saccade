---
authority-topic: saccade-0-2-0-runtime-contract
authority-scope: .
authority-owner: human-owner
authority-revision: 3
---

# Saccade 0.2.0 Node Broker contract

This revision adds authenticated, metadata-only Broker crash recovery and
restores bounded working-set and receipt behavior lost during the Node rewrite.

- R-001 — The implementation is Node.js plus the shared Chrome/Edge Extension; no Rust or platform-specific local driver remains.
- R-002 — One long-lived Node Broker multiplexes Extension connections and MCP sessions. MCP stdio adapters reconnect to the Broker without owning browser state themselves.
- R-003 — The Broker queue uses unique command IDs, bounded delivery, explicit acknowledgement, one in-flight delivery per command, and connection epochs.
- R-004 — A command not delivered before cancellation is removed. A delivered action is never replayed; disconnect before acknowledgement yields `outcome_unknown` and `retry_safe:false`.
- R-005 — Idempotent read commands may be redelivered only inside the same request deadline and only when no response was acknowledged.
- R-006 — Extension reconnect publishes its browser identity, current lease metadata, and a fresh full Truth snapshot for every still-authorized tab. Broker gaps never fabricate deltas.
- R-007 — `tabs.open` records the calling `agent_session_id` in the command; the returned tab is atomically leased to that session before it is visible through MCP.
- R-008 — `tabs.list`, `tabs.close`, `truth.read`, and `saccade.act` expose or operate only tabs leased to the calling session. No all-tabs Truth method exists.
- R-009 — `truth.read` requires `tab_id` and an explicit `mode` of `full` or `delta`. Delta reads also carry `after_revision`; unavailable continuity returns `reset_required` and current revision rather than silently sending a full page.
- R-010 — Full reads return the bounded canonical Truth for exactly one tab. Delta reads return only source-declared changes after the requested revision. Optional semantic queries further bound either response without changing canonical collection.
- R-011 — Agent responses always identify `tab_id`, `document_id`, revision, mode, completeness, and the next revision basis so the Agent knows exactly what it owns and what it has read.
- R-012 — One monotonic request deadline covers MCP → Broker → Extension → Collector. No layer restarts a timeout, and a disconnected Extension fails immediately when no reconnect is already active.
- R-013 — Diagnostics are a bounded metadata-only ring: session/tab/document, command stage, timestamps, sizes, reconnect reason, and failure code. It never stores editable/protected values, tokens, credentials, cookies, storage, full DOM, or screenshots.
- R-014 — Chrome and Edge run the identical Extension and Node Broker candidate for full, delta, reconnect/reset, open/close, stale replacement, action, Broker restart, and concurrent Agent-session conformance.
- R-015 — The Broker keeps an atomic, bounded recovery journal containing only hashed session proofs, exact Tab lease metadata, and value-free command occurrence metadata. Canonical Truth, deltas, action payloads, editable or protected values, action tokens, credentials, cookies, and storage are never persisted.
- R-016 — A usable session-resume proof exists only in the live MCP adapter's memory. Session-scoped loopback requests require that proof as well as the `agent_session_id`; the Broker stores only its hash and rotates the proof after every successful resume.
- R-017 — After Broker restart, previously active leases are recoverable but unavailable. The same still-running MCP connection may prove and resume its exact `agent_session_id`, atomically reactivating only that session's leases. This recovery does not create a new MCP connection or assign a new session.
- R-018 — A new MCP connection never receives another session's recovery proof. Without valid proof, a recoverable or orphaned lease is not exposed, closed, reassigned, or transferred.
- R-019 — Truth is always empty after Broker restart and becomes readable only after the Extension reconnects and supplies a fresh exact-tab full snapshot. A persisted lease never implies persisted action authority.
- R-020 — A command recorded as dispatched when the Broker stops becomes `outcome_unknown` with `retry_safe:false` on recovery and is never placed back in the delivery queue.
- R-021 — After proven recovery, only idempotent reads and requests rejected before dispatch may be retried automatically. `tabs.open`, `tabs.close`, and `saccade.act` are never retried after an ambiguous transport failure.
- R-022 — `truth.read` may declare `min_objects` and `timeout_ms`. The Broker waits on browser-pushed Truth events until the bounded semantic condition is satisfied; the Agent does not poll or sleep.
- R-023 — A semantic working set filters its related authorities and changes to the returned object identities. Known `object_ids`, roles, affordances, visibility, and safe text are bounded projection inputs, never selectors or execution authority.
- R-024 — Public MCP schemas avoid top-level `oneOf`, `anyOf`, and `allOf`; strict mutually-exclusive request validation remains Broker-owned so incompatible Agent registries do not lose the tool.
- R-025 — Action receipts project a compact relevant delta for the acted object identities and selected options. Canonical Truth retains unrelated geometry and page churn for later explicit reads.
