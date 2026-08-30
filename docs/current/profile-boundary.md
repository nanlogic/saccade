---
authority-topic: profile-boundary
authority-scope: .
authority-owner: human-owner
authority-revision: 2
---

# Profile architecture

- R-001 — Profiles are strict Node Broker inputs that contain Agent-facing behavior and bounded filtering policy.
- R-002 — Profile filtering occurs after canonical control recognition and cannot change a control module's recognition or projection semantics.
- R-003 — A filtered control and its action authority are both absent from the Agent projection.
- R-004 — Profile policy cannot reveal editable values, protected values, cookies, browser storage, locators, or arbitrary execution authority.
- R-005 — Profile boundary changes update the normative architecture, observation contract, and decisions together.
- R-006 — Setup creates the default Profile only when absent; update and ordinary uninstall preserve user customization.
