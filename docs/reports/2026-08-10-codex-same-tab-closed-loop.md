# Codex same-tab closed-loop evidence

Date: 2026-08-10 America/Chicago.

## Result

Passed one complete client-owned execution loop in ordinary Chrome. Saccade
provided current Truth and geometry, Codex performed the click with its own
Chrome tool in the same tab, and Saccade reported the resulting semantic delta.
The default MCP reported `execution_owner: agent_client` and
`reference_actuator_active: false`.

## Trace

| Step | Evidence |
| --- | --- |
| Shared tab | Chrome and Saccade both reported tab `1680320444` at the same fixture URL. |
| Initial Truth | Revision `3135`; button `o1`, name `Toggle signal`, `pressed: false`. |
| Current geometry | Viewport bounds `x: 8`, `y: 79.875`, `width: 93.8203125`, `height: 21.5`. |
| Agent action | Codex Chrome clicked viewport point `(55, 91)`, inside those Saccade bounds. |
| Observed result | Delta revision `3152`; the same object `o1` changed to `pressed: true`. |
| Additional transition | Status object `o3` updated from `Browser cycle 3132` to `Browser cycle 3148`. |

This is closed-loop evidence because it contains both the external Agent action
and the Extension-produced post-action delta. The earlier managed Chrome/Edge
Truth gates remain observation-path evidence; Reference Actuator diagnostics
remain separate and cannot substitute for this client-owned loop.

## Connection recovery finding

The first attempt timed out because ordinary Chrome retained a Native Host
process from before the clean-profile Chrome/Edge run. Restarting only that
stale Saccade Host process restored the Extension connection; Chrome and the
Codex Chrome execution extension remained running. `dev.sh attach` now refreshes
only processes whose command is the installed Saccade Runtime Native Host, so a
candidate test run cannot leave ordinary Chrome attached to an obsolete Host
session.

The repaired `attach` path was then exercised against the live ordinary-Chrome
session. MCP reconnected with `extension_connected: true`; Codex clicked the
same object again from revision `3244`, and Saccade returned delta revision
`3253` with `pressed` changing from `true` back to `false`. This confirms that
automatic Host refresh preserves the Agent-owned execution loop.
