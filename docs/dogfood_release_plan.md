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
  check-saccade
  open-saccade
  servoshell-bridge
  read-article
  run-formmax
  run-local-game-reflex
  DOGFOOD_STATUS.md
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

`open-saccade` first shows a local Saccade launch page, prints immediate
terminal status, then navigates that same ServoShell bridge session to the
target URL. On macOS headed launches it also makes a best-effort foreground /
position / resize call for the ServoShell process.

`open-saccade` uses the bundled persistent `profile/default/` directory so a
human login can be reused by later bridge/co-pilot runs from the same kit. It
does not import Chrome/Safari/Firefox cookies.

Check the kit:

```bash
dist/saccade-dogfood-<timestamp>/check-saccade
```

`check-saccade` prints human-readable status to stderr and JSON to stdout, so
other sessions can pipe it to `jq`.

Run the official ServoShell bridge manually:

```bash
dist/saccade-dogfood-<timestamp>/servoshell-bridge --smoke
```

The generated wrappers default to the package-local `profile/default/`,
`current_tab_grant.json`, and `runs/` paths unless the caller explicitly passes
an override.

Read a public tutorial/article page and exit with JSON:

```bash
dist/saccade-dogfood-<timestamp>/read-article https://example.com/tutorial
```

Run the local-game reflex gate:

```bash
dist/saccade-dogfood-<timestamp>/run-local-game-reflex http://127.0.0.1:4173/
```

Run the local FORMMAX long-form gate:

```bash
dist/saccade-dogfood-<timestamp>/run-formmax
```

Latest verification:

```text
dist/saccade-dogfood-ai016-20260619-204157/
dist/saccade-dogfood-current -> saccade-dogfood-ai016-20260619-204157
dist/saccade-dogfood-ai016-20260619-204157/runs/check/bridge_smoke/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/servoshell_bridge/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/article/ai016_rookies_article_final/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/formmax/ai017_formmax_wrapper/result.json
docs/ai018_dogfood_launch_visibility.md
```

Result:

```text
default kit: no bin/saccade-shell
check-saccade: PASS, JSON stdout, package-local profile/grant/output paths
manual bridge smoke: PASS, package-local profile/grant/output paths
article one-shot: Rookies tutorial page -> title ok, url ok, 9392 chars, selector main.layout-content
run-formmax: PASS, rows=96, pages=2, filled=672, blocked_sensitive=3
open-saccade launch visibility: PASS, visible_bootstrap=true, foreground_attempted=true
process shutdown: graceful_servo_shutdown
```

The article one-shot still reports known Servo page warnings such as the macOS
GL texture warning and missing `IntersectionObserver`, but the article truth
surface remains usable and exits cleanly.

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
