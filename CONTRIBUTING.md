# Contributing to Saccade

Saccade accepts small, evidence-backed control and runtime changes. The
architecture and wire boundaries remain fixed while coverage grows by control
family.

## Before opening a pull request

1. Read `AGENTS.md`, `docs/FINAL_ARCHITECTURE.md`, and
   `docs/extension_observation_contract.md`.
2. Check `docs/CONTROL_ROADMAP.md` and work within one listed batch.
3. Reproduce the behavior with the smallest local fixture possible.
4. Preserve explicit per-tab grants, protected-value
   isolation, browser-instance provenance, page-revision checks, native input
   receipts, and fail-closed errors.
5. Add the Catalog entry, Registry module, Extension projection, verifier,
   fixture, leak check, and stale/focus/covered rejection tests together.
6. Run focused checks for the changed component. Run the full README check list
   before opening a pull request that changes a contract or control family.

One pull request should contain one control family or one runtime boundary.
Catalog status stays `implementation` until the same release candidate passes
Chrome and Edge.

## Evidence hygiene

Never commit browser profiles, cookies, credentials, tokens, OTPs, payment
data, private form values, screenshots of sensitive pages, or unrestricted
debug logs. Test fixtures should use reserved domains and clearly fake values.
Replay and reports should contain field identifiers, counts, statuses, and
failure reasons, never user-entered values.

Compatibility claims must name the exact platform, engine build, site or local
fixture, test route, and observed limitation. Do not generalize one successful
session into a claim that CAPTCHA or anti-bot systems are supported.

## Style

- Keep platform-specific behavior behind explicit platform boundaries.
- Prefer fixed, bounded command surfaces to arbitrary script execution.
- Keep changes deterministic and avoid hidden network dependencies in tests.
- Update documentation when behavior, safety boundaries, or public claims
  change.

By contributing, you agree that your contribution is licensed under the
repository's Apache-2.0 license.
