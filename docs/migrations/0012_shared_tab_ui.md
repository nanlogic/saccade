# Shared-tab Extension UI

Date: 2026-07-29

## Provenance

The popup uses the current session ACL and authorization functions in
`extension/src/service_worker.js`. No UI or authorization code was copied from
`nanlogic/saccade-legacy` commit `8c4defb3f8b0`.

## Destination and behavior

- `extension/popup.html`, `popup.css`, and `popup.js` show Agent Off,
  user-shared, Agent-owned, collector readiness, and Runtime connection state.
- Only the Extension popup URL may send share, revoke, or status messages.
- Sharing adds one supported active tab to `chrome.storage.session`, configures
  its collector, and rolls back on failure.
- Revocation removes the shared tab, discards its observation session, clears
  collector authority, and stops its mutation observer.
- Agent-owned tabs remain separate and are revoked by closing them.

## Checks and evidence

Static Extension tests verify the fixed popup entry point, popup-only message
boundary, session ACL mutation, rollback path, and collector deauthorization.
Manual managed Chrome and Edge UI evidence remains pending because the local
Apple Development signing identity is absent.
