# Saccade Extension 0.4.7

This release makes local actionability checks follow the browser's flattened
composed tree for slotted Web Component controls.

- A light-DOM control assigned to a Shadow DOM `<slot>` now treats that slot as
  its rendered parent during topmost validation.
- Object authority, current-document checks, and strict resolution remain
  unchanged.
- The Chrome/Edge release smoke includes a real slotted button activation, so
  this behavior is covered as a general Web Components contract rather than a
  site-specific exception.
