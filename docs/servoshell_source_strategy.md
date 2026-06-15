# ServoShell Source Strategy

Date: 2026-06-14

## Decision

Use official ServoShell as the preferred browser-productization runtime.

The first implementation route is an external WebDriver adapter. A thin source
fork of official ServoShell is the safety fallback if WebDriver cannot satisfy
Saccade's product gates.

Wayne verified that the downloaded official macOS Servo.app can run the local
game at:

```text
http://127.0.0.1:4173/
```

The installed app reports:

```text
ServoShell 0.3.0
servoshell --version => Servo 0.3.0-302457869
```

Saccade currently embeds `servo = "=0.2.0"` from crates.io. This version gap is
now a first-class suspect for the canvas/game/runtime mismatch.

## What Not To Do

Do not patch the downloaded `/Applications/Servo.app` binary directly. It is a
reference artifact, not a maintainable source base.

Do not treat `ign.com` as a Saccade-specific blocker right now. Wayne verified
that official Servo.app has the same bad behavior on IGN, so it is an upstream
Servo/site compatibility limit for this phase.

## Viable Routes

### Route A: External Agent Bridge

Run official `servoshell` with `--webdriver` or `--devtools`, then connect a
Saccade agent controller from the outside.

This is the fastest way to dogfood against the same browser binary that already
runs the local game. It may be weaker for trusted-tab safety and native input
ownership, but it can quickly validate browser/runtime parity.

2026-06-14 result: WebDriver is viable for the first adapter gate. Saccade can
create a session, execute JavaScript, click a button, observe the resulting DOM
change, and capture a screenshot through official ServoShell.

Probe script:

```sh
python3 scripts/probe_servoshell_webdriver.py
```

### Route B: Fork Official ServoShell

Build official ServoShell source and add Saccade's agent bridge inside that app:

- browser truth/action-map extraction
- safe field policy
- replay logging
- login/profile handoff
- Saccade shell controls

This is the best product route if Route A proves the official shell runtime is
the missing piece.

### Route C: Upgrade Saccade Embedder

Move Saccade from crates.io `servo = 0.2.0` to a git/source Servo matching the
official ServoShell build, then keep the existing Saccade shell architecture.

This keeps Saccade's current ownership boundaries but may require Servo API
mapping updates and a heavier local build.

External review result: avoid Route C for now. It is likely the worst tradeoff:
heavy API migration while still not guaranteeing parity with official
ServoShell's working runtime.

## Next Gate

1. Find or clone the official Servo source revision that matches
   `Servo 0.3.0-302457869`.
2. Build `servoshell` locally with the official build path.
3. Verify it runs `http://127.0.0.1:4173/` like the downloaded app.
4. Add a Saccade adapter around official ServoShell WebDriver before attempting
   a deeper fork.
5. Switch to a thin official ServoShell fork only if the external adapter fails
   screenshot safety, trusted UI, login handoff, input provenance, or native
   action semantics gates.

## Build Note

This is not primarily an Xcode project. Xcode/Command Line Tools are still
needed for macOS SDK, native frameworks, signing, and packaging, but the core
build is the Servo/Rust build.

## Source Recon - 2026-06-14

Primary upstream source:

```text
https://github.com/servo/servo
```

The official repository contains `ports/servoshell`, and its public README
describes the macOS path as:

```text
./mach bootstrap
./mach build
```

Local installed app metadata:

```text
/Applications/Servo.app/Contents/Info.plist
CFBundleShortVersionString = 0.3.0

/Applications/Servo.app/Contents/MacOS/servoshell --version
Version: Servo 0.3.0-302457869
```

Remote tag check:

```text
v0.0.1 721214fbe44bf11b968e5e076e5b0af5b5663447
v0.0.2 cae6350af7c734444ddd4a238f2c47d70cecc3c5
v0.0.3 7cd39f4d2d9dbdf4940727d19a81ab8d09f3c0e8
v0.0.4 b73ae025690cce16185520ea88a6df162fc1298d
v0.0.5 301f7da52e320764fd0063facb90de0ec8db8f25
v0.0.6 7c2c0aaf23107a16cddb8b5f7904bcb3f308a086
main   54288c9d684306508bb30eeb61e071eed0e9192f
```

No public remote ref matched `0.3.0` or `302457869` during this check. Treat the
installed app as a reference binary, not an exact source identifier yet.

Practical next step:

1. Clone official `servo/servo` outside the Saccade workspace or as an
   explicitly ignored sibling checkout.
2. Try a current-main or newest-tag `./mach build` smoke first.
3. Confirm the locally built `servoshell` runs `http://127.0.0.1:4173/`.
4. Only after that, add the minimal in-process Saccade reflex bridge.

Clone completed:

```text
/Users/waynema/Documents/GitHub/servo-saccade-upstream
commit 54288c9d6
```

The current bridge source map is recorded in `docs/reflex_bridge_audit.md`.

Build preflight:

- `rust-toolchain.toml` pins Rust `1.95.0`.
- `./mach --help` initialized Servo's local `.venv` and confirmed the command
  set, but `./mach bootstrap` has not been run.
- `./mach build --help` confirms build types `--dev`, `--release`, and
  `--prod`, plus `-j/--jobs`.
- Recommended first build attempt: `./mach build --dev -j 4`.
