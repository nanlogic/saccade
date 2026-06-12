# Saccade Dogfood Browser Quickstart

Date: 2026-06-12

## What exists now

Saccade now has a macOS-friendly dogfood browser shell:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url https://example.com
```

It opens one Servo-backed Saccade window at `1440x1000` by default. You can click, scroll, type into ordinary fields, and use basic `<select>` controls. Close the window to exit.

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

Compile/smoke check without leaving a window open:

```bash
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url about:blank --smoke-seconds 1
```

## Current controls

- Mouse move, click, and wheel scroll are forwarded into Servo.
- Keyboard text entry is forwarded into focused inputs.
- `Cmd+R` reloads.
- `Cmd+[` goes back.
- `Cmd+]` goes forward.
- For a native `<select>` handoff, use Up/Down and Enter; Esc dismisses.

## Known limits

- This is a Saccade dogfood shell, not a packaged `.app` yet.
- There is no address bar or tabs yet; launch with a URL.
- File picker, native context menu, clipboard, downloads, and password-manager UX are not implemented.
- Visual parity with Chrome/Safari is still tracked separately. Use this shell for dogfood, and use Chrome reference captures when exact mainstream rendering matters.
