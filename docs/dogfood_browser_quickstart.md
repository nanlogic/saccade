# Saccade Dogfood Browser Quickstart

Date: 2026-06-16

## What exists now

Saccade's preferred dogfood browser path is now the ServoShell 0.3 bridge:

```bash
./scripts/build_dogfood_release.sh
dist/saccade-dogfood-<timestamp>/open-saccade https://example.com
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

It asks for a URL and launches the legacy shell. Prefer the release-kit
`open-saccade` command above for current dogfood.

From Terminal:

```bash
./scripts/saccade-open.command
```

## Useful commands

Open a site:

```bash
dist/saccade-dogfood-<timestamp>/open-saccade https://mouseaccuracy.com/classic/
```

Run a bridge smoke:

```bash
dist/saccade-dogfood-<timestamp>/servoshell-bridge --smoke
```

Read a public article/tutorial page, wait for content, emit JSON, and exit:

```bash
dist/saccade-dogfood-<timestamp>/read-article \
  https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art
```

This is the preferred A/B path for long learning pages. It uses the same
ServoShell bridge, extracts `article_text`, records the report under
`dist/saccade-dogfood-<timestamp>/runs/article/<name>/report.json`, and exits
instead of leaving a live browser session running.

Run the local game reflex gate:

```bash
dist/saccade-dogfood-<timestamp>/run-local-game-reflex http://127.0.0.1:4173/
```

Legacy embedded shell, only when you need an old regression check:

```bash
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
dist/saccade-dogfood-<timestamp>/open-legacy-saccade https://example.com
```

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
  dogfood release `open-saccade` wrapper uses its bundled `profile/default/`
  by default. There is not yet a friendly profile picker or password-manager
  flow, and it does not import Chrome/Safari/Firefox cookies.
- Canvas/WebGL-heavy pages can hit current Saccade/Servo canvas/runtime issues on this machine. Full-window Canvas2D can reproduce missing captured layers even without GL warnings. If logs show `GLD_TEXTURE_INDEX_2D is unloadable`, canvas/WebGL is extremely slow, or the page cannot be judged in Saccade, stop that run, record it as a Saccade runtime blocker, and validate with Chrome/reference instead.
- Visual parity with Chrome/Safari is still tracked separately. Use this shell for dogfood, and use Chrome reference captures when exact mainstream rendering matters.
- `servo-modern` improves action/layout correctness for current local gates, but it is not a claim that Servo renders like Chrome.
