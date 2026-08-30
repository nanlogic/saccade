# Changelog

## 0.2.0

- Replaced the compiled runtime and all platform-specific installation paths
  with a platform-independent Node Broker and stdio MCP adapter.
- Moved Extension transport to authenticated-origin loopback HTTP with bounded
  command queues, connection epochs, full reset after reconnect, and no action
  replay after ambiguous dispatch.
- Added per-MCP-session exclusive tab leases, orphan handling, exact-tab full or
  delta reads, local semantic waits, and action-plus-verification receipts.
- Removed Cargo, OS input backends, binary signing, platform installers, and
  platform artifact release jobs.
- Kept the MV3 command channel alive with a bounded two-second local heartbeat,
  removed expired long-poll waiters before dispatch, and made the Extension own
  exactly one generation-checked command loop per Broker connection.

## Historical previews

- Added a gated Windows x64 candidate path for 0.1.2: one headless Runtime,
  current-user Chrome and Edge Native Messaging registration, MCP/client setup,
  exact-checksum rollback, and a Windows Actions install/MCP/uninstall smoke.
  The candidate remains unpublished until a real Windows machine passes and
  the Runtime has a verified Authenticode signature.
- Moved the public setup package to the Nanlogic-owned
  `@nanlogic/saccade@0.1.1` name after the unavailable `@saccade` npm scope
  blocked the 0.1.0 bootstrap publish. Runtime and Extension behavior are
  unchanged.
- Promoted the store Extension source to the production `Saccade` name and
  version `0.3.24`; local development installs now derive a separately
  content-addressed development candidate so they cannot silently switch from
  the development Native Host to the production Host.
- Added Nanlogic-owned release automation for signed and notarized Apple
  Silicon and Intel macOS Runtime artifacts, a draft GitHub Release, and
  tokenless npm trusted publishing with provenance. The workflow fails closed
  until the Extension has a production manifest identity and all external
  company credentials and store identifiers exist.
- Corrected software actionability so continuously moving `reflex_target`
  objects use the immediate object-addressed path instead of waiting forever
  for stable geometry. Ordinary controls retain bounded stability, coverage,
  focus, and enablement waits; identity/authority replacement still fails stale
  and reflex success still requires semantic occurrence proof.
- Reduced the Truth MCP fixed control plane to a compact initialize contract
  and per-tool descriptions; Profile behavior now arrives once from
  `system.capabilities`, with deterministic Runtime, Profile, and contract
  identities verified by setup doctor.
- Added bounded Collector-local actionability waiting for transient animation,
  coverage, focus, and enablement, while preserving the immediate fast path and
  returning machine-readable prepare/dispatch/verify failures.
- Added oracle-generated 1/5/10/25/50 review queues and separate benchmark
  accounting for control plane, discovery, steady state, model cache usage,
  stability, and infrastructure failures.
- Surface a bounded pair of already-public semantic transition signals in the
  compact `saccade.act` tool result, so completion proof carried by the action
  is not lost behind a redundant follow-up read.
- Restore a missing local MCP action cursor only from a fresh snapshot of the
  exact tab and exact document; never rebind an alias across navigation or
  object replacement.
- Classify API rate limits alongside overloads, timeouts, and zero-tool runs as
  invalid infrastructure evidence rather than a browser-lane loss.
- Grade generated queues from their independent tool-output oracle rather than
  letting a contradictory model self-report erase objective completion proof;
  preflight every generated fixture over HTTP before spending model calls.
- Simplified `saccade.act` so current Truth semantics are executable directly:
  callers may omit `operation`, Runtime infers the sole advertised
  click/type/select affordance (or the payload-implied type/select action), and
  ambiguous objects fail closed instead of making the model guess. This is
  Runtime-only and adds no Extension Truth bytes or authority.
- Let a bounded semantic query carry `after_revision` as a canonical lower
  bound, eliminating the failed revision-read plus unbounded-query retry after
  a verified action reveals or replaces controls.
- Stop new tabs from inheriting Agent On through an Agent-owned
  `openerTabId`; only `tabs.open`, an exact confirmed claim, or explicit user
  sharing can authorize the new tab.
