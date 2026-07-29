# ARIA listbox and combobox choices

Date: 2026-07-29

## Provenance

This slice extends the current select Registry module, v1 observation contract,
and finite platform-input adapter. No code was copied from
`nanlogic/saccade-legacy` commit `8c4defb3f8b0`, and no legacy classifier was
migrated.

## Destination and behavior

- `extension/src/collector.js` recognizes standalone ARIA listboxes and ARIA
  comboboxes bound to listboxes by `aria-controls` or `aria-owns`.
- Both project as the existing `select` role. Their page-authored choices use
  the existing non-actionable `option` role and retain runtime object identity.
- Preparation requires a current, enabled option bound to the target owner and
  returns its position among enabled options.
- The platform adapter uses one finite click, popup wait, Home key, bounded Down
  keys, Return, and settle delay. It does not use a locator or accept arbitrary
  keyboard input.
- The option-selected verifier checks the requested object identity after a
  fresh observation.

## Checks and evidence

The focused fixture covers a standalone listbox, a controlled combobox, a
disabled option, duplicate visible names, a dynamically inserted option, and
popup close. Node collector checks and Runtime finite-plan tests pass. Managed
Chrome, Edge, and public-page evidence remains pending because the local Apple
Development signing identity is absent. The select fixture evidence was reset
to pending when its claimed surface expanded.
