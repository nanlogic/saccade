# Changelog

Saccade has no stable release yet. This file records user-visible changes from
the clean public repository.

## Unreleased

### Added

- one Catalog-backed Registry for button, text field, search field, textarea,
  contenteditable, spin button, checkbox, and select;
- the Extension → Native Host → Runtime → MCP production route;
- native macOS and Windows input adapters;
- managed macOS Chrome for Testing development and evidence commands;
- managed macOS Edge development with isolated profiles and evidence;
- an ordinary 24-target native mouse-accuracy gate for managed Chrome and Edge;
- an audited MouseAccuracy reflex target, bounded local MCP loop, and explicit
  native/soft input receipts with causal score verification;
- exact-PID managed-window move and resize phases for native accuracy evidence;
- three-field Profiles with Agent behavior and named-control bans;
- stale, replay, focus, coverage, postcondition, and value-leak checks;
- stale-preparation observation resynchronization without weakening rejection;

### Pending

- same-candidate Chrome and Edge release evidence;
- signed consumer packaging and browser-store Extension builds;
- the remaining control batches listed in `docs/CONTROL_ROADMAP.md`.
