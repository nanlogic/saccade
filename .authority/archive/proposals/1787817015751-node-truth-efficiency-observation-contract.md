---
authority-topic: truth-observation-contract
authority-scope: .
authority-owner: human-owner
authority-revision: 2
---

# Browser Truth observation contract

- R-001 — The Extension compiles each authorized tab into one canonical full semantic view and then pushes meaningful revision-bounded deltas to the loopback Node Broker.
- R-002 — Objects have stable document-local identity, bounded semantic state, affordances, current document- and viewport-relative CSS-pixel geometry, and explicit limitations.
- R-003 — Geometry changes update the same object identity; document replacement creates a new document boundary and invalidates stale authority. Replacement never rebinds an old object or action token.
- R-004 — Editable values, protected values, cookies, storage, DOM paths, locators, screenshots, and arbitrary JavaScript are never exposed as Truth.
- R-005 — Same-origin frames and open Shadow DOM may be composed; restricted frames and arbitrary Canvas or WebGL remain explicitly opaque unless an approved semantic bridge supplies revalidatable objects.
- R-006 — Stream gaps and impossible revisions force an exact-tab reset rather than guessed state. Delta requests never silently become full reads.
- R-007 — The Broker keeps one canonical current Truth per exact tab. Full reads return a bounded full view or complete compact catalog; delta reads wait locally for pushed change and return only provably continuous changes.
- R-008 — Semantic queries return a bounded working set or bounded candidates and never become selectors or execution authority.
- R-009 — Action receipts may carry value-free relevant deltas and verified semantic postconditions so an Agent does not need a redundant follow-up read.
- R-010 — `min_objects` is an explicit bounded hydration condition evaluated against canonical current Truth. It waits on Extension-pushed revisions and never causes model polling or fixed sleep.
- R-011 — Working-set projection scopes related authority and change collections to returned identities while preserving complete canonical Truth locally.
- R-012 — A form batch performs a complete preflight before dispatch, then revalidates every exact current token immediately before its step. Framework rerender may preserve the same live object authority; DOM replacement remains stale and is never rebound.
