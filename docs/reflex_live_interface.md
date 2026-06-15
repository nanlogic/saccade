# Reflex Live Interface

Date: 2026-06-14

## Purpose

This is the first stable control surface for release ServoShell plus the
Saccade in-process reflex bridge. It is deliberately small:

```text
external controller -> commands.jsonl -> ServoShell bridge -> browser input
external controller <- receipts.jsonl <- ServoShell bridge
external controller <- frames.jsonl   <- ServoShell bridge
```

The controller can be a Node script, Codex worker, MCP server, or future native
daemon. The internal semantics stay the same.

## Saccade Adapter

The reusable Saccade-side adapter lives in:

```text
scripts/lib/reflex_live_bridge.js
```

It owns:

- release ServoShell process launch and cleanup,
- WebDriver session setup for non-hot-loop diagnostics,
- command JSONL append,
- receipt/frame JSONL reading,
- report summaries for receipts, frame readback, and local-game samples.

Unit coverage:

```sh
node --test scripts/lib/reflex_live_bridge.test.js
```

Click-command fixture probe:

```sh
node scripts/probe_reflex_live_click_fixture.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --headless \
  --window-size 1024x740
```

Local game v0 reflex loop:

```sh
node scripts/run_local_game_reflex_loop.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 15000
```

This runner is intentionally labeled `local_game_debug_policy_v0`: it uses the
local game's public `canvas.dataset.debug` and DOM panel visibility as the
temporary detector, then sends real browser input through the ServoShell bridge.
It is useful for release dogfood and game-session testing, but it is not the
final visual detector. It now also records Browser Fact Stream output as
`facts.jsonl`, derived `semantic_facts.jsonl`, and `browser_facts_observed`
replay events.

## Browser Fact Stream

Generic page facts now have a stable adapter contract:

```text
scripts/lib/browser_fact_stream.js
docs/browser_fact_stream.md
```

The stream emits `saccade.browser_fact.v0` records for visible nodes,
actionable controls, sensitive fields, canvas surfaces, and optional
`visual_object_seen` facts. It is intentionally source-neutral: the current
emitter is an observe-only JS adapter with explicit canvas pixel sampling, while
future Servo native layout/canvas/frame hooks should emit the same schema.

Probe:

```sh
node scripts/probe_browser_fact_stream.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --headless \
  --window-size 1024x740 \
  --output-dir runs/browser_fact_stream/facts_visual_1781527623
```

Latest passing run:

```text
runs/browser_fact_stream/facts_visual_1781527623/report.json
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
forbidden_value_leaks=[]
```

MOUSEMAX replay adapter:

```sh
node scripts/convert_mousemax_replay_to_facts.js \
  --replay runs/arena/run_1781294025/replay.jsonl \
  --mode appeared \
  --output-dir runs/browser_fact_stream/mousemax_1781528244
```

Latest passing replay conversion:

```text
runs/browser_fact_stream/mousemax_1781528244/report.json
```

Summary:

```text
ok=true
visual_object_seen=45
facts_match_result_targets_seen=true
skipped_outside_game_area=2
```

Local game live semantic fact evidence:

```text
runs/local_game_reflex/semantic_live_1781529317/report.json
runs/local_game_reflex/semantic_live_1781529317/facts.jsonl
runs/local_game_reflex/semantic_live_1781529317/semantic_facts.jsonl
```

Summary:

```text
ok=true
browser_facts.count=40
visual_object_seen=27
semantic_object_seen=27
time_scale=0.940
dispatch_ms.p95=0.189
```

## Runtime

Use the release ServoShell build for product/runtime evidence:

```text
/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell
```

Bridge env vars:

```text
SACCADE_REFLEX_COMMANDS_PATH=/path/to/commands.jsonl
SACCADE_REFLEX_RECEIPTS_PATH=/path/to/receipts.jsonl
SACCADE_REFLEX_OBSERVE_PATH=/path/to/frames.jsonl
SACCADE_REFLEX_OBSERVE_MAX_FRAMES=420
```

`SACCADE_REFLEX_OBSERVE_PATH` is optional for command/receipt-only runs.

## Commands

Commands are newline-terminated JSON objects. Supported commands:

```json
{"id":"ping-1","type":"ping"}
{"id":"click-1","type":"click","x":640,"y":450}
{"id":"drag-1","type":"drag","start":{"x":640,"y":450},"end":{"x":1000,"y":450},"frames":8}
```

Flat drag points are also accepted:

```json
{"id":"drag-2","type":"drag","start_x":640,"start_y":450,"end_x":1000,"end_y":450,"frames":8}
```

Coordinates are Servo device pixels in the target webview, matching the bridge's
current internal input path.

## Receipts

Receipts are newline-terminated JSON objects with:

- `kind: "saccade_reflex_command_receipt"`
- `id`, `type`, `status`
- `frame_id`, `webview_id`
- URL/title when available
- dispatch timing for click/drag phases
- `dropped_receipts`

Drag emits one `drag:scheduled` receipt and per-frame `drag_phase:dispatched`
receipts.

## Verification

Probe script:

```sh
node scripts/probe_reflex_live_commands.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 6500 \
  --output-dir runs/reflex_live/live_release_<id>
```

Latest passing run:

```text
runs/reflex_live/live_release_1781495324/report.json
```

Summary:

```text
ok=true
time_scale=1.002
camera_delta.x=+21
receipts=11
ping:ok=1
drag:scheduled=1
drag_phase:dispatched=9
drag dispatch ms p50=0.023 p95=0.075 max=0.078
readback_ok=420/420
readback ms p50=2.74 p95=5.19 max=7.41
dropped_logs=0
```

This proves the external control interface can drive the in-process browser
input path in release ServoShell. It does not yet prove detector/motor ownership
or MOUSEMAX-level cropped readback.

Click-command verification:

```text
runs/reflex_live_click/click_release_1781496285/report.json
```

Summary:

```text
ok=true
click=(242,245)
post_revision=1
post_button=Verified
post_status="Agent action verified in the same browser session."
click receipt dispatch_ms=0.196
readback_ok=6/6
dropped_logs=0
```

Adapter refactor verification:

```text
runs/reflex_live/live_adapter_1781496157/report.json
```

Summary:

```text
ok=true
time_scale=0.977
camera_delta.x=+25
receipts=11
ping:ok=1
drag:scheduled=1
drag_phase:dispatched=9
drag dispatch ms p50=0.020 p95=0.039 max=0.404
readback_ok=420/420
readback ms p50=4.06 p95=9.69 max=16.31
dropped_logs=0
```

Local game v0 loop verification:

```text
runs/local_game_reflex/loop_release_1781525581/report.json
runs/local_game_reflex/loop_release_1781525581/replay.jsonl
```

Summary:

```text
ok=true
final_reason=duration_complete
controller=local_game_debug_policy_v0
commands=57
command_receipts=57
drag_phase_receipts=627
readback_ok=1400/1400
dispatch_ms p50=0.022 p95=0.071 max=4.709
readback_ms p50=3.23 p95=8.30 max=17.29
time_scale=0.993
hp_delta=0
camera_delta=+38,+29
fill_delta=0
```
