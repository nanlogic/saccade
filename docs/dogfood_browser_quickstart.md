# Saccade Dogfood Browser Quickstart

Date: 2026-07-01

## What exists now

Saccade's preferred dogfood browser path is now the ServoShell 0.3 bridge:

```bash
./scripts/build_dogfood_release.sh
dist/saccade-dogfood-current/open-saccade https://example.com
```

It opens ServoShell with the Saccade bridge attached, writes a current-tab grant,
and keeps the official/source ServoShell browser UI as the human-visible layer.
The legacy embedded `saccade-shell browse` path still exists for regression
checks, but it is not the default dogfood path because it pulls in the old
embedded `servo=0.2.0` stack.

## Easy mac launcher

From Finder, double-click:

```text
scripts/saccade-open.command
```

It asks for a URL and launches the legacy shell. This is kept only as a
fallback convenience. Prefer the release-kit `open-saccade` command above for
current dogfood.

From Terminal:

```bash
./scripts/saccade-open.command
```

## Useful commands

Open a site:

```bash
dist/saccade-dogfood-current/open-saccade https://mouseaccuracy.com/classic/
```

By default this uses the stable normal dogfood profile:

```text
runs/dogfood_profile/default
```

That profile is the human browser profile. Login cookies and site storage can
survive new dogfood builds when the site permits it, while the agent only sees
explicitly granted redacted truth/actions.

Check the active profile/grant state without launching a browser:

```bash
dist/saccade-dogfood-current/profile-status
```

Use a named local profile:

```bash
SACCADE_PROFILE_NAME=work dist/saccade-dogfood-current/open-saccade https://example.com
```

Clear the current normal profile only when you intentionally want to sign sites
out:

```bash
dist/saccade-dogfood-current/clear-profile --dry-run
dist/saccade-dogfood-current/clear-profile --yes
```

`clear-profile` reports counts and bytes only; it never prints cookies or
storage values. Custom profile paths require `--force-custom`.

For a throwaway/incognito-style run:

```bash
SACCADE_INCOGNITO=1 dist/saccade-dogfood-current/open-saccade https://example.com
SACCADE_PROFILE_MODE=incognito dist/saccade-dogfood-current/check-saccade
```

Incognito wrappers create a marked temporary profile under the kit's
`runs/incognito/` directory and delete it when the command exits. This is useful
for logged-out comparison and untrusted browsing checks.

Run a bridge smoke:

```bash
dist/saccade-dogfood-current/check-saccade
```

Read a public article/tutorial page, wait for content, emit JSON, and exit:

```bash
dist/saccade-dogfood-current/read-article \
  https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art
```

This is the preferred A/B path for long learning pages. It uses the same
ServoShell bridge, extracts `article_text`, records the report under
`dist/saccade-dogfood-<timestamp>/runs/article/<name>/report.json`, and exits
instead of leaving a live browser session running.

If the Saccade browser article path hangs or exits nonzero, `read-article`
kills the browser process group and emits a bounded public HTTP fallback packet
with `route=http_article_fallback`. The fallback sends no browser cookies and
does not use the persisted Saccade profile. Disable it with
`SACCADE_READ_ARTICLE_FALLBACK=off` when you need a strict browser-only test.

Run the no-login public-site smoke matrix:

```bash
dist/saccade-dogfood-current/run-public-site-smoke-matrix
```

This opens a small sequential public matrix through Saccade, writes per-site
reports, and does not log in, fill, submit, or bypass provider controls.

Run the larger public-read matrix before handing the kit to another session:

```bash
dist/saccade-dogfood-current/run-public-site-smoke-matrix extended --matrix extended
```

The extended matrix includes public GitHub/Gist, Stack Overflow, and Reddit
read-only pages. Passing it does not imply logged-in drafting or posting.

Run the local game reflex gate:

```bash
dist/saccade-dogfood-current/run-local-game-reflex http://127.0.0.1:4173/
```

Run a real-site human-in-loop draft measurement:

```bash
printf 'Saccade AI-020 draft rehearsal. Human will review and decide whether to submit.\n' > /tmp/saccade-draft.txt
dist/saccade-dogfood-current/run-ai020-live-draft \
  --site hn_comment \
  --url https://news.ycombinator.com/item?id=48706714 \
  --body-file /tmp/saccade-draft.txt \
  --manual-gate
```

