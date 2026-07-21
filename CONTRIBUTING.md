# Contributing to Saccade

Saccade is an early dogfood project. Small, evidence-backed changes are easier
to review than broad browser redesigns.

## Before opening a pull request

1. Read `AGENTS.md`, the relevant section of `SACCADE_BUILD_SPEC_v4.md`, and any
   local instructions in the directory you are changing.
2. Reproduce the issue with the smallest local fixture possible.
3. Preserve the core boundaries: explicit per-tab grants, protected-value
   isolation, same-WebView provenance, page-revision checks, native input
   receipts, and fail-closed errors.
4. Add or update the smallest relevant regression.
5. Run focused checks for the changed component. Run broader workspace or
   platform gates when the change affects shared contracts or packaging.

## Evidence hygiene

Never commit browser profiles, cookies, credentials, tokens, OTPs, payment
data, private form values, screenshots of sensitive pages, or unrestricted
debug logs. Test fixtures should use reserved domains and clearly fake values.
Replay and reports should contain field identifiers, counts, statuses, and
failure reasons—not user-entered values.

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