- Fixed semantic working-set hydration so `min_objects` is an actual completion
  boundary instead of waiting for a quiet page; added bounded `text_any`
  multi-target matching, with one distinct result reserved per matching phrase,
  so noisy early matches cannot truncate later named targets.
- Isolated concurrent MCP task tabs downstream: each session lists, reads, acts
  on, and closes only its own Agent tabs plus explicitly shared tabs.
- Let exact semantic queries match a control through bounded nearby heading
  context already present in canonical Truth, avoiding broad same-role reads on
  pages with many repeated examples.
- Rebase a stale object-addressed action only when Runtime's retained journal
  and the Extension's current opaque authority both prove the target unchanged;
  target changes still fail closed.
- Clear restored tab authority synchronously on the first worker initialization
  of a browser lifetime, so a delayed `runtime.onStartup` event cannot revoke a
  tab opened after the Host is already ready.
- Keep the ordinary-Chrome Native Host suspension effective across repeated dev
  installs, preventing it from stealing the single test Host mid-run.
- Make the public `saccade.act` schema acceptable to Claude and other strict
  tool registries by removing top-level JSON Schema composition while retaining
  all single-versus-batch constraints in Runtime validation.
- Fix root-scoped semantic queries on pages with same-origin child frames by
  selecting the unique frame without a parent instead of requiring the entire
  observation to contain only one frame.

Saccade has no stable release yet. This file records user-visible changes from
the clean public repository.

## Unreleased

### Changed

- send one eager full Snapshot when an authorized document Collector becomes
  ready, then send Extension→Host deltas instead of retransmitting the whole
  page on every revision; materialize one current Truth in Runtime, recover a
  transport gap by requesting a full Snapshot for that exact tab, and allow an
  Agent with a corrupt cache to request a one-tab-only `truth.read` resync;
- let an Agent client obtain Agent On for exactly one tab it created itself,
  through `claim: "arm"` and `claim: "confirm"` modes of the existing
  `saccade.tabs.open` tool: the intent is short-lived, origin-bound, single-use,
  and latches only the first new tab on that origin, the Agent must supply the
  tab identity its own tooling returned, every mismatch fails with one uniform
  message, and the confirmed tab is session-only with `provenance: agent_client`;
  ordinary user tabs stay Agent Off and the public tool count stays at six;
- make Truth delivery an automatic Agent cursor: one full view per document,
  then only revision-bound deltas, with automatic full reset on navigation or
  stream gap; remove public `view_mode` selection and routine repeated-full
  requests while retaining explicit recovery for one exact tab;
- return additional post-action page changes inline from `saccade.act`, advance
  the same Agent cursor, and replace Runtime's bounded full-snapshot history
  with one current observation plus a compact 256-entry change journal;
- add bounded semantic working-set queries to the existing `truth.read` tool,
  keeping complete canonical Truth local while selecting at most 32 objects by
  label, role, affordance, visibility, and frame scope; split action-relevant
  transitions from queued ambient page/frame churn without losing either;
- make semantic queries hydration-aware, include rendered offscreen targets by
  default, clear already-folded ambient pages on a new working set, and verify
  select opening through `expanded` before querying a named option;
- make verified action receipts suppress unrelated structural churn, return a
  batch `next_basis_revision`, and avoid duplicating Profile behavior in every
  capabilities response;
- let the existing sixth public tool execute one prevalidated batch of
  independent ordinary form edits with per-step semantic verification and one
  final delta, while keeping submit, navigation, upload, and arbitrary controls
  outside the batch;
- isolate fair comparisons from user-local learned input history through an
  explicit evidence-stamped benchmark override that never edits the policy;
- harden the same-model benchmark with real-time JSONL event stamps, explicit
  Claude authentication failures, archived prior attempts, and nonzero matrix
  exits whenever any run is not valid and passing;
- project resolved, bounded HTTP(S) navigation targets for Truth links, require
  source-page reading before verified recommendations, and retain useful
  supporting result tabs while cleaning up temporary search tabs;
- converge Extension candidate `0.3.22`, add finite macOS zero-window browser
  wake through the Extension's own validated popup surface, and prove two cold
  open → Truth → close cycles without adding webpage execution or fallback;
