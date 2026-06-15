# ServoShell Runtime Matrix

Date: 2026-06-14

## Purpose

Measure whether the local game slowdown is an engine problem, a build-profile
problem, or a headless/runtime problem.

Tool:

```text
scripts/measure_servoshell_game_runtime.js
```

Metric:

```text
time_scale = game_time_delta / wall_time_delta
```

The script samples the local game's public `canvas.dataset.debug` state through
WebDriver. It does not capture screenshots and does not click through WebDriver.

## Results

| Runtime | Mode | Version | Report | time_scale | Notes |
|---|---:|---|---|---:|---|
| Official Servo.app | headed | `0.3.0-302457869` | `runs/servoshell_runtime/official_headed_1781490600/report.json` | 0.889 | Wayne also observed no severe lag by eye. |
| Source dev raw | headless | source line before release baseline | `runs/servoshell_runtime/source_debug_headless_1781490700/report.json` | 0.089 | Too slow for product/reflex judgment. |
| Source dev raw | headed | source line before release baseline | `runs/servoshell_runtime/source_debug_headed_1781490800/report.json` | 0.038 | Too slow; raw headed can also hit macOS AppKit issues. |
| Source dev partial app | headed | source line before release baseline | `runs/servoshell_runtime/source_debug_partial_app_headed_1781490900/report.json` | 0.000 | Bundle shell alone did not fix debug profile. |
| Source release raw | headed | `0.3.0-805e6a423` | `runs/servoshell_runtime/source_release_headed_1781491100/report.json` | 0.946 | Good enough as a human/runtime baseline. |
| Source release raw | headless | `0.3.0-805e6a423` | `runs/servoshell_runtime/source_release_headless_1781491200/report.json` | 0.969 | Headless is not inherently slow in release. |
| Source release raw + bridge drag | headless | `0.3.0-805e6a423` | `runs/servoshell_runtime/source_release_headless_drag_1781491400/report.json` | 0.999 | Internal drag moved game camera `+20px`. |

## Bridge Timing In Release

Evidence:

```text
runs/reflex_input/release_game_drag_1781491400/frames.jsonl
```

Summary:

```text
frames=180
readback_ok=180/180
readback_ms p50=7.21 p95=13.44 max=19.50
drag_events=9
dispatch_ms p50=0.057 p95=0.979 max=0.979
dropped_logs=0
camera.x delta=+20
```

The readback number is for the full 1280x900 window. MOUSEMAX-level timing still
requires crop-based readback and the detector/motor loop, not full-window debug
capture.

## Packaging Note

`./mach package --dev --preserve-app` created a temporary `.app` bundle but
failed before completion because macOS packaging unconditionally tried to copy
GStreamer dylibs:

```text
AssertionError: gstreamer_root is not None
```

The local game/reflex gate does not need media playback. Product packaging still
needs one of:

- install the official GStreamer dependencies and package normally,
- or add a local packaging mode that skips GStreamer when the build uses dummy
  media.

## Conclusions

1. Servo itself is not the blocker for the local game.
2. Debug builds are not valid for Saccade product/reflex performance judgment.
3. Release source builds match the downloaded official app closely enough to
   continue the in-process Saccade bridge route.
4. The GL warning still appears in successful official/release runs, so the
   warning alone is not a failure signal. Treat it as a backlog/perf warning
   unless paired with measured slowdown.
5. Next reflex work should use
   `/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell`
   as the default ServoShell binary.
