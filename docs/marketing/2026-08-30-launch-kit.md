# Saccade 0.2.0 launch kit

This file contains drafts. It does not record publication to any external
platform.

## Short Chinese preview

Saccade 0.2.0 让 Agent 在用户授权的 Chrome/Edge 标签页里读取小而当前的语义状态，并执行带验证的精确对象操作。新版本通过了双浏览器表单、iframe、富文本、上传和 Mouse Accuracy 发布门槛。

## Audience and job to be done

### Primary: browser-agent and MCP product teams

These teams need an Agent to work in a real browser session without confusing
tabs, repeating full-page reads, or guessing whether an action took effect.
Saccade gives them exact tab ownership, revision-bounded Truth, delta reads,
and verified action receipts through six MCP tools.

### Primary: teams automating authenticated and legacy web operations

Admin consoles often combine long forms, old rich-text editors, iframes,
uploads, dynamic choices, and separate Save actions. Saccade can preflight
independent fields as one batch while keeping submit, navigation, and upload
explicit. This reduces repeated Agent reads without hiding consequential steps.

### Secondary: agent teams with long-lived or dynamic page work

When a page changes during a task, a stable object identity and browser-pushed
delta are more useful than repeatedly rebuilding a plan from a full snapshot.
The release gate covers replacement-stale handling and an exact moving-target
path in Chrome and Edge.

### Not the primary buyer: conventional end-to-end test teams

Playwright already provides strong locators, actionability checks, assertions,
and clean browser-context isolation for reproducible tests. Saccade is for the
different job of keeping an Agent attached to current, user-authorized browser
state.

## Positioning in one sentence

Saccade is the live semantic Truth and verified-action layer for Agents working
in authorized Chrome and Edge tabs.

## DEV Community draft

### Title

Saccade 0.2.0: live browser Truth for agents, with verified actions

### Body

Browser agents fail in a few recurring ways: they act in the wrong tab, read a
page that has already changed, re-fetch too much state, or click once and then
guess whether it worked.

Saccade 0.2.0 addresses those failures with a small MCP interface for authorized
Chrome and Edge tabs.

Every MCP connection gets its own Agent session. Every browser request names an
exact leased tab. A tab has one writer, so two Agents cannot silently operate it
at the same time.

The Agent can request a bounded full semantic view or a delta after a known
revision. The Broker keeps canonical current Truth while the browser pushes
changes. That lets the Agent read the part it needs instead of transferring the
whole page after every field.

Actions are addressed to a current semantic object. The Extension checks local
actionability under the request deadline, dispatches once, and returns a receipt
with the resulting semantic postcondition. If the object was replaced, its old
authority stays stale. If a side effect may have happened but cannot be proved,
Saccade returns an unknown outcome and does not replay it.

The 0.2.0 candidate adds and verifies the controls we needed for real admin
work: contenteditable and same-origin iframe rich text, native and ARIA choices,
independent form batches, semantic tables, standard uploads, and a bounded path
for fast moving targets.

The same Extension candidate passed the release gate in Chrome and Edge. In the
Mouse Accuracy fixture it completed 24/24 ordinary targets and 24/24 canvas
targets in each browser—96/96 exact-target actions across the run, with mean
action latency from 7.22 ms to 8.50 ms.

Playwright was an important reference, especially for strict resolution,
actionability, assertions, and isolation. It remains the better fit for
reproducible browser tests. Saccade is aimed at a different workflow: an Agent
working in the user's current, authorized browser state.

Our latest controlled same-model form comparison reflects that tradeoff.
Saccade used fewer browser tool calls and less browser-MCP time; Playwright
finished the full Agent task sooner. The repository includes the prompt,
measurements, and limitations rather than reducing the result to one winner.

Saccade 0.2.0 uses six MCP tools and installs with:

```sh
npx -y @nanlogic/saccade install
npx -y @nanlogic/saccade doctor
```

Source, release evidence, and the Mouse Accuracy result:
https://github.com/nanlogic/saccade

## Medium draft

### Title

What a browser agent needs after the click

### Subtitle

Saccade 0.2.0 keeps tab ownership, current page Truth, and action verification in one MCP browser layer.

### Body

Most browser-agent demos focus on choosing the next click. Production failures
often happen around that click: the Agent is attached to the wrong tab, its page
view is stale, the target was replaced, or a save may have happened just before
the connection dropped.

These are state and ownership problems. More model reasoning does not make an
old action token current or prove an ambiguous side effect.

Saccade treats one authorized browser tab as a revisioned semantic document.
Each Agent session owns its tab leases. Reads name one tab and request either a
bounded full view or a delta after a known revision. The Broker keeps canonical
current Truth while the Extension sends page changes.

Actions follow the same rule. The Agent names the current document, basis
revision, and semantic object. The Extension waits locally for the control to be
visible, enabled, stable, and ready to receive the action. The receipt then
returns a compact relevant delta and semantic postcondition. A verified action
does not require another full-page read.

This matters on ordinary but difficult web work. A long administration form may
contain native inputs, an iframe editor, dynamic choices, an upload control, and
a separate Save button. Saccade can preflight independent fields as one batch,
verify each step without returning editable values, and leave Save as a separate
action. It does not turn a convenient batch into hidden submission.

Version 0.2.0 expands that path across contenteditable and same-origin iframe
rich text, native and ARIA choices, semantic tables, standard upload controls,
and fast moving targets. The same Extension 0.4.0 candidate passed the blocking
release gate in Chrome and Edge.

The Mouse Accuracy gate is a useful stress case. Each browser completed 24
ordinary targets and 24 canvas targets with zero misses in this run. Across both
browsers that was 96/96 exact-target actions, with mean latency between 7.22 ms
and 8.50 ms. The public repository includes the machine-readable evidence and
fixture so the claim remains attached to its conditions.

Saccade does not make Playwright obsolete. Playwright is designed around
reproducible testing, locators, assertions, and isolated browser contexts.
Saccade is designed for an Agent operating the user's current authorized tab.
In our latest controlled form comparison, Saccade used fewer tool calls and
less time inside the browser MCP path, while Playwright finished the complete
Agent task sooner. Both facts belong in the comparison.

The practical question is not which tool wins every benchmark. It is whether
the job is a clean test or live Agent work. For the latter, exact tab ownership,
small revisioned updates, and an answer to “did that action take effect?” are a
useful foundation.

Project and evidence: https://github.com/nanlogic/saccade

## Hacker News

Post only the repository link with this title:

```text
Saccade 0.2.0 – Live semantic browser Truth and verified actions for agents
https://github.com/nanlogic/saccade
```

## Sources used for the comparison

- Playwright actionability: https://playwright.dev/docs/actionability
- Playwright locator strictness: https://playwright.dev/docs/locators#strictness
- Playwright browser-context isolation: https://playwright.dev/docs/browser-contexts
- Stagehand product position: https://www.stagehand.dev/
- Saccade 0.2.0 release gate:
  ../reports/2026-08-30-saccade-0.2.0-release-gate.md
- Controlled Saccade/Playwright form comparison:
  ../reports/2026-08-28-current-saccade-playwright-speed.md
