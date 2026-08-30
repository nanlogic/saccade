# Migration 0001: Node bounded reflex loop

- Historical reviewed integration: repository commit `bac8a5d94fa600e8b522f338ddbc0ad94f0231e5`.
- Original private-archive sources recorded by that integration: commit
  `8c4defb3f8b0`, limited to the `.target:not(.hit)` current-target predicate in
  `engines/cef/host/saccade_renderer.cc` and the bounded local
  observe/action/receipt loop in `bins/saccade-mcp/src/main.rs`.
- Current destinations: `packages/setup/src/broker.js` and
  `packages/setup/src/mcp.js`.
- Retained: one bounded Agent request, current same-document loop-class
  authority, exact current action token per occurrence, pre-dispatch stale
  recovery, occurrence advancement verification, compact value-free report,
  and no retry after ambiguous dispatch.
- Reimplemented: Node Broker owns the loop and sends ordinary exact `act`
  commands to the Extension. The capability is a bounded form of the existing
  `saccade.act`, not another public tool.
- Classic bridge: MouseAccuracy Classic retains its live score only in
  page-private JavaScript until the result screen. The Extension records each
  completed exact current-target software click dispatch as the live bounded
  occurrence; the final semantic result remains the independent aggregate
  page-owned verification. No ambiguous dispatch is replayed.
- Not migrated: Rust runtime, native/OS input, CEF or Servo, coordinates,
  selectors, locators, page-script tools, model polling, screenshots, or the
  historical reference-actuator namespace.
- Focused checks: MCP schema preserves exactly six public tools; Broker tests
  prove one request dispatches the current target and verifies occurrence
  advancement before reporting success.
- Release status: implementation evidence only until the current Node and
  Extension candidate passes real Chrome and Edge conformance.
- Chrome evidence: candidate
  `af09e65c83d1ea38ebdf6abe88804cd056e2f57a0492c8de4f3365df1f1943d7`
  completed the official Classic `Epic` + `Tiny` run with 49 verified
  occurrences, a final page score of 49 targets, and 0 misclicks. Edge
  same-candidate evidence remains required for release.
