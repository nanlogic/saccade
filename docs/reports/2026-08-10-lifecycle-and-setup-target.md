# Lifecycle evidence and setup target

Date: 2026-08-10 America/Chicago.

The page-driven lifecycle matrix passed in Chrome and Edge. It covered all 11
declared scenarios, including a deterministic 1.5-second HTTP response, a
150-object replacement, modal appearance and removal, infinite append, table
reorder with stable identity, and viewport geometry change. It also checked
value-free upload, download-link, and drag/drop Truth representation.

Evidence root:
`~/Library/Application Support/Saccade Dev/evidence/20260811T021920Z`.

The complete clean-profile Truth regression passed after the lifecycle work.
Evidence root:
`~/Library/Application Support/Saccade Dev/evidence/20260811T022037Z`.

The team discarded the DMG and visible Runtime App release experiment. The
first public setup target is the browser-store Extension plus
`npx -y @saccade/setup`. The setup command installs the headless local MCP and
Native Host, configures supported local Codex and Claude clients, and verifies
the connection. `docs/SETUP_TARGET.md` defines the current boundary.
