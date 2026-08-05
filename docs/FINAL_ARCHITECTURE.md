# Saccade final architecture

Status: accepted direction, 2026-08-02.

## Permanent product objective

> Saccade is a live semantic Truth Layer for the web. The Extension continuously
> compiles an authorized page, publishes a full semantic view, and then pushes
> meaningful deltas to any Agent. Execution belongs to the Agent client.

Every core change preserves fast interaction, low model-token cost, easy
maintenance and extension, trustworthy observation, and model independence.
Saccade is not a browser-testing framework, coordinate clicker, input backend,
or model-specific plugin.

## Product responsibility boundary

Core Saccade owns page semantics, stable document-local identity, full→delta
compilation, iframe and open Shadow DOM composition, Profile filtering, honest
opaque/restricted boundaries, and observation of the page transition after an
external action.

Core Saccade does not dispatch mouse or keyboard input, run selector scripts,
provide a Playwright or Accessibility fallback, or decide which action an Agent
should take. Soft mouse, native mouse, input policy, closed-loop verification,
and receipts exist only in the optional Reference Actuator.

## The single route

```text
authorized Chrome/Edge tab
  → Extension compiler
  → Native Messaging Host
  → owner-only local IPC
  → MCP adapter
  → Agent
```

The default route transports Truth only. It does not initialize an input
policy, request Accessibility, dispatch mouse or keyboard input, or issue an
execution receipt. There is no Playwright, CDP, embedded-browser, screenshot,
vision, or coordinate fallback.

`tabs.open` creates and authorizes a tab in the managed Chrome/Edge instance.
An Agent may act with its own web-act or computer-use tool only if that tool
controls the same browser instance and tab. A separate embedded browser cannot
be mixed with Saccade truth; clients must report that combination as
incompatible rather than add a fallback route.

## Truth compiler and state

The Extension—not the model or MCP adapter—interprets DOM, ARIA, registered
control semantics, visibility, relationships, open Shadow DOM, and accessible
same-origin frames. It emits:

- one full document view;
- `appeared`, `updated`, and `disappeared` semantic objects;
- document, viewport, and semantic revisions;
- explicit stream gaps and resets;
- observed transition evidence.

Each public object may contain role, accessible name, safe state, affordances,
stable document-local identity, provenance, and limitations. It never contains
a locator, DOM path, arbitrary coordinate, editable value, protected value,
cookie, browser storage, or default action authority. Profile `ban` filtering
happens before the Agent projection; Profile behavior is supplied as
Agent-facing instructions. The three-field boundary is defined by
`PROFILE_ARCHITECTURE.md`.

The Host retains complete current evidence and bounded revision history for
recovery. MCP applies document-scoped aliases and response compaction; it does
not infer page meaning by comparing snapshots. A missing base revision or a
stream discontinuity produces a full reset rather than a fabricated delta.
Canvas and WebGL remain opaque unless an approved application semantic bridge
publishes revalidatable objects.

## Public MCP API

Default MCP exposes exactly:

- `saccade.system.capabilities`
- `saccade.tabs.list`
- `saccade.tabs.open`
- `saccade.truth.read`

Capabilities use `saccade.capabilities/5`, declare `product: truth_layer`,
push/resource support, and `execution_owner: agent_client`. They do not expose
an input backend or Accessibility state.

`truth.read` without `after_revision` returns the current full or next compact
view. With `after_revision`, the Runtime waits locally for a newer revision;
the model does not poll the page. Truth resources use
`saccade://tabs/{tab_id}/truth`; subscribe/unsubscribe and unsolicited
`notifications/resources/updated` carry the same Extension-produced stream.

The wire protocols remain `saccade.observation/1` and
`saccade-extension-host/1`. Optional action-authority fields remain legal on
the internal wire for the Reference Actuator, but the default Agent projection
omits them.

## Truth Catalog and Registry

`catalog/truth_inventory.json` is the canonical public Truth inventory. It
accounts for every protocol role, reusable control variant, structural
boundary, and its conformance gate. `catalog/controls.json` is the narrower
Reference Actuator module catalog; its 15 rows must never be presented as the
total Truth Layer surface. The core Registry owns semantic recognition and
projection consistency. Adding a role or variant must not add site-specific
selectors or execution policy.

The current machine inventory contains 34 protocol roles, 12 reusable
variants, and 6 structural/push boundaries. The 34 roles consist of 15
interactive roles, 17 additional semantic roles, `frame`, and reserved
`unknown`, which is forbidden from Agent output. Date/time/color inputs,
listbox/combobox implementations, and drag/drop reuse existing roles rather
than creating one protocol role per HTML element.

Common controls require same-candidate Chrome and Edge truth evidence before
becoming `publishable`. Fixtures are regression evidence, not proof of public
web compatibility.

## Evidence and comparison boundary

The complete local Chrome and Edge gate proves that the Extension → Host →
Runtime → MCP projection and pushed-delta framework works for the current
inventory. It does not prove universal compatibility with modern websites.

A fair Playwright comparison starts both lanes from the same unknown URL and
natural-language task. The Saccade lane uses Saccade Truth plus the Agent
client's own web-act tool in the same browser tab, never the Reference
Actuator. The Playwright lane uses official Playwright MCP without prepared
scripts or human-supplied selectors. Record completion, discovery time,
initial bytes/tokens, delta latency, re-observation count, stale/replacement
recovery, tool calls, total time, and failures. Click latency alone is not a
product comparison.

## Reference Actuator

Historical execution code is retained as an optional development adapter:

```text
saccade-runtime reference-actuator-mcp
```

It exposes only `saccade.reference.*` tools and is never written into default
Codex or Claude MCP configuration. Its separate catalog owns native primitives,
backend policy, verifier rules, form fill, reflex loops, stale/replay checks,
and receipts. Native permissions and local input policy are loaded lazily only
after an explicit reference action request. Every returned execution artifact
has `reference_actuator` provenance and cannot establish default product
execution capability.

## Installation and verification

`dev.sh up`, `status`, `test`, and `down` exercise the Truth Layer without
Accessibility. `dev.sh test-actuator` explicitly exercises the optional
Reference Actuator and may require native-input permission.

The decisive dogfood loop is:

```text
tabs.open → truth.read/subscribe → Agent-owned web act in the same tab
→ Extension observes the real change → pushed delta → Agent verifies outcome
```

Saccade reports observed transitions, not `AcceptedByOs` or
`AcceptedBySoftware`. Those statuses belong only to Reference Actuator tests.
