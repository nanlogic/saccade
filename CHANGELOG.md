# Changelog

Saccade has no stable release yet. This file records user-visible changes from
the clean public repository.

## Unreleased

### Changed

- hard-cut the default MCP surface to the four-tool Truth API, advance
  capabilities to `saccade.capabilities/5`, remove action authority from the
  default Agent view, and assign browser execution to the Agent client;
- split execution metadata into the Reference Actuator catalog and move the
  old action/form/reflex tools under explicit `saccade.reference.*` names;
- stop default startup from requesting Accessibility or loading local input
  policy; reference execution loads both boundaries only when explicitly used;
- add a Chrome/Edge Truth-only gate for all 34 protocol roles, 12 reusable
  variants, and 6 structural/push boundaries, including safe projection,
  absent action authority, and browser-pushed state deltas;
- keep the 15-family Reference Actuator gate separate from the complete
  machine-readable Truth inventory;
- add a generated 63-row public Truth denominator covering every role,
  variant, structural/push boundary, and recorded lifecycle scenario without
  hiding blocked evidence;
- require an explicitly configured same-tab external web-act MCP before a fair
  Saccade/Playwright run; otherwise mark the complete comparison blocked and do
  not run an unmatched Playwright lane;
- implement the remaining roadmap Truth projections for semantic text/list/
  table/row containers, sliders, bound labels, explicit generic/drag controls,
  date/time/month/week/datetime/color inputs, and opaque or restricted
  Canvas/WebGL/video/document surfaces;

- move semantic change compilation to the Extension Truth Layer source; MCP now
  compacts and aliases source deltas instead of independently interpreting two
  page snapshots;
- advance the development Extension and managed-browser profile generations
  with the source-delta compiler, preventing an older cached MV3 worker from
  injecting a newer Collector alone;
- compact repeated Agent Browser defaults without changing the v1 Host wire
  schemas or semantic truth;
- replace native select's redundant post-action sleep with fresh
  selected-option verification and shorten the measured macOS popup handoff;
- bind macOS keyboard delivery to the exact browser process that launched the
  Native Host, after the real click and bounded focus handoff;
- start authorized collection when an HTTP(S) document is loading instead of
  waiting indefinitely for every third-party resource to complete;
- project long internal object identities as short document-scoped Agent
  aliases and retain 128-bit opaque action-token authority.
- let `web.observe` wait locally for a revision newer than `after_revision`, so
  an Agent does not spend tool calls or context polling unchanged truth.
- let collapsed ARIA choices complete a verified expand loop before their
  dynamically created option identities enter the existing select loop;
- disambiguate duplicate actionable controls across control families with
  bounded value-free page-authored context.
- honor explicit ARIA structural roles before native tag fallbacks, including
  live status regions authored on paragraph elements.
- retain bounded Extension delta history in the Host so action settlement and
  skipped authority-only revisions cannot erase an unconsumed semantic change;
- replace programmatic Collector injection with one ordered dormant static
  bundle and a long-lived authorized content-script Port;
- expose each authorized Agent Browser as a subscribable MCP Resource with
  unsolicited URI-only update notifications and full/delta reads;

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
- bounded non-actionable structural reading for headings, paragraphs, list
  items, table cells, alerts, and status messages;
- native select, ARIA listbox, and ARIA combobox selection through enabled
  option-object identity, including duplicate visible names;
- session-only Extension popup controls for sharing and revoking existing tabs;
- monotonic per-tab document handling so delayed retired-document observations
  cannot replace current state or contaminate action receipts;
- Catalog-declared automatic software/native input selection for finite click
  controls versus controls that require operating-system input;
- a value-free user-local input-policy log that learns verified page/control
  behavior, upgrades future actions to native after an unverified software
  receipt, and never retries the same token;
- MCP tools to inspect the learned log and remember a native-input exception
  for a current control;
- per-Agent Browser views that return one full Truth Layer followed by semantic
  deltas, while complete Host snapshots remain local verification evidence;
- one bounded form-fill MCP plan that locally orchestrates fresh independent
  control loops and returns value-free step summaries;
- collector-ready `tabs.open` results and compact structured MCP receipts
  without duplicating full JSON as text;
- verifier-aware action settlement and bounded pre-dispatch stale refreshes for
  locally orchestrated form steps;
- declarative cross-site evidence, isolated unknown-page Saccade/Playwright
  comparisons, stable failure taxonomy, timeout evidence, and artifact redaction;

### Pending

- source-diverse public compatibility evidence across Selenium, WAI-ARIA APG,
  Angular, Vue, Web Components, dynamic replacement, delayed render, and
  frames;
- three fair unknown-page Playwright comparisons using Saccade Truth plus the
  Agent client's own web-act tool, never the Reference Actuator;
- lifecycle evidence for dynamic loading, disappearance, overlays/modals,
  infinite scroll, sortable tables, dialogs, slow resources, upload/download,
  large rearrangements, and viewport changes;
- same-candidate Chrome and Edge release evidence, signed consumer packaging,
  and browser-store Extension builds.
