# Cross-site stability and fair Agent report

Date: 2026-08-01  
Status: local development evidence; not publication evidence

## Result

The public runner is now data-driven and records URL, source, implementation
type, outcome stage, dispatch status, postcondition, elapsed time, source
commit, and redacted full/delta/receipt evidence. Fixture results remain
separate. External status now requires two independent traceable public sources
per control and browser; old untraceable `passed` flags no longer count.

The final public suite passed 9/9 cases in both managed Chrome and Edge under
evidence root `20260801T133340Z`. It covers Selenium native HTML text field,
textarea, select, checkbox, and radio plus W3C ARIA radio, switch, tab, and menu
item. Radio is currently the only control with two independent public sources
in both browsers. The other controls remain explicit evidence gaps, and every
Catalog row remains `implementation`.

## Root fixes from Angular Material

Angular Material revealed a general dynamic-choice dead end: a collapsed ARIA
combobox exists before its overlay options. Saccade previously required option
identity for `select` but offered no legal expand action. The shared select
module now declares two audited strategies:

1. collapsed ARIA combobox `click` → `primary_click` → `expanded_transition`;
2. fresh option identity `select` → `select_option` → `option_selected`.

Native select remains unchanged. No URL, selector, framework name, special
wait, or site branch entered production code. Duplicate actionable controls
across all families now receive bounded value-free semantic context rather
than limiting that disambiguation to buttons and links. Initial Host readiness
has its own bounded gate, and accepted-but-unverified software receipts tell
the Agent that the local policy already learned native.

## Unknown-page Saccade versus Playwright

Both lanes received only the same public URL and natural-language task. Page
discovery, semantic transfer, action, wait, recovery, and browser-proven
completion were timed. The runner isolated the user input policy, restarted
managed Chrome, waited for MCP readiness, prohibited selectors, source and DOM
inspection, screenshots, coordinates, and human help, then reversed order.

| Task | Lane | Pass | Mean time | Mean calls | Mean input tokens |
| --- | --- | ---: | ---: | ---: | ---: |
| Selenium official form | Saccade | 2/2 | 32.169 s | 4.5 | 82,660 |
| Selenium official form | Playwright | 2/2 | 35.666 s | 6.0 | 113,558 |
| DemoQA React form | Saccade | 2/2 | 47.587 s | 6.5 | 125,760 |
| DemoQA React form | Playwright | 2/2 | 36.724 s | 5.0 | 98,759 |
| Angular Material select | Saccade | 2/2 | 103.661 s | 16.5 | 428,558 |
| Angular Material select | Playwright | 2/2 | 54.730 s | 9.0 | 160,257 |

Evidence roots are `20260801T132632Z` (Selenium), `20260801T132919Z`
(DemoQA), and `20260801T131954Z` (Angular). Earlier Angular reports are retained
as diagnostics but excluded because they predated the root fix, inherited local
policy, used different browser families, or lacked the MCP-readiness gate.

Saccade wins this Selenium task on time, calls, and input tokens. It loses the
DemoQA and Angular tasks on time and tokens. Angular's large initial Truth
Layer, page churn, one soft-to-native learning step, and Agent recovery remain
concrete optimization targets. These results provide task-specific completion
evidence and expose current costs for these exact pages and candidates. They do
not prove general modern-web compatibility or support a blanket claim that
Saccade is faster than Playwright. This historical Saccade lane used the
execution stack that is now the optional Reference Actuator; it is not the
current core-product lane.

## Remaining evidence gaps

- Add a second independent public source for every control except radio.
- Activate validated Vue and Web Component cases without site-specific logic.
- Add public iframe, open-shadow, delayed-render, and dynamic-replacement cases.
- Cover button, link, search field, contenteditable, spin button, reflex target,
  and file input across both browsers.
- Keep signed release installation evidence separate; no control is publishable.
