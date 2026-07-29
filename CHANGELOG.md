# Changelog

Saccade has no stable release yet. This file records user-visible changes from
the clean public repository.

## Unreleased

### Added

- one Catalog-backed Registry for button, link, text field, search field,
  textarea, contenteditable, spin button, checkbox, radio, ARIA switch, select,
  tab, menu item, reflex target, and file input;
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
- observation refresh filtering so unrelated page mutations do not churn
  otherwise-current control tokens;
- native link navigation and file selection, including transient chooser
  buttons, path-free receipts, and bounded macOS/Windows chooser plans;
- bounded visible action-group context for repeated generic controls, plus
  deduplicated file/image chooser triggers for cover and screenshot uploads;
- versioned unpacked-Extension directories and browser-profile generations so
  MV3 worker updates do not require reading or copying login cookies;
- human-only managed Profile selection with the bundled smart-barbarian-eco
  Profile;
- explicit restricted reporting for browser-owned confirmation dialogs;
- non-actionable, application-declared semantic image identity;
- public W3C WAI-ARIA dogfood for radio, switch, tab, and menu item, with an
  isolated Playwright comparison oracle and screenshots;
- accessible fallback names that omit `aria-hidden` descendants and explicit
  ARIA menu-item precedence over native link projection;

### Pending

- same-candidate Chrome and Edge release evidence;
- signed consumer packaging and browser-store Extension builds;
- the remaining control batches listed in `docs/CONTROL_ROADMAP.md`.