For an issue/discussion-style title + body draft:

```bash
printf 'Saccade draft issue title\n' > /tmp/saccade-title.txt
printf 'Saccade draft issue body. Human owns submit.\n' > /tmp/saccade-body.txt
dist/saccade-dogfood-current/run-ai020-live-draft \
  --site github_issue \
  --draft-profile github_issue \
  --url <new_issue_url> \
  --title-file /tmp/saccade-title.txt \
  --body-file /tmp/saccade-body.txt \
  --manual-gate
```

This launches the visible ServoShell bridge, waits for the human when
`--manual-gate` is set, calls `inspect_editors` and `draft_editor_fill`, writes a
redacted AI-020 report, and verifies draft values do not appear in the report or
control replay. It never clicks submit/publish. In visible `--manual-gate` mode
it stops again after filling so the human can inspect the draft before pressing
Enter to close the browser.

Draft profiles only map user-facing names such as `title` or `comment` onto the
existing safe bridge slots. They do not allow arbitrary form fill.

Legacy embedded shell, only when you need an old regression check:

```bash
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
dist/saccade-dogfood-<timestamp>/open-legacy-saccade https://example.com
```

## Current Verified Kit

Latest local dogfood kit:

```text
dist/saccade-dogfood-ai021-profile-final-20260705/
dist/saccade-dogfood-current -> saccade-dogfood-ai021-profile-final-20260705
Saccade commit: 138c9b4
ServoShell source fork: 2ac8f98d7
```

Verification:

```text
check-saccade: PASS
runtime: official_servoshell_webdriver
profile_mode: normal
profile_persistent: true
smoke title: Browser Session Smoke
smoke.same_webview_control: true
process.termination: graceful_servo_shutdown
control report: dist/saccade-dogfood-current/runs/check/bridge_smoke/control/report.json

profile product controls: PASS
profile-status: JSON stdout, no cookie/storage values
browser chrome: trusted Profile and Copilot badges
clear-on-quit: PASS, pending request applied after browser exit
artifact: runs/ai021_profile_finalize/clear_on_quit_cleanup_final_20260705/summary.json

read-article Rookies: PASS
title: Step-by-Step Guide to Modular Environment Art: From Blender to UE5 | The Rookies Blog
bodyTextLength: 9680
article_text_length: 9352
selector: main.layout-content
termination: graceful_servo_shutdown
artifact: dist/saccade-dogfood-current/runs/article/rookies_20260701/report.json

run-ai020-live-draft local fixture: PASS
read_status: pass
draft_status: pass
control artifacts: present
value_leak_check: pass, including final_report_candidate
artifact: runs/ai020_live/local_forum_fixture_review_release/report.json
```

Known warning during these green routes:

```text
UNSUPPORTED ... GLD_TEXTURE_INDEX_2D ...
```

Treat that warning as monitored noise for the current article/check routes
unless the page is visibly slow, blank, or missing required Canvas/WebGL state.

The legacy profile path shares Saccade-owned cookies/storage across Saccade
processes. It does not import Chrome/Safari/Firefox cookies. For Google/GitHub
login, prefer the ServoShell bridge handoff flow unless a legacy regression
explicitly needs `browser-session-worker`.

Probe editor routability without typing or publishing:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- inspect-editors --url https://gist.github.com/new --profile-dir runs/dogfood_profile/default
```

If this reports `route_login_or_non_authoring_page`, the profile has probably not reached the real authoring editor yet.

Mark owned domains as first-party dogfood surfaces:

```bash
export SACCADE_OWNED_DOMAINS=nanmesh.ai,mythcastera.com,mysterypartynow.com
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://nanmesh.ai
```

The allowlist only changes normal owned sites to `owned_domain` Green in
Saccade policy reports. It does not override login, government, financial,
healthcare, cloud-console, payment, legal, security, CAPTCHA, or anti-abuse
blocks.

Open the pinned-default baseline profile:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://example.com --rendering-profile servo-safe
```

Compile/smoke check without leaving a window open:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url about:blank --smoke-seconds 1
```

Check the minimal WebGL runtime fixture:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-webgl-runtime
```

Check the live local WebGL game against Chrome:

