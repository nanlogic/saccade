# Saccade Dogfood Release Plan

Date: 2026-06-16

## Decision

Make a **local dogfood release kit** now. Do not call it a public release and do
not block on a notarized `.app` bundle yet.

Default the dogfood kit to **ServoShell 0.3 bridge**, not the legacy embedded
`servo=0.2.0` shell. The legacy shell is opt-in for historical regression
checks only.

Use a **distinct Saccade icon** for Saccade builds. Do not reuse the official
Servo app icon unless the Servo project explicitly grants that use. Product copy
can say "Powered by Servo" or "Uses the Servo engine" where appropriate.

Why:

- Other Codex sessions need a stable dogfood entrypoint that does not depend on
  `cargo run` debug paths.
- Official Servo is an embeddable engine, but Servo project marks/logos are
  project trademarks; Saccade should not look like the official Servo browser.
- Apple distribution outside the App Store eventually needs signing and
  notarization; that is a public-distribution step, not required for same-machine
  dogfood.

## Local Dogfood Kit

Build:

```bash
./scripts/build_dogfood_release.sh
```

The script writes:

```text
dist/saccade-dogfood-<timestamp>/
  bin/saccade-mcp
  bin/saccade-servoshell
  open-saccade
  servoshell-bridge
  read-article
  run-local-game-reflex
  current_tab_grant.json
  saccade-dogfood.env
  profile/default/
  docs/
```

By default it does **not** build or copy `bin/saccade-shell`, because that pulls
in the old embedded Servo 0.2 stack. To include it explicitly:

```bash
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
```

Open a site:

```bash
dist/saccade-dogfood-<timestamp>/open-saccade https://example.com
```

Run the official ServoShell bridge:

```bash
dist/saccade-dogfood-<timestamp>/servoshell-bridge --smoke
```

Read a public tutorial/article page and exit with JSON:

```bash
dist/saccade-dogfood-<timestamp>/read-article https://example.com/tutorial
```

Run the local-game reflex gate:

```bash
dist/saccade-dogfood-<timestamp>/run-local-game-reflex http://127.0.0.1:4173/
```

Latest verification:

```text
dist/saccade-dogfood-test-ai014/
runs/dogfood_release/ai014_bridge_smoke/report.json
runs/local_game_reflex/ai014_kit_reflex_smoke/report.json
runs/dogfood_release/article_rookies_smoke_20260619/report.json
```

Result:

```text
default kit: no bin/saccade-shell
bridge smoke: PASS
local-game reflex wrapper: live_game_reflex_readback_green
article one-shot: Rookies tutorial page -> title ok, url ok, 9392 chars, selector main.layout-content
```

Servo 0.2 retirement details:

```text
docs/servo_0_2_retirement_plan.md
```

## Icon Policy

Dogfood CLI kit:

- No app icon required.
- Terminal/window title may say `Saccade Dogfood`.

Future `.app` bundle:

- Bundle name: `Saccade Dogfood` for internal builds.
- Bundle id: `ai.nanlogic.saccade.dogfood` or equivalent.
- Icon: distinct Saccade icon, not the official Servo icon.
- About text: "Saccade dogfood browser. Powered by Servo."

Public release later:

- Use a polished Saccade icon that remains legible at small sizes.
- Do not use Servo branding as the primary product identity.
- Add Developer ID signing, notarization, staple verification, and a clean
  uninstall/profile story.

## Sources

- Servo describes itself as an embeddable web engine: https://servo.org/
- Servo project charter says project trademarks are held by LF Europe:
  https://github.com/servo/project/blob/main/governance/CHARTER.md
- Apple app icon guidance: https://developer.apple.com/design/human-interface-guidelines/app-icons
- Apple macOS notarization guidance:
  https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
