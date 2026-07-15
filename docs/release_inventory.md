# Saccade V3 Release Inventory

Status: signed local macOS dogfood kit; not a notarized public release.

## CEF dogfood artifact

`engines/cef/scripts/build_dogfood_release_macos.sh` produces the local release
under `dist/saccade-cef-dogfood-<stamp>/` and updates
`dist/saccade-cef-dogfood-current`. It contains:

- a fixed-identity signed `Saccade.app` using CEF `150.0.11` and Chromium
  `150.0.7871.115`;
- owner-only saved/incognito profile and current-tab grant launchers;
- exact source/engine metadata, CEF license, Chromium credits, and portable
  SHA-256 checksums;
- the integration contract and Day 5 measured report;
- `bin/run-local-game-gate`, which reruns the fact-bound Canvas drag and
  guarded-render validation against an already-running local game server.

The artifact is deliberately marked `notarized=false` and
`public_distribution_ready=false`.

## Included contract material

- `docs/integration_contract_v1.md` — version negotiation, policy, lifecycle, typed errors, and compatibility rules.
- `docs/integration_examples/typescript-host/` — stdio MCP host flow.
- `docs/integration_examples/python-host/` — equivalent standard-library host flow.
- `saccade-mcp serve-stdio` — the only supported local-tool endpoint.

## Reproducible local gate

```bash
RUST_LOG=error cargo run -q -p saccade-mcp -- selftest
cargo run -q -p saccade-mcp -- tools
```

The selftest checks MCP initialization, registry discovery, persistent tab state, policy gates, redaction, current-tab grants, safe fill, action verification, replay validation, and bridge shutdown paths.

## Deliberately not claimed complete

V3's distribution requirement cannot honestly be closed by source changes alone. The following need release-owner authority and external infrastructure:

| Item | Status | Required owner action |
| --- | --- | --- |
| License selection | blocked | Choose and approve a project license. |
| macOS code signing/notarization | partial | Local Developer ID signing passes. Decide hardened-runtime entitlements, notarize, staple, and test on a clean Mac. |
| Windows/Linux signing | blocked | Provide platform certificates and release channels. |
| SBOM + dependency/license inventory | pending | Freeze a release commit and choose the output standard/attestation signer. |
| Checksums and hosted artifacts | partial | Local SHA-256 inventory passes. Choose distribution location and release key. |

Until these are complete, distribute only as a signed local engineering
evaluation kit. Do not represent it as a notarized, public, cross-platform, or
supported product release.
