---
authority-topic: product-execution-boundary
authority-scope: .
authority-owner: human-owner
---

# Actionability follows the execution primitive

## Policy

| Route | Local prerequisites | Locally rebasable | Hard stale |
| --- | --- | --- | --- |
| ordinary software click | visible, enabled, topmost, browser focused; stable geometry when animated | geometry-only and unrelated same-document revisions | document, object, role, affordance, protected state, or token replacement |
| software type | visible, enabled, focused, current editable authority | geometry-only and unrelated same-document revisions | protected/readonly state or authority replacement |
| software select | visible, enabled, focused, current option ownership | geometry-only and unrelated same-document revisions | target/option ownership or authority replacement |
| software `reflex_target` click | exact current object, visible, enabled, click affordance, current token | continuous geometry and unrelated same-document revisions | document, object, role, affordance, loop class, protected state, or token replacement |
| Agent-client/native pointer fallback | client-owned physical focus, hit testing, and current coordinate mapping | client-defined | cannot prove same browser/tab/object |

A software `reflex_target` action dispatches to the exact authorized DOM object,
not to a physical coordinate. It therefore does not wait for stable geometry,
browser focus, or topmost coordinate hit-testing. Success still requires pushed
Truth to prove advancement of the same loop class's `reflex_occurrence`.

This adopts Playwright's local revalidation and bounded retry discipline without
copying selector rebinding. The same object may move; a replacement object is
never silently accepted.

## Acceptance

- A continuously moving reflex target completes 100 verified software actions
  with zero geometry-only prepare failures and zero model rereads.
- Settling animation, overlay, delayed enablement, and replacement retain their
  existing ordinary-control behavior.
- Document, object, role, affordance, protected state, loop class, and token
  replacement remain stale.
- No selector, XPath, CDP, screenshot, coordinate input, or Playwright runtime
  enters the product route.
