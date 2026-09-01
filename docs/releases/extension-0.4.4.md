# Saccade Extension 0.4.4

This release fixes local topmost validation for controls composed through open
Shadow DOM and slots.

- Hit-test descendants are now validated through the composed tree instead of
  ordinary light-DOM `contains()`.
- A current YouTube-style custom control is no longer falsely rejected as
  covered merely because its hit target lives inside an open shadow root.
- Strict document, revision, token, visibility, enabledness, geometry, focus,
  and topmost checks remain mandatory before dispatch.
