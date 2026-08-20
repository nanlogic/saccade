# How Saccade works

Status: public architecture overview for the Developer Preview.

Saccade gives an AI Agent a current semantic view of an authorized Chrome or
Edge tab. The browser Extension compiles the initial page, keeps that view
current, and publishes source-declared changes under stable object identities.
The Agent can respond to the latest page state without rescanning the page or
asking the model to compare screenshots.

The local headless Runtime hosts the Native Messaging connection, current Truth
state, and MCP adapter. The development tree may wrap it in an internal macOS
app for signing tests. That wrapper is not installed for users. MCP is the
interface Codex and Claude use to read Truth; the Runtime does not control the
browser.

## One data path

```text
authorized browser tab
  → Extension compiles semantic objects, geometry, and changes
  → Native Host keeps current Truth and bounded revision history
  → MCP delivers a full view or folded delta
  → Agent plans and acts with its own same-tab tool
  → Extension observes the result and publishes the next revision
```

A closed-loop Saccade test must include the Agent-owned action and the observed
post-action delta. A test that stops after reading Truth proves observation
only, even if every Extension, Host, and MCP check passes.

| Part | Responsibility |
| --- | --- |
| Extension | Interprets DOM, ARIA, registered control semantics, visibility, relationships, and rendered geometry. |
| Native Host | Stores the current compiled state and enough revision history to prove continuity. |
| MCP adapter | Gives the Agent compact full views and deltas. It does not parse the page or choose actions. |
| Agent client | Chooses the task strategy and uses its own browser or computer-use tool in the authorized tab. |

The Extension serves as the sole page parser, and Saccade uses one browser
route. The Host preserves the Extension's output for MCP delivery.

## Stable identity with current geometry

A projected object can include its role, accessible name, safe state,
affordances, limitations, and current bounds. The identity stays stable while
the object moves.

```json
{
  "id": "object_17",
  "role": "button",
  "name": "Continue",
  "affordances": ["click"],
  "viewport_bounds": { "x": 612, "y": 428, "width": 104, "height": 36 }
}
```

Scroll, resize, layout, transition, and animation changes produce updated
geometry on the same identity. The Agent receives the current coordinates as
Truth. Saccade does not expose locators, DOM paths, action tokens, or an API for
arbitrary-coordinate action requests.

## Full view, then pushed changes

The first read returns the current document view. Later reads return
Extension-compiled `appeared`, `updated`, and `disappeared` objects. A revision
number ties each response to a known page state. The Host returns a full reset
when it cannot prove an unbroken revision chain.

The Agent keeps one local view and folds each delta into it. Objects omitted
from a delta remain unchanged. This keeps model input small and removes the
need for repeated full-page reads.

## The Agent chooses delivery speed

Each `saccade.truth.read` call accepts one of two delivery modes.

| Mode | Behavior | Best fit |
| --- | --- | --- |
| `live` | Returns the next browser-pushed revision as soon as it arrives. | Dynamic controls, games, and latency-sensitive work. |
| `economy` | Collects a bounded 150 ms burst and returns the latest folded delta. | Forms, research, and routine browsing with lower model churn. |

Both modes contain the same Truth surface, including current geometry. The
Agent may switch modes per read. Saccade leaves this choice to the Agent rather
than binding it to a model, Profile, or task category.

## Fast reactions stay local

The model chooses the goal and strategy. A latency-sensitive client can then
consume live revisions in a bounded local loop, react to the current object,
and ask the model to review the result after the loop. The model does not need
to process every animation frame.

An Agent client may consume live revisions in a bounded local loop and use its
own browser tool for each action. Saccade remains the observation side of that
loop; it does not become the input side when updates are fast.

## Protected content

The Extension blocks password, SSN, and EIN values before data leaves the page.
It also masks SSN- and EIN-shaped text. Protected objects can still expose safe
state and geometry, so an Agent can understand the layout without reading the
value.

Existing tabs remain private until the user shares them. A tab opened through
`saccade.tabs.open` belongs to the authorized Agent session. MCP forwards the
allowed Truth without adding a data classifier or action policy. The Agent owns
decisions after the Extension boundary.

## Product boundary

Default Saccade exposes five MCP tools:

- `saccade.system.capabilities`
- `saccade.tabs.list`
- `saccade.tabs.open`
- `saccade.tabs.close`
- `saccade.truth.read`

`tabs.list` reports whether an authorized tab is `agent` or `user_shared`.
`tabs.close` is deliberately narrower than a normal browser close command: it
can close only tabs created through `tabs.open`. Agents use it to clean up
temporary research tabs when work is complete, while leaving user-shared tabs,
useful result pages, and unfinished work alone.

The core product does not dispatch input. It does not ship Playwright, CDP,
vision, Accessibility, or an embedded browser as a fallback. An integration
must use an Agent-owned tool that can act in the same authorized browser tab.
Installing or running Saccade does not require macOS Accessibility permission.

The public setup target is the store Extension plus
`npx -y @nanlogic/saccade`. The command installs the local headless route for
supported Codex and Claude clients. Cloud-only sessions cannot reach that local
route and are incompatible with the first release.

The normative details live in the
[final architecture](FINAL_ARCHITECTURE.md) and
[Extension Truth contract](extension_observation_contract.md).
