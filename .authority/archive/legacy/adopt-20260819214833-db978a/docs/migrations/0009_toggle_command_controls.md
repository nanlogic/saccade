# Toggle and command controls

Date: 2026-07-29

## Provenance

Radio, ARIA switch, tab, and menu item were implemented from the current public
contracts and existing Registry patterns. No code was copied from
`nanlogic/saccade-legacy` commit `8c4defb3f8b0`, and no monolithic classifier
was migrated.

## Destination and behavior

- Extension modules: `extension/src/controls/radio.js`, `switch.js`, `tab.js`,
  and `menu_item.js`.
- Collector: explicit native-radio and ARIA-role recognition with safe state
  only.
- SDK: checked, selected, and expanded transition verifiers over the existing
  `primary_click` primitive.
- Fixtures: one focused fixture per control plus the managed all-controls gate.

Radio and switch advertise click only while enabled. Tab verifies that the
target becomes selected. Menu item v1 advertises click only for an explicit
expanded-state loop; command-only effects remain outside this claim.

## Checks and evidence

Node Registry/collector tests and Rust closed-loop tests cover projection,
unavailable controls, finite primitives, and role-specific verification.
Managed Chrome run `20260729T192723Z` and Edge run `20260729T192757Z` each
recorded 12 native verified receipts, stale-token rejection, Profile filtering,
and an editable-value leak scan. Evidence is local development evidence, so all
Catalog rows remain `implementation` and browser evidence remains `pending`.

Public-page comparison run `20260729T211221Z` added W3C WAI-ARIA radio, switch,
tab, and menubar examples. Chrome and Edge each produced four independent
Saccade native verified receipts, then an isolated Playwright oracle matched
all four names and false-to-true state transitions. External dogfood corrected
three fixture-blind issues: ARIA radio fallback names, `aria-hidden` text
exclusion, and explicit `role=menuitem` precedence over native anchor
projection.
