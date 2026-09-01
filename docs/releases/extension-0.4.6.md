# Saccade Extension 0.4.6

This release removes duplicate Truth objects produced by one composed control.

- When a semantic wrapper and its native inner button or link have the same
  frame, role, name, geometry, and composed ancestor relationship, Saccade
  projects the native control once.
- Independent controls with similar labels remain distinct bounded candidates.
- Strict object resolution remains unchanged: Saccade never guesses among
  genuinely separate objects.
