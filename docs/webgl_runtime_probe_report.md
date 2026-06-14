# WebGL Runtime Probe Report

Date: 2026-06-14

## Goal

Move BP-011 from a vague backlog warning to a repeatable dogfood blocker with concrete evidence.

Saccade must be able to dogfood local games, visualizations, and canvas-heavy frontend work. If WebGL pages are slow or partially missing visual layers, Saccade cannot be the default browser for our own development.

## Live Game Probe

Target:

```text
http://127.0.0.1:4173/
```

Saccade command:

```sh
printf '{"id":1,"method":"audit"}\n{"id":2,"method":"close"}\n' | \
  RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker \
  --url http://127.0.0.1:4173/ \
  --width 1440 --height 900 \
  --rendering-profile servo-modern
```

Result:

```text
UNSUPPORTED (log once): POSSIBLE ISSUE: unit 1 GLD_TEXTURE_INDEX_2D is unloadable and bound to sampler type (Float) - using zero texture because texture unloadable
```

Artifacts:

- Saccade screenshot: `runs/browser_session_worker/worker_1781443202266_10321/audit_completed_rev1.png`
- Saccade replay: `runs/browser_session_worker/worker_1781443202266_10321/replay.jsonl`
- Chrome reference screenshot: `runs/webgl_runtime/chrome_game_reference_1781443202266/chrome_page.png`
- Chrome manifest: `runs/webgl_runtime/chrome_game_reference_1781443202266/chrome_reference_manifest.json`

Observation:

- Chrome shows the gameplay layer: grid, player/cup, strawberries, and projectiles.
- Saccade shows the HUD/title/background but misses the gameplay canvas layer in the captured frame.
- The Saccade run still produces an audit response and screenshot, so this is not a total browser crash. It is a rendering/runtime correctness blocker.

## Minimal Fixture

Added:

```text
test_pages/webgl_runtime/index.html
```

The fixture draws:

- a 2D canvas gradient and circles,
- a WebGL textured quad,
- visible runtime status for 2D, WebGL context, shader, texture upload, `readPixels`, frame count, average frame time, and GL error.

Saccade command:

```sh
(sleep 3; printf '{"id":1,"method":"audit"}\n{"id":2,"method":"close"}\n') | \
  RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker \
  --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/webgl_runtime/index.html \
  --width 1000 --height 760 \
  --rendering-profile servo-modern
```

Result:

```text
UNSUPPORTED (log once): POSSIBLE ISSUE: unit 5 GLD_TEXTURE_INDEX_RECTANGLE is unloadable and bound to sampler type (Float) - using zero texture because texture unloadable
```

Fixture status visible in Saccade screenshot:

```text
canvas2d=ok
webglContext=ok
shader=ok
texture=ok
readPixels=ok_132_204_22
frames=3
avgFrameMs=135.22
lastError=none
```

Artifacts:

- Saccade screenshot: `runs/browser_session_worker/worker_1781443347692_12895/audit_completed_rev1.png`
- Saccade replay: `runs/browser_session_worker/worker_1781443347692_12895/replay.jsonl`

## Interpretation

BP-011 is now a P1 dogfood blocker, not P2 polish.

Current evidence says:

- 2D canvas is healthy on the minimal fixture.
- Simple WebGL can create a context, upload a texture, draw, and read pixels.
- The macOS GL path still emits texture unloadable warnings.
- Frame progress is too slow for game dogfood on the minimal fixture.
- The real local game loses important gameplay visual layers in Saccade while Chrome shows them.

## Next Step

Debug the Saccade/Servo GL runtime path before broad game/canvas dogfood:

1. Add a scripted selftest around `test_pages/webgl_runtime/index.html` that extracts `window.__saccadeWebglRuntime`.
2. Compare Saccade versus Chrome frame count and screenshot pixels after the same wait.
3. Inspect whether the warning is tied to `WindowRenderingContext`, HiDPI scale, `preserveDrawingBuffer`, texture target choice, or WebRender/macOS backend behavior.
4. Keep routing WebGL-heavy product judgement to Chrome/reference until the selftest is green.
