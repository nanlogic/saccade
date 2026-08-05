# Truth coverage and evidence roadmap

Saccade has completed the current local Truth inventory. The next milestone is
not a larger role count; it is source-diverse evidence that the existing
Extension compiler remains truthful across real implementations and page
lifecycles.

The machine-readable scope is `catalog/truth_inventory.json`: 34 protocol
roles, 12 reusable variants, and 6 structural/push boundaries. The separate
`catalog/controls.json` list contains 15 optional Reference Actuator families
and is not the core product roadmap.

`catalog/public_truth_cases.json` is the explicit public-evidence denominator.
Every inventory and lifecycle row must retain one of `pass`,
`truthful_limitation`, `unsupported`, or `blocked`; missing source documents or
same-tab execution capability remain visible blockers rather than implicit
skips.

## Evidence levels

- `implemented`: focused source and fixture tests exist.
- `local Chrome + Edge`: the real Extension → Host → Runtime → MCP → pushed
  delta route passes in both managed browsers.
- `public compatibility`: independent public implementations compile and push
  truthful changes; repeated runs of one site do not increase source diversity.
- `publishable`: one frozen release candidate passes the complete current
  Chrome and Edge matrix plus packaging and clean-install gates.

Local success proves the framework and projection path. It does not prove that
all modern websites, frameworks, or browser-owned surfaces are supported.

## Priority 1: public-site compatibility

Build a traceable matrix across independent implementations:

- Selenium official forms;
- WAI-ARIA Authoring Practices Guide examples;
- Angular Material;
- one official Vue component library;
- official Web Components and open Shadow DOM examples;
- dynamic replacement, delayed rendering, and iframe cases.

For every case retain the initial Truth view, Extension-produced delta,
browser/version, source URL, limitation or failure reason, and redacted transfer
metrics. Fix shared compiler defects at the Registry or collector boundary;
do not add site-specific selectors.

## Priority 2: fair Playwright comparison

Run at least three unknown-page, natural-language tasks:

1. native HTML form;
2. React dynamic page;
3. Angular or Vue multi-control page.

The Saccade lane uses Saccade Truth plus Codex or Claude's own web-act tool in
the same tab. It does not use the Reference Actuator. The Playwright lane uses
official Playwright MCP. Neither lane receives selectors, control names, page
structure, prepared scripts, or state from the other lane.

Record completion, initial discovery time, initial bytes and estimated model
tokens, page-change-to-Agent delta latency, post-action re-observation count,
stale/dynamic-replacement recovery, total tool calls, total task time, and all
failure reasons. Compare full trajectories, not click latency.

Existing historical actuator/oracle benchmarks remain implementation records;
they are not evidence that the core Truth Layer is faster than Playwright.

## Priority 3: lifecycle scenarios

Complete the legacy gauntlet as page-behavior evidence, not new roles:

- dynamic loading and delayed resources;
- disappearance and large DOM replacement;
- overlays, modals, and dialogs;
- infinite scroll and viewport changes;
- sortable tables;
- upload/download Truth representation;
- drag/drop representation and limitations.

## Priority 4: release

After public evidence is complete:

- freeze one release candidate and gate the same build in Chrome and Edge;
- prove default install and use without Accessibility;
- package the store Extension, signed macOS DMG, and Windows Setup;
- dogfood Codex and Claude against the same browser instance;
- publish a five-minute README quickstart and reproducible evidence bundle.

Current honest claim: Saccade has a complete local Truth role inventory and a
two-browser pushed-delta framework gate. Universal modern-web compatibility and
superiority over Playwright remain unproven.
