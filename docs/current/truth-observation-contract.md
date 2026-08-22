---
authority-topic: truth-observation-contract
authority-scope: .
authority-owner: human-owner
authority-revision: 1
---

# Browser Truth observation contract

- R-001 — The Extension compiles each authorized tab into one canonical full semantic view and then pushes meaningful revision-bounded deltas.
- R-002 — Objects have stable document-local identity, bounded semantic state, affordances, current document- and viewport-relative CSS-pixel geometry, and explicit limitations.
- R-003 — Geometry changes update the same object identity; document replacement creates a new document boundary and invalidates stale authority.
- R-004 — Editable values, protected values, cookies, storage, DOM paths, and locators are never exposed as Truth.
- R-005 — Same-origin frames and open Shadow DOM may be composed; restricted frames and arbitrary Canvas or WebGL remain explicitly opaque unless an approved semantic bridge supplies revalidatable objects.
- R-006 — Stream gaps and impossible revisions force an exact-tab reset rather than guessed state.
