# Pointer Input Diagnostic Report

Date: 2026-06-14

## Summary

Saccade's agent `act` path can click real Next.js pages, but the user-visible manual mouse path is currently not trustworthy on this macOS Retina session.

Root cause evidence points to a coordinate unit mismatch:

- winit `WindowEvent::CursorMoved.position` is `PhysicalPosition`.
- Saccade stores that raw physical position in `cursor_x/cursor_y`.
- Saccade then sends it to Servo as `WebViewPoint::Page`, whose units are CSS pixels.
- This machine reports `hidpi_scale_factor=2.0`, so manual clicks can land about 2x away from the intended page target.

## New Diagnostic Switch

Set:

```bash
SACCADE_TRACE_POINTER=1
```

This logs pointer events to stderr with prefix `SACCADE_POINTER_TRACE`.

It is implemented for:

- `browser-session-worker`
- `saccade-shell browse` / dogfood browser

The switch is off by default and does not change input behavior.

## Evidence

Local button fixture:

- CSS button center: about `(220, 245)`
- Saccade worker window: `800x600` logical, `1600x1200` device, `hidpi=2.0`
- Real CGEvent click posted near the visible button center.

Artifact:

```text
runs/pointer_trace/trace_cgevent_1781475144/summary.json
```

Relevant trace:

```text
raw_physical=(440.0,486.0)
logical_if_css=(220.0,243.0)
stored_page=(440.0,486.0)
hidpi=2.000
```

The page title stayed `not clicked`. If Saccade had used the logical/CSS point `(220,243)`, the click would have landed inside the button. It instead sent `(440,486)` as a CSS page point.

## Related Observation

`MouseInput` has no position payload in winit 0.30. Saccade currently reuses the last stored cursor position, so a click after focus changes can also use a stale point if no fresh `CursorMoved` was delivered.

## Current Decision

Do not patch behavior yet. Keep this as BP-012 and gather 2-3 traces from manual dogfood windows first, especially on:

- `https://www.nanmesh.ai/`
- `https://www.mysterypartynow.com/`
- a local fixed-button fixture

Likely fix candidates after confirmation:

- Convert winit physical cursor coordinates to CSS/page coordinates before using `WebViewPoint::Page`.
- Or send `WebViewPoint::Device` only if Servo's current embedder path proves it handles device points correctly on this platform.
- Add stale-cursor protection for first click after focus/raise.