- add ownership-aware `saccade.tabs.close` cleanup for Agent-owned temporary
  tabs, expose `agent` versus `user_shared` in `tabs.list`, and advance public
  capabilities to `saccade.capabilities/6`; preserve the value-free ownership
  ACL across Extension Reload/update and clear it on browser startup;
- define closed-loop evidence as an Agent-owned same-tab action followed by a
  Saccade-observed delta; reclassify Reference Actuator public-page runs as
  observation diagnostics rather than product dogfood or a release gate;
- refresh an obsolete ordinary-Chrome Saccade Native Host during `dev.sh attach`
  without restarting Chrome or touching the Agent client's execution extension;
- add an 11-scenario page-driven lifecycle gate for Chrome and Edge, including
  deterministic slow HTTP resources, large replacement, modal, infinite-list,
  sortable-table, viewport, upload/download, and drag/drop Truth evidence;
- set the first-release distribution target to the browser-store Extension plus
  `npx -y @saccade/setup`, with a headless local MCP and Native Host configured
  for supported Codex and Claude clients;
- implement the dependency-free setup CLI with checksum-gated Runtime install,
  user-level Chrome and Edge manifests, additive Codex and Claude MCP setup,
  doctor, update, rollback, Profile preservation, and safe uninstall;
- move the Accessibility permission helper out of the default Runtime command
  namespace and rename it `reference-actuator-repair`;
- add a default-MCP public Truth probe with test-only Reference Actuator
  stimulus, five official source families, explicit pass/blocked/fail outcomes,
  browser versions, and value-free evidence;
- fingerprint dirty local candidates and record commit, Runtime, Extension,
  Chrome, and Edge versions before the clean-profile two-browser Truth gate;
- keep latency and structural fixtures geometry-stable so single-object and
  iframe gates measure the declared scenario after coordinates became public
  Truth;
- add a publishable architecture overview that explains the Extension compiler,
  stable objects with current geometry, Agent-selected delivery modes, local
  fast-reaction pattern, and protected-content boundary;
- let Agents select `live` or `economy` Truth delivery per read, keeping live
  immediate while economy coalesces a bounded 150 ms revision burst into one
  latest truthful delta without filtering objects or geometry;
- recover dynamic Reference Reflex preparation races within a 45 ms budget and
  keep semantic soft clicks separate from native physical hit-testing, reaching
  96/96 targets with zero failures on Mouse Accuracy `Insane + Tiny`;
- expose current `document_bounds` and `viewport_bounds` for every projected
  object, treat movement and resizing as first-class Truth deltas, and keep
  animated/transitioning object geometry fresh through frame-bounded local
  tracking without exposing action authority;
- keep the client-owned MCP process alive when the Native Host is temporarily
  absent, and reconnect bounded Host calls across recreated sockets and rotated
  owner capabilities without weakening permission or protocol failures;
- instruct Agent clients to plan once per page, consecutively execute already
  determined reversible operations, and verify them with one revision-bounded
  delta instead of rereading full Truth between fields;
- make autonomous completion the default Profile behavior, require known URLs
  to open as automatically Agent-On tabs, keep existing Agent-Off tabs private,
  and remove Saccade-side safety/action policy from MCP;
- keep the Extension as the only product content gate: protect password, SSN,
  and EIN fields and mask formatted SSN/EIN text before observation emission;
- distinguish editable placeholders from current values in Agent Truth, recover
  impossible future revision bases with an immediate full gap reset, and guide
  clients to fold deltas plus verify exact custom-control selections;
- project authored `aria-live` regions and otherwise unmarked leaf text inside
  visible dialogs, so dynamic success and failure confirmations produce Truth
  deltas without site-specific selectors;
- hard-cut the default MCP surface to the five-tool Truth API, advance
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
- human-only managed Profile selection with the bundled smart-barbarian-ceo;
  the removed development name smart-barbarian-eco now has an explicit CLI
  migration alias to that Profile;
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
- same-candidate Chrome and Edge release evidence, the published and verified
  `@saccade/setup` package, and browser-store Extension builds.
