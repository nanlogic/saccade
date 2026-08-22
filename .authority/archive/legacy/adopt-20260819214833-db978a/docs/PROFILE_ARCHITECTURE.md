# Profile architecture

Status: normative for the Truth Layer product.

A Profile tells the Agent how to behave and hides named controls from the
Agent. It never changes how the Extension recognizes an object, derives its
identity, projects safe state and affordances, or computes semantic deltas.

The public schema is
[`catalog/profile.schema.json`](../catalog/profile.schema.json). A Profile has
three fields:

```json
{
  "name": "cautious",
  "behavior": "Explain consequential actions before acting.",
  "ban": [
    {
      "control": "Delete account"
    },
    {
      "control": "Continue",
      "condition": "payment"
    }
  ]
}
```

## Fields

- `name` identifies the Profile for the user and Agent.
- `behavior` is user-authored text supplied to the Agent through
  `saccade.system.capabilities`.
- `ban` lists controls that the Runtime must hide from the Agent.

Each ban entry requires `control`, the control's semantic name. An entry may
also contain `condition`, text associated with that control.

## Matching

The Runtime compares `control` with the full semantic control name. It folds
case, trims surrounding whitespace, and collapses whitespace runs. A rule
without `condition` bans every matching control.

For a rule with `condition`, the Runtime applies the same text normalization
and searches the control's associated text. The current v1 associated text is
the semantic name plus description. Labels already incorporated into the
semantic name therefore participate in the match. A future observation version
may add an explicit association without changing the Profile shape.

If any rule matches, the Runtime bans the control.

## Ban effect

The Native Host applies the active Profile before it caches an Extension
observation. It removes a banned control, its change entries, and any
limitation that refers only to that object. MCP never receives the control.

Ban affects Agent access. It does not remove the page control, prevent human
or Agent-client input, alter the Truth inventory, or change the Extension's
recognition and projection semantics.

## Loading and Agent behavior

The Native Host reads `profile.json` from its Runtime directory at startup. If
the file is absent, it uses [`profiles/default.json`](../profiles/default.json):

```json
{
  "name": "default",
  "behavior": "Continue autonomously until the goal is complete or the Agent client's own policy requires human input. Open known URLs as Agent-owned Agent-On tabs. Saccade MCP adds no safety taxonomy or action gate.",
  "ban": []
}
```

Autonomous completion is therefore the product default, not an expert-only
Profile. Saccade MCP does not classify user data or actions as safe, sensitive,
consequential, or requiring confirmation. Those decisions belong to the Agent
client and its LLM policy. A custom Profile may change behavior or add control
bans, but the default product adds no MCP safety gate.

The shipped default behavior also treats Saccade as the primary automatic
route for browser navigation, page reading, downloads, and web research. An
Agent client must discover Saccade when its MCP tools live in a deferred or
lazy registry; an initially collapsed tool list is not absence. A registered
timeout is unhealthy Saccade. After one retry and same-route reconnect, the
Agent stops the browser task instead of silently falling back to generic web
search or a different browser.

The Extension retains the only product-enforced content redaction: password,
SSN, and EIN fields are marked protected, and SSN/EIN-shaped text values are
masked before an observation is emitted. This is observation hygiene at the
browser boundary, not an MCP decision policy.

The Runtime returns the active Profile's `name` and `behavior` from
`saccade.system.capabilities` using `saccade.capabilities/6`. The first
capabilities call delivers the behavior once with
`behavior_delivery: "capabilities_once"` and a `profile_digest`; initialize
contains only the compact route/loop invariant. The ban list is never exposed.

The invariant MCP instructions also define the low-round-trip observation
pattern: make one automatic initial Truth read. A bounded page arrives as a
full view; an oversized page arrives as a complete stable-ID catalog, followed
by one detail request for only task-relevant identities. The Agent then performs
already-determined reversible operations and folds revision-bounded deltas or
`saccade.act` transitions. It does not repeat the initial read, fetch every
catalog detail, or resync merely because a catalog was returned. The Agent
replans only after a failed operation, stale detail basis, material page
boundary, or delta that invalidates its plan. This reduces model/tool round
trips without weakening semantic verification.

An empty authorized-tab list is not a user task when the target HTTP(S) URL is
known. The Agent must call `saccade.tabs.open`, which creates an Agent-owned tab
that is Agent On automatically. It must not ask the user to open the page,
refresh the Extension, or toggle Agent On. Existing Agent-Off tabs remain
unreadable unless the user explicitly shares that exact tab.

At task completion, the Agent closes Agent-owned tabs used only for temporary
research through `saccade.tabs.close`. It keeps user-facing result pages,
unfinished work, tabs the user requested to retain, and every `user_shared`
tab. This behavior uses the Extension's ownership classification; Profiles do
not gain a separate tab heuristic or timer.

Profile fields do not enter `saccade.observation/1` or
`saccade-extension-host/1`. Both wire schemas keep their current meanings.

The managed development environment provides a human-only Profile entry point:

```sh
./scripts/dev.sh profile set smart-barbarian-ceo
./scripts/dev.sh profile show
./scripts/dev.sh profile reset
```

`set` validates the same three-field shape, writes `profile.json` atomically,
and restarts the managed browser Host. A new MCP connection then loads the
selected Profile. Saccade does not expose Profile mutation as an Agent tool.
The former development-only name `smart-barbarian-eco` is retired. During the
Preview migration, the development CLI resolves that exact legacy name to
`smart-barbarian-ceo`; new documentation and installed defaults use only the
CEO name. This compatibility alias does not create a second Profile or change
Profile filtering semantics.
