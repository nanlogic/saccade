# Migration 0007: reflex target and soft mouse

- Source commit: private legacy archive commit `8c4defb3f8b0`.
- Reviewed sources: `engines/cef/host/saccade_renderer.cc`, limited to the
  `.target:not(.hit)` current-target predicate and post-input refresh concept;
  `bins/saccade-mcp/src/main.rs`, limited to the bounded local
  observe/action/receipt loop pattern.
- Destinations: `extension/src/collector.js`,
  `extension/src/controls/reflex_target.js`, `crates/saccade_control_sdk`, and
  `crates/saccade_runtime`.
- Retained: current targets exclude `.hit` history, every occurrence receives a
  fresh opaque token, stale work is rejected and reobserved, and the repeated
  hot loop stays local after one bounded MCP request.
- New design: two explicit backends share the same transaction. `native` uses
  OS input and `soft` is limited to an Extension-dispatched reflex click.
  Receipts distinguish `accepted_by_os` from `accepted_by_software`.
- Verification: MouseAccuracy exposes safe score text as
  `reflex_occurrence` on a non-actionable loop-status object. The same loop
  class must advance that score; movement, disappearance, canvas change, or
  revision change alone is insufficient.
- Not migrated: CEF/Servo execution, monolithic classifiers, arbitrary canvas
  clicks, Agent coordinates, locators, page-script tools, detector routes, or
  legacy benchmark protocols.
- Fixtures and checks: `fixtures/conformance/reflex_target.html`, Extension
  protocol tests, SDK verifier tests, Runtime soft-dispatch tests, and
  `./scripts/dev.sh reflex` against the real site.
- Managed integration evidence: Chrome run `20260729T064526Z` reached
  `Insane + Tiny`; 31 software-dispatched hits advanced score with zero
  failures at 14.72 ms p50 and 15.76 ms p95 observation-to-receipt latency.
- Public status: `reflex_target` remains `implementation`. Local evidence does
  not make it publishable.
