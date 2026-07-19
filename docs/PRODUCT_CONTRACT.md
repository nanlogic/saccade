# Saccade Product Contract

Status: canonical product definition

Saccade is an engine-neutral truth, action, safety, and human-collaboration
runtime for the browser tab the user can see. Servo, CEF/Chromium, or another
engine is an adapter. Changing the adapter must not change the six guarantees
below.

## 1. Millisecond Browser Loop

The browser emits structured facts, accepts a revision-scoped action, applies
native input, and returns a renderer-observed receipt. Reflex policies may
repeat this loop locally in milliseconds without screenshots, network calls,
or another LLM inference between each action.

"Millisecond" describes the local `fact -> motor -> receipt` lane. Planning by
a remote language model has separate latency and is not advertised as a
millisecond operation.

## 2. Forms And Tables

The agent receives structured fields, controls, tables, ownership, validation,
and page/revision state. It can fill ordinary fields quickly, preserve existing
work, scroll and continue across long or multi-page forms, and verify what the
page accepted. Sensitive, signature, payment, login, and confirmation steps
remain protected.

Once the user authorizes a form task, the default is completion rather than
manual handoff: the agent fills all known ordinary fields itself. Contact email,
company name, ordinary address, URL and similar profile data are ordinary. The
agent asks only when the exact value is unavailable or a materially different
choice is genuinely ambiguous. It does not treat field entry as permission to
click Next, submit, purchase, publish, or cross another user-specified stopping
point. Saccade does not silently save a new personal profile as a side effect.

The product target is broad form and table reliability. A particular engine is
not considered ready until the FORMMAX control, long-table, multi-page,
dropdown, contenteditable, PDF, and sensitive-handoff gates pass on that
adapter.

## 3. Human And Agent Share One Tab

The user sees agent-filled values immediately in the normal browser. The agent
may inspect user-entered non-sensitive values, understand progress, identify a
likely typo or inconsistency, and suggest a correction. It must not silently
overwrite user work.

For protected fields, the agent receives status such as
`requires_user_input` or `completed_without_value`, never the raw value. The
same rule applies after navigation, restart, or login handoff.

## 4. Screenshot Is An Audit Fallback

Structured browser truth is the normal perception path. Screenshot capture is
optional evidence for an unusual visual/layout disagreement or a final visual
cross-check; it is not the reflex detector and is not required for ordinary
reading, forms, or control.

A screenshot is allowed only when policy proves the surface is non-sensitive
or masks protected regions before pixels leave the browser boundary. A
post-capture blur is insufficient. Logged-in or user-filled pages remain on the
no-screenshot path by default.

## 5. Vibe Coding Uses The User's Rendered Reality

Development agents inspect the same granted tab, renderer state, viewport,
layout geometry, action map, and receipts that produced the user's visible
page. They can connect a source/layout change to its actual browser result and
iterate without guessing from a different headless rendering environment.

Saccade must not claim visual agreement from DOM structure alone. When
structured facts conflict with the visible result, the agent reports the
disagreement and may request the guarded screenshot audit from section 4.

## 6. Protected Values Stay Outside Model Context

Saccade never returns passwords, OTPs, CVVs, SSNs, payment-card numbers or
equivalent secret values to an LLM. These values stay out of agent truth,
action labels, logs, replay, reports, screenshots and model context.

Passport numbers, driver's-licence numbers and government document numbers use
a user-confirmed local fill path. The LLM may request the named field and learn
whether the user completed it. The raw value stays inside the browser-owned
path and remains absent from model context and evidence.

Cookies, browser storage, credentials and local capability tokens remain
browser-owned. The agent may receive a protected field's type, ownership,
requirement and completion state.

## 7. The Host Owns Action Policy

Saccade does not decide whether the user or LLM should play a game, redeem a
reward, buy an item, spend money, send a message, publish or submit. The LLM
host applies its own user-intent, risk and confirmation policy.

Saccade enforces the visible tab's Human-controlled Agent On/Off permission. It
binds input to the current action and page revision, blocks stale or invalid
input, protects the values in section 6 and returns a receipt. Saccade does not
add a second product-policy approval layer for ordinary site actions.

## Engine Adapter Gate

An engine adapter is not accepted because it opens or renders a page. It must
pass the relevant chain:

```text
same visible tab
  -> redacted structured fact
  -> revision-scoped action map
  -> native browser input
  -> renderer-observed receipt
  -> verified page-state change
  -> value-free replay
```

Current CEF evidence covers visible top-frame button, link, and DOM-target
pointer reflex; structured ordinary form/table fill; non-sensitive same-tab
inspection; sensitive completion-only truth; guarded screenshot audit; and
value-free replay. See `docs/cef_day4_forms_safety_report.md`. Cross-origin
frame enumeration, PDF forms, and broader custom-control/public-site coverage
remain measured gates and must not be inferred from these local results.

The current v1 integration contract and AI-033 implementation still enforce a
broader Saccade-owned confirmation policy for submit, payment, publication and
other side effects. DECISION_PRODUCT_071 marks that behavior as an
implementation gap against section 7. Tests must continue to describe the
shipping behavior until the contract and code change together.
