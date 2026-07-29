# Migration 0006: native mouse accuracy gate

- Source commit: private legacy archive commit `8c4defb3f8b0`.
- Reviewed source: `scripts/probe_cef_human_input_macos.py`, specifically the
  CoreGraphics HID-system event source and `mouseMoved`, `leftMouseDown`, and
  `leftMouseUp` timing.
- Destination: `crates/saccade_runtime/src/platform_input/macos.rs`.
- Retained: one HID-system event source, a real move to the prepared center,
  50 ms move settle, 50 ms down/up separation, and Accessibility-gated
  CoreGraphics posting.
- Not migrated: CEF, Servo, renderer-native clicks, WebDriver, CDP,
  screenshots, page JavaScript actions, old classifiers, benchmark MCP tools,
  or the legacy reflex loop.
- New gate: `fixtures/conformance/mouse_accuracy.html`, the
  `mouse_accuracy` probe mode, and `./scripts/dev.sh accuracy`. The fixture has
  24 normal static targets at 32, 40, and 48 CSS pixels across horizontal and
  scrolled positions. The probe chooses semantic button names and opaque action
  tokens only.
- Environment finding: an unrelated Codex Pet layer-3 window intercepted
  clicks over the right side of a 1200-pixel browser window. The closed loop
  truthfully returned `unverified`. Managed browser geometry is now fixed at
  800 by 747 for unobstructed measurement; old profiles are retained.
- Recovery finding: after a Native Host reconnect, the collector could be one
  revision ahead of the Host indefinitely. Stale preparation still rejects,
  then emits a fresh full observation so a new request can recover.
- Native evidence: paired managed rerun `20260729T053405Z` passed 24/24 targets
  in Chrome for Testing and 24/24 in Microsoft Edge with zero misses on reused
  browser profiles.
- Public status: this is local development evidence. It does not promote any
  Catalog row or replace signed-product release evidence.
