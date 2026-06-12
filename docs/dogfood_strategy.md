# Saccade Dogfood Strategy

Date: 2026-06-11

## Rule

Use Saccade as the default browser layer for Saccade development as soon as a task can run through it.

Chrome and Playwright become comparison tools, compatibility adapters, or escape hatches. They should not define the product shape.

## Why

Saccade should replace the workflows we currently reach for Chrome or Playwright to handle:

- inspect a local app the agent just wrote,
- find UI breakage the agent cannot see from source,
- click and verify local pages,
- fill long forms with replay,
- handle login without exposing passwords,
- tell the user when a field or action needs confirmation,
- produce artifacts a second agent can audit.

If we do not dogfood these paths, Saccade will drift back into a benchmark harness.

## Dogfood Tracks

### 1. Development

Default target: local apps and fixtures.

Use Saccade to answer:

- Did the page render?
- Is the primary action visible and clickable?
- Did any console or network errors happen?
- Is text clipped, invisible, covered, or offscreen?
- Did the form validate and produce the expected receipt?

N2 and DEVMAX should make this usable from Codex/Cursor-style loops.

### 2. Forms

Default target: local FORMMAX fixture.

Use Saccade to:

- discover fields,
- scroll long tables,
- fill non-sensitive values,
- block sensitive values,
- submit local fixtures,
- verify receipts,
- replay every field action.

Saccade must say "user confirmation required" before filling tax IDs, signatures, legal attestations, passwords, OTP, payment fields, or destructive controls.

### 3. Web Research

Default target: pages where current AI/browser tools lose visual state.

Saccade should help when:

- DOM is noisy or stale,
- visible content differs from source,
- screenshots alone do not expose actionability,
- login handoff is needed,
- the user wants a verified action trail.

No CAPTCHA bypass, anti-detection work, spam automation, or bulk posting.

### 4. Login Handoff

Default target: local login fixture, then owned dev apps.

Saccade should let the human log in inside a Human tab, then let the Agent continue in an Agent tab without seeing credentials.

Trusted Tabs must stay first-class:

- every action has `tab_id`,
- every truth read has `tab_id`,
- Human tabs deny Agent input,
- Human tabs deny Agent truth unless granted,
- user takeover pauses or converts Agent tabs.

## What Counts As Dogfood

A feature counts only when it has:

- a local fixture or owned-app target,
- a command we run during development,
- a JSON report,
- replay or artifact paths,
- a documented failure mode,
- a short note in `docs/decisions.md` when behavior changes.

## Near-Term Dogfood Gates

```text
N1: cargo run -q -p saccade-shell -- selftest-tabs
N1B: cargo run -q -p saccade-shell -- selftest-login-handoff
N2: cargo run -q -p devmax -- selftest-fixtures
N3: cargo run -q -p saccade-mcp -- selftest
N4: cargo run -q -p formmax -- run --fixture test_pages/formmax/index.html --replay
```

## Default Tool Policy

Use Saccade first for:

- local page inspection,
- local form filling,
- login handoff experiments,
- replay-backed browser actions,
- artifact validation.

Use Chrome/Playwright for:

- compatibility comparison,
- sites Servo cannot render,
- DEVMAX Chrome adapter,
- baseline benchmarks.

Do not let Playwright locator semantics become the main Saccade API. Saccade's API should return browser truth, action maps, fill transactions, policy gates, and verified results.

## Immediate Next Step

Move DEVMAX from static fixture markers toward browser-backed truth:

```text
Open local app or fixture.
Collect rendered truth and action map from the browser boundary.
Add browser-side console/network capture.
Expand click verification from one action to multi-action smoke flows.
Keep report JSON and replay artifacts stable.
```
