# Browser Fact Stream v0

Date: 2026-06-15

## Purpose

Saccade needs a generic fact interface between the browser and the agent:

```text
browser truth source -> BrowserFact stream -> detector / planner / replay
```

The agent should not need to know whether a fact came from DOM, Servo native
layout, canvas observation, pixels, WebGL diagnostics, or a future extension. It
should receive typed facts such as:

- a new visible node appeared,
- a new actionable control appeared,
- a sensitive field exists but its value is redacted,
- a canvas exists and its observable metadata changed,
- later: a visual object was detected inside a canvas/frame crop.

This is the bridge between product truth and reflex truth. It is not the final
MOUSEMAX hot loop by itself.

## Current Adapter

Implementation:

```text
scripts/lib/browser_fact_stream.js
scripts/probe_browser_fact_stream.js
test_pages/browser_fact_stream/index.html
```

Schema:

```text
saccade.browser_fact.v0
```

Fact envelope:

```json
{
  "kind": "browser_fact",
  "schema": "saccade.browser_fact.v0",
  "seq": 1,
  "t_ms": 123,
  "url": "file:///...",
  "title": "Browser Fact Stream Fixture",
  "fact_type": "actionable_seen",
  "privacy": "safe"
}
```

Current `fact_type` values:

- `node_seen`
- `actionable_seen`
- `sensitive_field_seen`
- `canvas_seen`

Current source:

- observe-only JavaScript installed through the Saccade bridge,
- `MutationObserver` for added nodes, text changes, and relevant attributes,
- initial page scan plus explicit snapshots,
- no raw input/select/textarea values in the fact payload.

Future sources should emit the same schema instead of inventing new controller
APIs. Good next fact types:

- `visual_object_seen`
- `layout_box_seen`
- `validation_message_seen`
- `navigation_state_seen`
- `permission_prompt_seen`
- `download_seen`

## Safety Contract

The stream follows the existing safety truth profile:

- The user sees the real browser state.
- The agent can see that sensitive fields exist.
- The agent can see status such as `requires_user_input`,
  `completed_without_value`, `checked`, or `selected_without_value`.
- The agent does not receive raw human-owned values for password, SSN,
  government ID, credit card, OTP, token, signature, or similar fields.
- Canvas debug values are disabled by default; only debug keys are reported.

This keeps the browser useful without turning screenshots or raw DOM dumps into
the default agent truth channel.

## Verification

Release ServoShell probe:

```sh
node scripts/probe_browser_fact_stream.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --headless \
  --window-size 1024x740 \
  --output-dir runs/browser_fact_stream/facts_release_1781527171
```

Evidence:

```text
runs/browser_fact_stream/facts_release_1781527171/report.json
runs/browser_fact_stream/facts_release_1781527171/facts.jsonl
```

Summary:

```text
ok=true
facts=31
node_seen=16
actionable_seen=7
canvas_seen=2
sensitive_field_seen=6
actionable=17
sensitive=15
redacted=15
forbidden_value_leaks=[]
```

The fixture inserted task content, buttons, SSN, credit-card, password fields,
and an updated canvas. The fact stream observed the changes and did not leak the
fixture's raw sensitive values.

## Product Meaning

For normal websites, this gives the LLM a compact live map:

```text
new node/control appeared -> classify -> decide whether to fill/click/ask user
```

For games and dynamic canvas pages, this is the control-plane half. The next
step is to add a visual detector that emits `visual_object_seen` facts from
frame crops or canvas/native hooks:

```text
frame/canvas source -> object detector -> visual_object_seen -> motor/replay
```

That lets Saccade keep one interface while swapping truth providers underneath.
