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
  profile-status
  clear-profile
  open-saccade
  servoshell-bridge
  read-article
  run-formmax
  run-local-game-reflex
  DOGFOOD_STATUS.md
  current_tab_grant.json
  saccade-dogfood.env
  profile/default/        # legacy/empty per-kit fallback, not the default login profile
  userscripts/
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

`open-saccade` uses a stable Saccade profile at
`runs/dogfood_profile/default` by default, so a human login can be reused across
new dogfood kit builds and later bridge/co-pilot runs. This is closer to Chrome's
"same profile stays logged in" behavior. It does not import Chrome/Safari/Firefox
cookies. Use a named local profile with:

```bash
SACCADE_PROFILE_NAME=work dist/saccade-dogfood-current/open-saccade https://example.com
```

This resolves under `runs/dogfood_profile/work`. Override the full path with:

```bash
SACCADE_PROFILE_DIR=/path/to/profile dist/saccade-dogfood-current/open-saccade https://example.com
```

Profile ownership rule: this profile is human browser state. The agent may
attach to the current tab after an explicit grant and receive redacted
truth/actions, but it must not receive the raw cookie jar, password data,
storage dumps, or sensitive field values.

Check the current profile and grant file without launching a browser:

```bash
dist/saccade-dogfood-current/profile-status
```

Clear a normal Saccade profile explicitly:

```bash
dist/saccade-dogfood-current/clear-profile --dry-run
dist/saccade-dogfood-current/clear-profile --yes
```

`clear-profile` signs sites out by deleting the current normal profile contents.
It reports counts and bytes only; it never prints cookie or storage values. It
refuses custom `SACCADE_PROFILE_DIR` paths unless `--force-custom` is supplied.

The source ServoShell dogfood chrome also exposes a safer interactive version:
click the `Normal` / `Incognito` / `Profile: <name>` badge, review the profile
state and agent boundary, then request `Clear this profile on quit`. The browser
writes only a small clear request. The dogfood wrapper applies it after the
browser exits, refuses custom paths, and reports counts/bytes only.

Incognito/ephemeral browsing is available through the wrapper layer:

```bash
SACCADE_INCOGNITO=1 dist/saccade-dogfood-current/open-saccade https://example.com
SACCADE_PROFILE_MODE=incognito dist/saccade-dogfood-current/check-saccade
```

The wrapper creates a marked temporary profile under the kit's `runs/incognito/`
directory and deletes it when the command exits. Use this for untrusted checks,
logged-out comparison, or no-persistence dogfood. Agent grants inside incognito
still use the same redacted truth/action boundary.

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

The generated wrappers default to the stable `SACCADE_PROFILE_DIR`,
package-local `current_tab_grant.json`, and package-local `runs/` paths unless
the caller explicitly passes an override.

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

Run the public no-login site smoke matrix:

```bash
dist/saccade-dogfood-<timestamp>/run-public-site-smoke-matrix
```

Run the larger public-read exploratory matrix:

```bash
dist/saccade-dogfood-<timestamp>/run-public-site-smoke-matrix extended --matrix extended
```

Latest verification:

```text
dist/saccade-dogfood-ai021-profile-final-20260705/
dist/saccade-dogfood-current -> saccade-dogfood-ai021-profile-final-20260705
runs/profile_productization/ai021_profile_commands_final_20260705/
runs/profile_productization/ai021_check_saccade_final_20260705/check_saccade.json
runs/profile_productization/ai021_incognito_check_final_20260705/check_saccade_incognito.json
runs/ai021_profile_badge/profile_badge_final_20260705/browser_chrome.png
runs/ai021_profile_badge/profile_badge_final_20260705/smoke_stdout.json
runs/ai021_profile_finalize/clear_on_quit_cleanup_final_20260705/summary.json
runs/ai023_public_site_matrix/default_20260705/report.json
runs/ai024_public_site_matrix/extended_20260705/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/servoshell_bridge/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/article/ai016_rookies_article_final/report.json
dist/saccade-dogfood-ai016-20260619-204157/runs/formmax/ai017_formmax_wrapper/result.json
dist/saccade-dogfood-current/runs/article/uscis_i797_forced_fallback3/report.json
docs/ai018_dogfood_launch_visibility.md
docs/ai019_public_evidence_pack.md
docs/ai020_human_in_loop_site_matrix.md
```

Result:

```text
default kit: no bin/saccade-shell
check-saccade: PASS, JSON stdout, package-local profile/grant/output paths
profile-status: PASS, JSON stdout, no cookie/storage values, reports profile mode/name/persistence/grant file
clear-profile: PASS on disposable named profile, dry-run and --yes paths verified, custom path requires --force-custom
normal profile check: profile_mode=normal, profile_persistent=true, profile_dir=runs/dogfood_profile/default
named profile check: SACCADE_PROFILE_NAME=work resolves to runs/dogfood_profile/work
browser chrome profile badge: PASS, internal ServoShell chrome screenshot shows separate `Profile: work` and `Copilot` badges
browser chrome profile panel: PASS, clicking the profile badge can request clear-on-quit; wrapper applies the request after browser exit and keeps raw cookies/storage hidden
public site smoke matrix: PASS, 4/4 public no-login sites with same-WebView control and graceful shutdown
public extended read matrix: PASS, 8/8 public read-only sites; adds GitHub, Gist, Stack Overflow, and Reddit
real GitHub profile reuse: after one human login, reopened bridge reached https://gist.github.com/new with title "Create a new Gist" and route=usable_ignore_hidden_backing_fields
GitHub/Gist CodeMirror userscript: PASS, shim=saccade_github_codemirror_input_shim_v1, visible Saccade caret/focus ring, textValuesLogged=false
incognito profile check: profile_mode=incognito, profile_persistent=false, temporary profile removed after exit
manual bridge smoke: PASS, package-local profile/grant/output paths
article one-shot: Rookies tutorial page -> title ok, url ok, 9392 chars, selector main.layout-content
article fallback: forced USCIS timeout -> route=http_article_fallback, cookies_sent=false, profile_used=false, text_chars=5620, has_i797c=true, has_biometric=true
run-formmax: PASS, rows=96, pages=2, filled=672, blocked_sensitive=3
open-saccade launch visibility: PASS, visible_bootstrap=true, foreground_attempted=true
process shutdown: graceful_servo_shutdown
```

The article one-shot still reports known Servo page warnings such as the macOS
GL texture warning and missing `IntersectionObserver`, but the article truth
surface remains usable and exits cleanly.

`read-article` now has a bounded public-page fallback. The normal browser path
is still preferred and remains green on the Rookies tutorial page. If the
browser path times out or exits nonzero, the wrapper kills the browser process
group and returns `route=http_article_fallback`. That fallback sends no browser
cookies and does not use the persisted Saccade profile; it is for public
reference pages only.

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