```bash
python3 scripts/probe_webgl_game_runtime.py --url http://127.0.0.1:4173/ --wait-sec 3 --timeout-sec 75
```

Run the Canvas2D reductions:

```bash
# Default local fixture diagnostics use Servo WebView::take_screenshot().
python3 scripts/probe_canvas_reductions.py --variants static dpr animated hud --wait-sec 2 --timeout-sec 75
python3 scripts/probe_canvas_reductions.py --preset sizing --wait-sec 2 --timeout-sec 75
python3 scripts/probe_canvas_reductions.py --preset threshold --wait-sec 2 --timeout-sec 75
python3 scripts/probe_canvas_reductions.py --preset threshold-bare --repeat 2 --wait-sec 2 --timeout-sec 75
python3 scripts/probe_canvas_reductions.py --preset fill --repeat 2 --wait-sec 2 --timeout-sec 75
python3 scripts/probe_canvas_reductions.py --preset gradient --repeat 2 --wait-sec 2 --timeout-sec 75

# Force the low-latency manual readback path when testing the reflex/readback gate.
python3 scripts/probe_canvas_reductions.py --variants bare-gradient2-size-1152x648 --saccade-screenshot-mode manual --wait-sec 2 --timeout-sec 75
```

Run the source ServoShell reflex readback gate:

```bash
node scripts/probe_reflex_readback_canvas.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/debug/servoshell \
  --variant bare-gradient2-size-1152x648 \
  --duration-ms 2500 \
  --window-size 1440x900
```

## Current controls

- Mouse move, click, and wheel scroll are forwarded into Servo.
- Keyboard text entry is forwarded into focused inputs.
- The visible toolbar hit-zones handle Back, Forward, Reload, editable address
  entry, and current-tab Copilot grant without injecting page DOM.
- Click the address bar once to select the current URL. Click again while it is
  focused to move the caret within the URL. Type to replace the selection or
  insert at the caret.
- `Cmd+L` focuses/selects the address bar. Type a URL, press Enter to open it,
  or press Esc to cancel.
- `Cmd+A`, Backspace, Delete, arrow keys, Home, and End work inside the address
  bar.
- `Cmd+R` reloads.
- `Cmd+[` goes back.
- `Cmd+]` goes forward.
- `Cmd+Shift+G` grants the current visible dogfood tab to MCP co-pilot and
  writes `runs/current_tab_grants/latest.json`. MCP can then attach with
  `saccade.tabs.grant_current` and navigate that same tab with
  `saccade.browser.navigate`.
- For a native `<select>` handoff, use Up/Down and Enter; Esc dismisses.

## Known limits

- This is a Saccade dogfood shell, not a packaged `.app` yet.
- Tabs are not implemented in the legacy dogfood shell. Product browser UI is
  still planned to move onto official ServoShell or a thin ServoShell fork.
- File picker, native context menu, clipboard, downloads, and password-manager UX are not implemented.
- Persistent `--profile-dir` is supported for Saccade-owned session reuse. The
  dogfood release `open-saccade` wrapper now defaults to the stable
  `runs/dogfood_profile/default` profile so login can survive dogfood kit
  rebuilds. Override with `SACCADE_PROFILE_DIR=/path/to/profile`. Use
  `SACCADE_INCOGNITO=1` or `SACCADE_PROFILE_MODE=incognito` for temporary
  no-persistence dogfood. The ServoShell dogfood chrome shows the active profile
  badge and includes a clear-on-quit profile panel for normal named Saccade
  profiles. There is not yet a full profile picker/relaunch or password-manager
  flow, and it does not import Chrome/Safari/Firefox cookies.
- Canvas/WebGL-heavy pages can hit current Saccade/Servo canvas/runtime issues on this machine. Full-window Canvas2D can reproduce missing captured layers even without GL warnings. If logs show `GLD_TEXTURE_INDEX_2D is unloadable`, canvas/WebGL is extremely slow, or the page cannot be judged in Saccade, stop that run, record it as a Saccade runtime blocker, and validate with Chrome/reference instead.
- Visual parity with Chrome/Safari is still tracked separately. Use this shell for dogfood, and use Chrome reference captures when exact mainstream rendering matters.
- `servo-modern` improves action/layout correctness for current local gates, but it is not a claim that Servo renders like Chrome.
