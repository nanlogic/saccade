# Frame and open-shadow composition

Date: 2026-07-31

## Provenance

This slice was implemented from `docs/extension_observation_contract.md` and
the existing v1 frame/limitation schema. No code, frame tree, classifier, or
execution route was copied from `nanlogic/saccade-legacy` commit
`8c4defb3f8b0`.

## Destination and behavior

- `extension/src/collector.js` keeps the existing top-document
  `collector.observation` route and composes accessible same-origin iframe
  documents into that snapshot.
- Open shadow roots contribute normal descendants. Closed shadow roots are not
  traversed and are not claimed as generically detected.
- Inaccessible frames carry frame identity and `restricted_permission` status
  plus the existing `restricted_frame` limitation.
- Descendant document and shadow mutations schedule ordinary browser-pushed
  revisions.
- Native preparation composes local geometry through the same-origin
  `frameElement` chain and revalidates both the target and ancestor coverage.
- No locator, arbitrary coordinate, editable value, or new Host/MCP route is
  exposed.

## Checks and evidence

`fixtures/structural/frames_and_shadow.html` contains one same-origin frame, one
opaque-origin frame, one open shadow root, and one closed shadow root. Static
Extension tests preserve the root route and verify the composition boundaries.
Paired managed Chrome and Edge run `20260731T051006Z` reported two observed
frames and one restricted frame per browser, withheld both opaque
descendants, and returned native `accepted_by_os + verified` receipts for the
same-origin frame button and open-shadow button. Evidence remains local
development evidence and does not make the Catalog publishable.
