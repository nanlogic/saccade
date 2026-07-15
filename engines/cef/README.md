# Saccade CEF host

This directory is the direct official CEF host for the Chromium product
engine. Day 1 uses the standard distribution so the macOS bundle starts from
the complete upstream sample resources. CEF archives and build products are
intentionally kept out of git.

## Build on Apple Silicon

Requirements: macOS 14.5+, Xcode 16+, CMake 3.21+, Ninja, and about 2 GB of
free disk space.

```sh
engines/cef/scripts/fetch_macos.sh
engines/cef/scripts/build_macos.sh
```

The release app is written to `target/cef-release/Saccade.app`. The outer app
is branded Saccade while the internal executable and helpers retain their
upstream names. Keeping the tested upstream lifecycle intact is a deliberate
Day 1 gate, not a product boundary.

```sh
engines/cef/scripts/run_macos.sh normal https://example.com

engines/cef/scripts/run_macos.sh incognito https://example.com
```

Normal browser state lives under
`~/Library/Application Support/Saccade/CEF/Profiles/<name>`. Incognito uses an
isolated temporary user-data directory that is removed after clean exit.
Neither profile reads Chrome, Servo, or another Saccade profile's storage.

### macOS signing and Keychain

Normal profiles use the macOS Keychain-backed Chromium Safe Storage key to
encrypt cookies and login state. Public or login-bearing builds must have a
stable signing identity; an ad-hoc build has a changing designated requirement
and can repeatedly prompt for Keychain access after rebuilds.

Build and sign with the first available Developer ID identity:

```sh
SACCADE_CODESIGN_IDENTITY=auto engines/cef/scripts/build_macos.sh
```

Or sign an existing staged app with an explicit certificate SHA-1:

```sh
SACCADE_CODESIGN_IDENTITY=<certificate-sha1> \
  engines/cef/scripts/sign_macos.sh target/cef-release/Saccade.app
```

`--use-mock-keychain` is restricted to automated tests and temporary profiles
that never receive real credentials. It uses a test encryption key and must not
be used for GitHub, payments, government sites, or other human login sessions.

`--remote-debugging-port` is accepted by CEF for local tests, but the build and
launch scripts never enable it by default. It is not part of the product
agent interface.

## Supply chain

`cef.lock.json` pins the CEF and Chromium revisions, official archive URLs,
the SHA-1 published by the CEF build service, and locally measured SHA-256.
The fetch script rejects an archive unless both digests match. CEF's license
and Chromium credits are copied into the application bundle at build time.

The host source begins from the official `cefsimple` structure and preserves
the upstream BSD notices. Saccade policy, truth, grants, and replay do not
belong in this directory; later days connect those through the versioned
engine adapter.

## Engine adapter lifecycle

Day 2 adds a browser-process adapter without CDP or page injection. Launching
through the explicit grant wrapper creates a one-session Unix socket and grant
under an owner-only temporary directory:

```sh
engines/cef/scripts/run_adapter_macos.sh normal https://example.com
```

Day 2 starts with `ping`, `shell_status`, `navigate`, `pause`, and `close`.
Day 3 adds `truth`, `actions`, `next_fact`, `act`, `next_receipt`, and
`reflex_start` for the bounded renderer truth/reflex path. Closing the granted
tab removes the socket, bearer capability, grant, and private session
directory. The ordinary `run_macos.sh` path does not start an agent transport.

Run the engine-neutral Python and TypeScript lifecycle gate with:

```sh
engines/cef/scripts/test_day2_macos.sh
```

The gate always uses a hidden incognito profile with Chromium's test-only mock
keychain. It cannot read or modify the user's normal browser credentials and
does not produce macOS Keychain prompts. Normal profiles keep the platform
credential store; the mock keychain switch is never enabled by product launch
scripts.

## Truth/reflex gate

The no-CDP CEF kill gate uses the unchanged Chrome POC fixture and three
independent 100-target Release runs:

```sh
engines/cef/scripts/test_day3_macos.sh
```

The renderer collector emits allowlisted target geometry, semantic identity,
page revision, control kind/completion, and input receipts. It never exports
field values. The browser adapter resolves action ids to coordinates and uses
native CEF pointer input. See `docs/cef_day3_truth_reflex_report.md` for the
measured boundary and explicit non-claims.

Visible Canvas elements are emitted as revision-bound `surface` actions.
`act_drag` accepts only the action id and an allowlisted cardinal direction;
the browser resolves coordinates within the current surface and requires a
matching renderer receipt. With the local Blend or Die server running:

```sh
python3 scripts/probe_cef_local_game.py \
  --url http://127.0.0.1:4173/ \
  --output-dir runs/cef_day5/local_game_final \
  --headed
```

## Day 4 form and safety gate

The current-tab grant additionally advertises fixed form inventory, inspect,
compile, execute, lazy reveal, screenshot-policy, and screenshot-audit
commands. These commands are revision scoped and do not expose a general page
evaluation primitive.

```sh
python3 scripts/probe_cef_form_safety.py \
  --output-dir runs/cef_day4_form_safety

python3 scripts/probe_cef_formmax.py \
  --output-dir runs/cef_formmax
```

The first gate verifies ordinary controls, human/agent preservation, sensitive
redaction, pre-capture blocking, a permitted non-sensitive screenshot, and
value-free replay. The second fills the 96-row, two-page lazy FORMMAX fixture.
See `docs/cef_day4_forms_safety_report.md`.

The screenshot audit uses CEF's internal structured DevTools method only after
policy approval. Remote debugging remains disabled, no screenshot bytes enter
truth or replay, and truth/actions/forms do not use CDP.
