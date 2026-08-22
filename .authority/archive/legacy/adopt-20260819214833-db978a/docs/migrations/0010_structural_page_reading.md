# Structural page reading

Date: 2026-07-29

## Provenance

This slice was implemented from `docs/extension_observation_contract.md` and
the current v1 protocol. No code was copied from `nanlogic/saccade-legacy`
commit `8c4defb3f8b0`, and no legacy classifier was migrated.

## Destination and behavior

- `extension/src/collector.js` recognizes visible headings, paragraphs, list
  items, table cells, alerts, and status messages.
- Structural objects use `kind=text`, carry text in the dedicated `text`
  field, and expose no name, affordance, or action token.
- Heading level and authored alert/status busy state use the existing safe
  state keys.
- Hidden nodes, nested controls and images, editable contents, and nested
  structural descendants are excluded from text extraction.
- A 256 KiB UTF-8 budget reports the existing `truncated` limitation rather
  than presenting an unmarked partial projection.

## Checks and evidence

The fixture includes each structural role plus hidden and nested-editable leak
sentinels. The development probe checks roles, text, heading level, alert busy
state, non-actionability, and absence of both sentinels. Node collector tests
and Rust protocol tests pass. Managed Chrome and Edge evidence is pending
because the local Apple Development signing identity is currently absent;
Catalog publication status is unchanged.
