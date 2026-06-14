# Pointer Input Official Research

Date: 2026-06-14

## Question

Manual clicks in Saccade can miss on macOS Retina even when the agent `act`
path can click the same page. Before inventing a workaround, check the official
coordinate contracts for winit, Servo, and macOS.

## Official Findings

### winit

Official docs for `WindowEvent::CursorMoved` say the event carries:

```text
position: PhysicalPosition<f64>
```

and that the coordinates are pixels relative to the top-left of the window.
The DPI docs define physical positions as actual device pixels and logical
positions as physical pixels divided by the window scale factor. `PhysicalPosition`
also exposes `to_logical(scale_factor)`.

Sources:

- https://docs.rs/winit/latest/winit/event/enum.WindowEvent.html
- https://docs.rs/winit/latest/winit/dpi/index.html
- https://docs.rs/winit/latest/winit/dpi/struct.PhysicalPosition.html

### Servo

Servo's `WebViewPoint` supports two coordinate systems:

```text
Device(Point2D<f32, DevicePixel>)
Page(Point2D<f32, CSSPixel>)
```

Servo's docs say page pixels are CSS pixels and account for device scale, page
zoom, and pinch zoom. Servo also exposes `convert_rect_to_css_pixel`, confirming
that Device-to-CSS conversion is part of the intended embedder contract.

Sources:

- https://doc.servo.org/servo/enum.WebViewPoint.html
- https://doc.servo.org/servo/fn.convert_rect_to_css_pixel.html

### macOS AppKit

Apple's event docs point embedders at view-relative conversion when using
native AppKit mouse events: `NSEvent.locationInWindow` plus `NSView.convert`.
This is useful as a platform fallback if winit delivers stale positions, but it
is not required for the primary Retina unit mismatch because winit already gives
physical window-relative positions plus scale factor support.

Sources:

- https://developer.apple.com/documentation/appkit/nsevent/locationinwindow
- https://developer.apple.com/documentation/appkit/nsview/convert(_:from:)

## Decision

Use the official winit/Servo path first:

1. Keep sending Servo `WebViewPoint::Page`, because the rest of Saccade's truth,
   action-map, and agent click paths are CSS/page-coordinate based.
2. Convert winit `PhysicalPosition` to logical/page coordinates at the
   `CursorMoved` boundary with `position.to_logical(window.scale_factor())`.
3. Keep the pointer trace switch so future dogfood can prove
   `raw_physical=(440,486)` becomes `stored_page=(220,243)` on a `2.0` scale
   display.

Do not use `WebViewPoint::Device` as the first fix. It is a valid Servo API, but
our previous live path and replay evidence are CSS/page based, and the failed
trace was specifically caused by passing physical coordinates as `Page`.

## Remaining Fallback

`MouseInput` has no position field in winit, so Saccade still reuses the last
`CursorMoved` position. If misses remain after the unit fix, add a separate
stale-cursor guard or a macOS-only AppKit fallback that reads the current event
or mouse location and converts it through the NSView.
