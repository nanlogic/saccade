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
  "behavior": "",
  "ban": []
}
```

The Runtime returns the active Profile's `name` and `behavior` from
`saccade.system.capabilities` using `saccade.capabilities/5`. MCP reads those
fields during initialization and places them in its Agent instructions. It does
not reveal the ban list.

Profile fields do not enter `saccade.observation/1` or
`saccade-extension-host/1`. Both wire schemas keep their current meanings.

The managed development environment provides a human-only Profile entry point:

```sh
./scripts/dev.sh profile set smart-barbarian-eco
./scripts/dev.sh profile show
./scripts/dev.sh profile reset
```

`set` validates the same three-field shape, writes `profile.json` atomically,
and restarts the managed browser Host. A new MCP connection then loads the
selected Profile. Saccade does not expose Profile mutation as an Agent tool.
