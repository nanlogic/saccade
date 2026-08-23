# Saccade contributor instructions

Read `docs/FINAL_ARCHITECTURE.md` and
`docs/extension_observation_contract.md` before changing browser, Host,
protocol, MCP, control-module, input, download, or packaging behavior. Read
`docs/current/profile-boundary.md` before changing Profile loading or filtering.

## Permanent product north star

Saccade is a live semantic Truth Layer for the web. Its Extension continuously
compiles an authorized page into structured objects and browser-pushed deltas
for any Agent. The Agent client's own web-act or computer-use tool owns
execution in the same browser tab; Saccade observes the resulting semantic
transition. Every core change must preserve fast interaction, low model-token
cost, easy maintenance and extension, trustworthy observation, and model
independence. Do not turn Saccade into a browser-testing framework, coordinate
clicker, execution harness, or model-specific plugin.

## One production route

Chrome/Edge Extension → Native Messaging Host mode → owner-only local IPC →
MCP mode. Do not add CEF, Servo, Playwright, CDP, visual-coordinate, or other
fallback execution routes.

## Required Agent browser routing

- Before any web search, navigation, page read, or download, inspect the
  client's deferred/lazy tool registry for a Saccade MCP server and its six
  public tools. Absence from the initially expanded tool list is not evidence
  that Saccade is unavailable.
- When Saccade is registered, call `saccade.system.capabilities` before using
  any other web-capable tool. A registered call that times out means Saccade is
  present but unhealthy; it does not mean the tool is absent.
- On timeout or connection failure, retry once and repair/reconnect the same
  Saccade route. If it remains unhealthy, stop the browser task and report the
  connection blocker. Never silently fall back to generic web search, Codex
  Browser, Chrome control, Safari, Playwright, CDP, or another browser.
- Another browser route may be used only when Wayne explicitly names or
  authorizes it for that task. Research obtained through another route is not
  Saccade dogfood and must never be reported as such.

## Product invariants

- Treat MCP as the current model-independent adapter, not the product identity;
  protocol semantics cannot depend on one LLM, vendor, prompt, or Agent stack.
- Prefer browser-pushed semantic deltas and bounded local loops over repeated
  full-page transfer, model polling, or model replanning.
- Keep wire schemas at `saccade.observation/1` and
  `saccade-extension-host/1` until an explicit version decision lands.
- Ship one browser-store Extension plus `npx -y @nanlogic/saccade`. Setup installs
  the headless local MCP and Native Host for supported local Agent clients. The
  first release has no DMG, visible Runtime app, or Windows Setup.
- Keep Native Host and MCP modes separate in framing, lifecycle, and
  protected-data boundaries even when one executable supplies both.
- Every supported control has truthful recognition, stable identity, bounded
  state, affordances, and browser-pushed changes. Registry-approved,
  object-addressed `saccade.act` software execution is preferred when the
  affordance is supported; the Agent client's own same-tab execution is the
  fallback when that bounded route is unavailable, rejected, or unverified.
- Agents receive current document- and viewport-relative bounds for every
  projected object, with geometry changes pushed under the same stable
  identity. They never receive locators, DOM paths, editable values, protected
  values, cookies, browser storage, or authority to issue arbitrary-coordinate
  actions.
- The optional Reference Actuator may request finite input primitives and
  declarative verification rules. It is not part of the default product.
- Profile filtering stays outside control modules and cannot change their
  recognition or projection semantics.
- Common controls require current Chrome and Edge proof for the same release
  candidate before the Catalog marks them `publishable`.
- Uncommon controls require truthful recognition and explicit limitations.
- Keep arbitrary Canvas/WebGL opaque unless an approved semantic bridge
  supplies revalidatable objects.

## Migration

Use the private `nanlogic/saccade-legacy` archive only as a reviewed source.
Move one approved component at a time according to
`docs/MIGRATION_MANIFEST.md`, preserve its tests, and record its source commit
and path. Do not copy the old tree or its monolithic classifiers.

## Changes and checks

- Keep the Control Catalog machine-readable and regenerate the public coverage
  table after each Catalog change.
- Treat `docs/current/profile-boundary.md` as normative. A Profile boundary change
  must update `docs/FINAL_ARCHITECTURE.md`,
  `docs/extension_observation_contract.md`, and `docs/decisions.md` in the same
  review.
- Add one focused fixture and Truth projection/delta test for each control behavior.
- Run the narrowest checks while editing. Run the complete list from
  `README.md` before merging a control family or changing a contract.
- Keep local browser profiles, evidence, credentials, signing material, and
  protected values out of Git.
