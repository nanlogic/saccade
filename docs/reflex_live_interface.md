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
