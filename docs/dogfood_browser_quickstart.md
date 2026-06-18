# Saccade Dogfood Browser Quickstart

Date: 2026-06-16

## What exists now

Saccade now has a macOS-friendly dogfood browser shell:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://example.com
```

It opens one Servo-backed Saccade window at `1440x1000` by default. Dogfood uses the `servo-modern` rendering profile, which currently enables Servo's measured CSS Grid pref. You can click, scroll, type into ordinary fields, use basic `<select>` controls, and use the visible address bar. Close the window to exit.

## Easy mac launcher

From Finder, double-click:

```text
scripts/saccade-open.command
```

It asks for a URL and then launches the same browser shell.

From Terminal:

```bash
./scripts/saccade-open.command
```

## Useful commands

Open a site:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://mouseaccuracy.com/classic/
```

Open a larger window:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://example.com --width 1920 --height 1080
```

Open with a persistent Saccade profile:

```bash
mkdir -p runs/dogfood_profile/default
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://gist.github.com --profile-dir runs/dogfood_profile/default
```

Use the same profile for an agent worker:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browser-session-worker --url https://gist.github.com/new --profile-dir runs/dogfood_profile/default
```

This shares Saccade-owned cookies/storage across Saccade processes. It does not import Chrome/Safari/Firefox cookies. For Google/GitHub login, log in inside Saccade with the persistent profile, then reuse that same profile path for later worker sessions.

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
- Persistent `--profile-dir` is supported for Saccade-owned session reuse, but there is not yet a friendly profile picker or password-manager flow.
- Canvas/WebGL-heavy pages can hit current Saccade/Servo canvas/runtime issues on this machine. Full-window Canvas2D can reproduce missing captured layers even without GL warnings. If logs show `GLD_TEXTURE_INDEX_2D is unloadable`, canvas/WebGL is extremely slow, or the page cannot be judged in Saccade, stop that run, record it as a Saccade runtime blocker, and validate with Chrome/reference instead.
- Visual parity with Chrome/Safari is still tracked separately. Use this shell for dogfood, and use Chrome reference captures when exact mainstream rendering matters.
- `servo-modern` improves action/layout correctness for current local gates, but it is not a claim that Servo renders like Chrome.
