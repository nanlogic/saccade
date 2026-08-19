# Saccade product and execution boundary

- R-001 — Saccade is a model-independent live semantic Truth Layer for authorized Chrome and Edge tabs.
- R-002 — The production route is Extension → Native Messaging Host → owner-only local IPC → MCP.
- R-003 — Registry-approved, object-addressed `saccade.act` software execution is the preferred bounded action path when the target exposes a supported affordance.
- R-004 — The Agent client's own same-tab execution is the fallback when bounded software execution is unavailable, rejected, or cannot verify the requested transition.
- R-005 — Playwright, CDP, screenshots, selectors, and arbitrary coordinates are not product fallback routes.
- R-006 — The optional Reference Actuator remains development-only and is not a default product dependency.
- R-007 — Software execution never receives arbitrary coordinates, DOM locators, or protected values; it binds the object, document, and revision and must report verified, accepted-but-unverified, or explicit handoff truthfully.
