# Servo 0.2 Retirement Plan

Date: 2026-06-19

## Decision

Do not perform an in-place `servo = 0.2.0 -> 0.3.x` upgrade inside the legacy
`saccade_browser` embedder.

Retire the old embedded Servo 0.2 path from default dogfood instead. The default
browser runtime is the ServoShell 0.3 bridge:

```text
saccade-servoshell -> source/official ServoShell 0.3.x -> Saccade MCP/control
```

The legacy embedded `saccade-shell` remains only for old regression checks until
its remaining gates are either ported or explicitly deleted.

## Why

AI-012 and AI-008D changed the tradeoff:

- ServoShell 0.3 is the browser UI/runtime path that users should see.
- Source-release ServoShell passes the local game reflex gate with Saccade's
  in-process readback/control bridge.
- The legacy embedded `servo=0.2.0` path drags a second Servo stack into
  release builds and still has known rendering/UI drift.
- Directly upgrading `crates/saccade_browser` would be a heavy API migration
  while keeping the product on the wrong browser shell.

Therefore the migration is not "make old Saccade shell newer"; it is "move
remaining Saccade product gates onto ServoShell bridge/fork and stop defaulting
to the old shell."

## Inventory

Direct Servo 0.2 dependency:

```text
crates/saccade_browser/Cargo.toml -> servo = "=0.2.0"
```

Transitive users:

```text
bins/saccade-shell -> saccade_browser
bins/devmax        -> saccade_browser
bins/formmax       -> saccade_browser
bins/mousemax      -> saccade_browser
```

Already free of embedded Servo 0.2:

```text
bins/saccade-mcp
bins/saccade-servoshell
crates/saccade_core
crates/saccade_detect
crates/saccade_motor
crates/saccade_replay
crates/saccade_verify
```

## Current Change

`scripts/build_dogfood_release.sh` no longer builds `saccade-shell` by default.
The local dogfood kit now builds:

```text
saccade-mcp
saccade-servoshell
```

and `open-saccade` launches:

```text
saccade-servoshell bridge --no-headless --url <URL>
```

To include the legacy shell for old regression checks:

```sh
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
```

## Porting Plan

1. Keep `saccade-servoshell` as the default dogfood browser launcher.
2. Port remaining `saccade-shell browser-session-worker` MCP uses to the
   ServoShell bridge grant/control endpoint.
3. Port FORMMAX and form safety gates to the ServoShell bridge path first.
   These already have evidence; keep the old `formmax` binary only as a local
   regression fixture until parity is complete.
4. Port DEVMax browser-backed audits to the ServoShell bridge or Chrome
   reference engine. Keep the old embedded Servo probe only when comparing
   historical regressions.
5. Keep MOUSEMAX/local-game ms loops on the source ServoShell reflex bridge.
   Do not route reflex gates through WebDriver.
6. After every product gate has a ServoShell/Chrome/reference replacement,
   remove `saccade_browser` from default workspace flows and archive the old
   embedded shell docs as historical.

## Exit Criteria

The embedded Servo 0.2 path can be removed from default development when:

- Dogfood release builds without `saccade_browser`.
- MCP selftest passes through ServoShell bridge, not `browser-session-worker`.
- FORMMAX live fill has a ServoShell bridge gate.
- DEVMAX browser-backed evidence has a ServoShell or Chrome route.
- Local game reflex gate uses source-release ServoShell bridge.
- Docs no longer tell other sessions to use `cargo run -p saccade-shell` as
  the preferred dogfood entrypoint.

## Non-Goals

- Do not `cargo update -p servo`.
- Do not replace official ServoShell UI with legacy GL chrome.
- Do not fork Servo layout/rendering/network to chase parity unless a measured
  product gate proves the issue is Saccade-owned and not upstream/site-specific.
