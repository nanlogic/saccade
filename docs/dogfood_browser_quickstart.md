# Saccade Dogfood Browser Quickstart

Date: 2026-06-12

## What exists now

Saccade now has a macOS-friendly dogfood browser shell:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://example.com
```

It opens one Servo-backed Saccade window at `1440x1000` by default. Dogfood uses the `servo-modern` rendering profile, which currently enables Servo's measured CSS Grid pref. You can click, scroll, type into ordinary fields, use basic `<select>` controls, and open another URL from the same window with `Cmd+L`. Close the window to exit.

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
python3 scripts/probe_canvas_reductions.py --variants static dpr animated hud --wait-sec 2 --timeout-sec 75
```

## Current controls

- Mouse move, click, and wheel scroll are forwarded into Servo.
- Keyboard text entry is forwarded into focused inputs.
- `Cmd+L` opens the address command in the native title bar. Type a URL, press Enter to open it, or press Esc to cancel.
- `Cmd+R` reloads.
- `Cmd+[` goes back.
- `Cmd+]` goes forward.
- For a native `<select>` handoff, use Up/Down and Enter; Esc dismisses.

## Known limits

- This is a Saccade dogfood shell, not a packaged `.app` yet.
- There is no clickable address bar or tabs yet; use `Cmd+L` for keyboard URL entry.
- File picker, native context menu, clipboard, downloads, and password-manager UX are not implemented.
- Persistent `--profile-dir` is supported for Saccade-owned session reuse, but there is not yet a friendly profile picker or password-manager flow.
- Canvas/WebGL-heavy pages can hit current Saccade/Servo canvas/runtime issues on this machine. Full-window Canvas2D can reproduce missing captured layers even without GL warnings. If logs show `GLD_TEXTURE_INDEX_2D is unloadable`, canvas/WebGL is extremely slow, or the page cannot be judged in Saccade, stop that run, record it as a Saccade runtime blocker, and validate with Chrome/reference instead.
- Visual parity with Chrome/Safari is still tracked separately. Use this shell for dogfood, and use Chrome reference captures when exact mainstream rendering matters.
- `servo-modern` improves action/layout correctness for current local gates, but it is not a claim that Servo renders like Chrome.
