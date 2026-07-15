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

The adapter advertises only `ping`, `shell_status`, `navigate`, `pause`, and
`close`. Closing the granted tab removes the socket, bearer capability, grant,
and private session directory. The ordinary `run_macos.sh` path does not start
an agent transport.

Run the engine-neutral Python and TypeScript lifecycle gate with:

```sh
engines/cef/scripts/test_day2_macos.sh
```

The gate always uses a hidden incognito profile with Chromium's test-only mock
keychain. It cannot read or modify the user's normal browser credentials and
does not produce macOS Keychain prompts. Normal profiles keep the platform
credential store; the mock keychain switch is never enabled by product launch
scripts.
