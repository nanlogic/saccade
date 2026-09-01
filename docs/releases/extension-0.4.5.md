# Saccade Extension 0.4.5

This release completes composed-tree topmost validation for browser-retargeted
Shadow DOM controls.

- The current hit may be a composed descendant of the authoritative control,
  or the control may be a composed descendant of its browser-retargeted shadow
  host. Both are the same current hit-test branch.
- Unrelated siblings and overlays still fail topmost validation.
- Parent-frame validation remains strict to the exact frame element; an outer
  overlay is never accepted as action authority.
