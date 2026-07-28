# Profile architecture

Status: accepted and implemented for the first Runtime slice.

A Profile tells the Agent how to behave and hides named controls from the
Agent. It never changes how a control works. Every supported control keeps the
same observe, prepare, revalidate, native execute, reobserve, and verify loop.

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
observation. It removes a banned control, its change entries, and any limitation
that refers only to that object. MCP never receives the control or its action
token.

The Host accepts an action token only when the token remains in the current
Profile-filtered observation. A token observed before a Host restart or Profile
change cannot bypass a ban.

Ban affects Agent access. It does not remove the page control, prevent human
input, alter the Control Catalog, or weaken the control module's closed loop.

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
`saccade.system.capabilities` using `saccade.capabilities/4`. MCP reads those
fields during initialization and places them in its Agent instructions. It does
not reveal the ban list.

Profile fields do not enter `saccade.observation/1` or
`saccade-extension-host/1`. Both wire schemas keep their current meanings.
