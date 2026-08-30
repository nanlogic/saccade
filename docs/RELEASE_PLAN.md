# Saccade 0.2.0 release plan

Release one platform-independent npm package and one identical Chrome/Edge
Extension candidate.

Required gates:

1. Node Broker and MCP unit/integration tests pass.
2. Extension projection and transport tests pass.
3. The architecture check finds no compiled runtime or platform-specific route.
4. One candidate passes Chrome and Edge: first full, continuous delta,
   reconnect/full reset, open/close, forms batch, replacement stale, moving
   target, MCP restart, and multiple Agent sessions.
5. `npm pack --dry-run` contains only Node source, CLI, Profile, and docs.
6. The Extension package has a current content-addressed candidate identity.

Publishing uses npm provenance. No platform artifacts, binary signatures,
installers, or browser-test fallback are release inputs.
