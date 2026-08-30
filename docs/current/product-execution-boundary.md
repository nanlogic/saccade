---
authority-topic: product-execution-boundary
authority-scope: .
authority-owner: human-owner
authority-revision: 2
---

# Node-only product and execution boundary

- R-001 — Saccade 0.2.0 ships exactly one browser-store Extension and one
  Node.js package, `@nanlogic/saccade`.
- R-002 — The production route is authorized Chrome/Edge tab → Extension →
  loopback Node Broker → MCP adapter → Agent.
- R-003 — Rust, Cargo workspaces, Native Messaging Hosts, owner-IPC drivers,
  platform input drivers, platform-specific Runtime binaries, code signing,
  notarization, DMG, and Windows Setup are removed from the product and build.
- R-004 — The Node Broker is transport and state coordination, not a browser
  driver. It accepts no selectors, arbitrary JavaScript, screenshots, CDP, or
  arbitrary coordinates.
- R-005 — Registry-approved object-addressed software actions execute only in
  the Extension. Unsupported or unverifiable execution is handed explicitly to
  the Agent client's same-tab browser capability and is never retried
  automatically.
- R-006 — The Extension and Broker use one versioned loopback protocol with
  bounded messages, acknowledgements, heartbeats, reconnect epochs, and exact
  tab routing.
- R-007 — Every MCP connection has a fresh `agent_session_id`. A tab has at
  most one active Agent lease, and all Truth/action requests require both the
  session and exact `tab_id`.
- R-008 — Tabs opened by an Agent are leased automatically to that Agent.
  User-shared tabs are assigned explicitly. Lost sessions leave orphaned
  leases; tabs are not closed, transferred, exposed, or replayed automatically.
