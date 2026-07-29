# Truth Layer coverage

The machine-readable [Control Catalog](../catalog/controls.json) defines the
current Registry. `catalog/development_evidence.json` records fixture and
external development proof separately from release evidence. The
[generated coverage table](generated/control_coverage.md)
is the public status for implemented controls. The
[control roadmap](CONTROL_ROADMAP.md) lists planned batches without presenting
them as current support.

## Status meanings

| Status | Meaning |
| --- | --- |
| `implementation` | Source, fixtures, focused tests, and a development route exist. Release evidence remains incomplete. |
| `publishable` | Chrome and Edge passed the same release candidate through the production route. |
| Planned | The role appears only in the contract or roadmap. It has no Catalog module or action claim. |
| Limited | Saccade reports an object or surface without claiming unsupported semantics or action. |

## Current controls

Saccade currently implements button, link, text field, search field, textarea,
contenteditable, spin button, checkbox, radio, ARIA switch, select, tab, menu
item, reflex target, file input, and select-option observation. The current
toggle/command candidate passed 12 native closed loops in Chrome run
`20260729T192723Z` and the same 12 in Edge run `20260729T192757Z`, with Profile
and stale-token gates in both browsers. Reflex target, link, and file input have
focused managed Chrome evidence, including authenticated file-selection
dogfood, but not paired release evidence. Every Catalog row remains
`implementation` because signed-product Chrome and Edge evidence for one
release candidate is pending.

Public W3C WAI-ARIA run `20260729T211221Z` independently produced native
verified Saccade receipts for radio, switch, tab, and menu item in Chrome and
Edge. A separate Playwright oracle matched all four semantic names and state
transitions and saved screenshots. Playwright results are comparative only and
do not count as Saccade receipts or release evidence.

## Coverage tiers

Common controls require semantic identity, safe state, revision-bound native
action, control-specific verification, redaction checks, and current Chrome and
Edge evidence.

Uncommon controls begin with truthful recognition, safe state, and an explicit
limitation. The team adds native action after a focused browser gate proves the
postcondition.

Browser and operating-system chrome, arbitrary closed-shadow internals, PDF
form internals, and arbitrary Canvas/WebGL/custom widgets remain outside the
core. Saccade reports an opaque or restricted surface instead of inventing
controls.

## Evidence required for each control

- accepted native input and a verified semantic postcondition;
- stale, replayed, detached, hidden, covered, unfocused, and unauthorized
  rejection;
- Profile-ban filtering before Extension preparation;
- absence of editable or protected values from observations, receipts, logs,
  diagnostics, and committed artifacts;
- current Chrome and Edge artifacts tied to one source commit and release
  candidate.

Historical CEF, Servo, detector, and benchmark work lives in the private
`nanlogic/saccade-legacy` archive. It can guide a reviewed migration, but it
cannot satisfy current evidence or appear as current support.
