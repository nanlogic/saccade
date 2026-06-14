# ServoShell Source Strategy

Date: 2026-06-14

## Decision

Use official ServoShell source as the preferred browser-productization route.

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

## Next Gate

1. Find or clone the official Servo source revision that matches
   `Servo 0.3.0-302457869`.
2. Build `servoshell` locally with the official build path.
3. Verify it runs `http://127.0.0.1:4173/` like the downloaded app.
4. Add a Saccade adapter around official ServoShell WebDriver before attempting
   a deeper fork.

## Build Note

This is not primarily an Xcode project. Xcode/Command Line Tools are still
needed for macOS SDK, native frameworks, signing, and packaging, but the core
build is the Servo/Rust build.
