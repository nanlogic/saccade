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
- a visual object was detected inside a canvas/frame crop.

This is the bridge between product truth and reflex truth. It is not the final
MOUSEMAX hot loop by itself.

## Current Adapter

Implementation:

```text
scripts/lib/browser_fact_stream.js
scripts/probe_browser_fact_stream.js
scripts/convert_mousemax_replay_to_facts.js
scripts/run_local_game_reflex_loop.js
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
- `visual_object_seen`

Current source:

- observe-only JavaScript installed through the Saccade bridge,
- `MutationObserver` for added nodes, text changes, and relevant attributes,
- initial page scan plus explicit snapshots,
- optional canvas pixel component sampling when `allowCanvasPixelRead=true`,
- no raw input/select/textarea values in the fact payload.

Future sources should emit the same schema instead of inventing new controller
APIs. Good next fact types:

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
- Canvas pixel reads are disabled by default and should be enabled only for
  explicit non-sensitive reflex/fixture modes.

This keeps the browser useful without turning screenshots or raw DOM dumps into
the default agent truth channel.

## Verification

Release ServoShell probe:

```sh
node scripts/probe_browser_fact_stream.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --headless \
  --window-size 1024x740 \
  --output-dir runs/browser_fact_stream/facts_visual_1781527623
```

Evidence:

```text
runs/browser_fact_stream/facts_visual_1781527623/report.json
runs/browser_fact_stream/facts_visual_1781527623/facts.jsonl
```

Summary:

```text
ok=true
facts=33
node_seen=16
actionable_seen=7
canvas_seen=2
sensitive_field_seen=6
visual_object_seen=2
actionable=17
sensitive=15
redacted=15
forbidden_value_leaks=[]
```

The fixture inserted task content, buttons, SSN, credit-card, password fields,
and an updated canvas. The fact stream observed the changes, detected two green
canvas objects via `canvas_pixel_probe`, and did not leak the fixture's raw
sensitive values.

The generated `facts.jsonl` is real newline-delimited JSON and is parseable as a
replay/fact artifact.

## Relationship To MOUSEMAX

The original MOUSEMAX dot benchmark did not use this adapter-level fact stream.
It used the lower-level reflex path:

- local arena `observe_only`: synchronized `.target` DOM proxies provided
  layout rectangles as browser-owned evidence,
- real-site pure pixel run: Servo-rendered RGBA pixels were scanned by the red
  connected-component detector,
- both paths sent clicks through Servo input, not DOM `click()` or Playwright.

Browser Fact Stream v0 is the unifying product language on top of those sources.
The next integration step is:

```text
old DOM rect detector / pixel detector / future Servo native hook
  -> visual_object_seen
  -> motor / replay / LLM-visible summary
```

The first bridge is implemented for replay artifacts:

```sh
node scripts/convert_mousemax_replay_to_facts.js \
  --replay runs/arena/run_1781294025/replay.jsonl \
  --mode appeared \
  --output-dir runs/browser_fact_stream/mousemax_1781528244
```

Evidence:

```text
runs/browser_fact_stream/mousemax_1781528244/report.json
runs/browser_fact_stream/mousemax_1781528244/facts.jsonl
```

Summary:

```text
ok=true
visual_object_seen=45
facts_match_result_targets_seen=true
skipped_outside_game_area=2
detector_sources.DomRect=45
```

This proves an existing MOUSEMAX arena replay can be reduced to the same
`visual_object_seen` facts that the new browser fact stream uses. The converter
filters `tracker_appeared` targets through the frame `game_area_css`, matching
the benchmark's `targets_seen` definition instead of blindly exporting every
internal tracker appearance.

## Local Game Live Facts

`scripts/run_local_game_reflex_loop.js` now installs Browser Fact Stream during
the live ServoShell run, writes `facts.jsonl`, and records
`browser_facts_observed` summary events into the replay. Canvas pixel sampling
is throttled by `--visual-fact-interval-ms` so it remains evidence/control-plane
work, not the millisecond motor loop.

Short release ServoShell check:

```sh
node scripts/run_local_game_reflex_loop.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 5000 \
  --visual-fact-interval-ms 1000 \
  --output-dir runs/local_game_reflex/live_facts_1781528515
```

Evidence:

```text
runs/local_game_reflex/live_facts_1781528515/report.json
runs/local_game_reflex/live_facts_1781528515/replay.jsonl
runs/local_game_reflex/live_facts_1781528515/facts.jsonl
```

Summary:

```text
ok=true
browser_facts.count=48
canvas_seen=1
visual_object_seen=35
replay browser_facts_observed events=6
time_scale=0.985
dispatch_ms.p95=0.138
readback_ms.p95=9.032
```

This proves live page changes can be seen through the fact stream while the
bridge is also driving Servo input. The current visual facts are object-like
pixel components, not semantic labels such as fruit/enemy/drop.

## Product Meaning

For normal websites, this gives the LLM a compact live map:

```text
new node/control appeared -> classify -> decide whether to fill/click/ask user
```

For games and dynamic canvas pages, this is now the first generic visual-object
fact path. The current implementation is a fixture-grade canvas pixel component
sampler; the next production step is to emit the same `visual_object_seen` facts
from frame crops or Servo native hooks:

```text
frame/canvas source -> object detector -> visual_object_seen -> motor/replay
```

That lets Saccade keep one interface while swapping truth providers underneath.
