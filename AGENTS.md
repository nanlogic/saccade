# Saccade contributor instructions

Read `docs/current/product-execution-boundary.md`,
`docs/current/saccade-0-2-0-runtime-contract.md`, and
`docs/current/truth-observation-contract.md` before changing browser, Broker,
protocol, MCP, control-module, input, download, or packaging behavior. Read
`docs/current/profile-boundary.md` before changing Profile loading or filtering.

## Permanent product north star

Saccade is a live semantic Truth Layer for the web. Its Extension continuously
compiles an authorized page into structured objects and browser-pushed deltas
for any Agent. Registry-approved `saccade.act` owns bounded object-addressed
software execution. Every core change must preserve fast interaction, low
model-token cost, easy maintenance, trustworthy observation, and model
independence. Do not turn Saccade into a browser-testing framework,
coordinate clicker, or model-specific plugin.

## One production route

Chrome/Edge Extension → loopback Node Broker → MCP adapter. The npm package and
Extension are the complete product. Do not add a compiled runtime, platform
driver, CEF, Servo, Playwright, CDP, visual-coordinate, or other fallback route.

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
- Keep observation schema `saccade.observation/1`; Broker transport uses
  `saccade.node-broker/1`.
- Ship one browser-store Extension plus `npx -y @nanlogic/saccade`. Setup only
  configures the Node MCP command and Profile; it installs no binaries or OS
  registrations.
- Every supported control has truthful recognition, stable identity, bounded
  state, affordances, and browser-pushed changes. Registry-approved,
  object-addressed `saccade.act` software execution is the only product action
  route when the affordance is supported.
- Agents receive current document- and viewport-relative bounds for every
  projected object, with geometry changes pushed under the same stable
  identity. They never receive locators, DOM paths, editable values, protected
  values, cookies, browser storage, or authority to issue arbitrary-coordinate
  actions.
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
  must publish matching revisions of the current product, Broker, and Truth
  authority topics in the same review.
- Add one focused fixture and Truth projection/delta test for each control behavior.
- Run the narrowest checks while editing. Run the complete list from
  `README.md` before merging a control family or changing a contract.
- Keep local browser profiles, evidence, credentials, signing material, and
  protected values out of Git.
