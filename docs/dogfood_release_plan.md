# Saccade Dogfood Release Plan

Date: 2026-06-16

## Decision

Make a **local dogfood release kit** now. Do not call it a public release and do
not block on a notarized `.app` bundle yet.

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
  bin/saccade-shell
  bin/saccade-mcp
  bin/saccade-servoshell
  open-saccade
  servoshell-bridge
  saccade-dogfood.env
  profile/default/
  docs/
```

Open a site:

```bash
dist/saccade-dogfood-<timestamp>/open-saccade https://example.com
```

Run the official ServoShell bridge:

```bash
dist/saccade-dogfood-<timestamp>/servoshell-bridge --smoke
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
