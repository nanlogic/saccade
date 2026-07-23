# Saccade

Saccade is an experimental, agent-native desktop browser built around a simple
rule: an agent should act only from structured facts tied to the visible browser
tab, and every native input should produce a verifiable receipt.

The project is in active dogfood development. It is not yet a stable general
browser release. The current product engine is Chromium Embedded Framework
(CEF), with Windows and macOS hosts plus a local MCP interface for authorized
agent workflows.

## Why Saccade

Most browser automation treats the browser as a remote test target. Saccade
instead treats the visible browser as a human-and-agent shared workspace:

- access is granted per tab and can be paused or revoked by the human;
- observations are revision-bound structured facts from the same WebView;
- native actions are checked against the observed page revision;
- passwords, one-time codes, payment secrets, and other protected values stay
  outside the agent data path;
- submit, publish, purchase, and comparable side effects remain separate from
  ordinary form completion; and
- missing truth, stale targets, transport failures, and unverifiable receipts
  fail closed.

The detailed product contract and current engineering constraints live in
[`SACCADE_BUILD_SPEC_v4.md`](SACCADE_BUILD_SPEC_v4.md). Public dogfood evidence
and explicit non-claims start with the
[`Public Evidence Guide`](docs/PUBLIC_EVIDENCE_GUIDE.md) and curated
[`evidence/`](evidence/) packs. Milestone files under [`docs/`](docs/) are the
engineering archive and may describe obsolete builds.

## Current status

- **Windows:** active CEF dogfood host, staged profile-preserving installer, and
  local MCP bridge. Development builds are currently unsigned while the project
  applies for open-source code signing through SignPath Foundation.
- **macOS:** active CEF dogfood host with platform signing and Keychain-aware
  profile handling.
- **Agent layer:** fixed truth, action, form, article, download, and reflex
  surfaces with value-free replay evidence.
- **Compatibility:** ordinary Chromium pages are the target. Site-specific
  challenges, proprietary media codecs, and anti-bot services may still route
  to human handling or a system browser. Saccade does not bypass CAPTCHA or
  security verification.

There is no supported stable release yet. When public artifacts are ready they
will appear on [GitHub Releases](https://github.com/nanlogic/saccade/releases).

## Repository map

| Path | Purpose |
| --- | --- |
| `engines/cef/` | Windows and macOS CEF browser hosts, packaging, and platform tests |
| `bins/saccade-mcp/` | Local MCP server and browser-agent policy surface |
| `crates/` | Engine-neutral truth, motor, verification, replay, and browser components |
| `scripts/` | Regression, packaging, and dogfood utilities |
| `docs/` | Decisions, compatibility ledger, evidence, and non-claims |
| `fixtures/`, `test_pages/` | Local deterministic browser fixtures |

## Build and test

Rust workspace checks:

```sh
cargo test --workspace
```

CEF platform requirements and build commands are documented in
[`engines/cef/README.md`](engines/cef/README.md). The short Windows path is:

```powershell
engines\cef\scripts\build_windows.ps1
engines\cef\scripts\install_windows.ps1
```

The Windows installer preserves the Saccade profile. Do not point development
scripts at a Chrome profile or another browser's storage.

## Security and privacy

Please read [`SECURITY.md`](SECURITY.md) before reporting a vulnerability.
Saccade's security boundary depends on tab grants, protected-value isolation,
same-WebView provenance, revision checks, and verified input receipts. A visual
change by itself is not proof that an agent action succeeded.

## Contributing

Contributions and careful compatibility reports are welcome. See
[`CONTRIBUTING.md`](CONTRIBUTING.md). Please keep claims narrow, add the smallest
relevant regression, and record evidence without cookies, credentials, form
values, or other personal data.

## License and trademarks

Source code is licensed under the [Apache License 2.0](LICENSE). Third-party
notices are in [`NOTICE`](NOTICE). The license does not grant rights to the
Saccade name, logos, or official distribution identity; see
[`TRADEMARKS.md`](TRADEMARKS.md).

Saccade is published by [NaN Logic LLC](https://nanlogic.com/).
