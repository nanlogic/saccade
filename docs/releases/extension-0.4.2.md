# Saccade Extension 0.4.2

This patch fixes ordinary clicks on framework and custom-element controls.

- Ordinary controls now receive one native DOM activation at the deepest
  current interactive element. Saccade no longer injects a synthetic
  pointer/mouse cascade into wrapper controls.
- Continuous `reflex_target` execution keeps its dedicated pointer-event
  strategy.
- Single actions finish their complete local actionability wait before any
  side effect, then immediately revalidate the same document, object, token,
  affordance, geometry, visibility, enabled state, topmost state, and focus
  before the sole dispatch.
- A preparation timeout is retry-safe. Only a missing response after the
  dispatch boundary is reported as `outcome_unknown`.
- Chrome and Edge conformance now includes an open-Shadow-DOM custom-element
  activation fixture modeled on framework wrapper buttons.
