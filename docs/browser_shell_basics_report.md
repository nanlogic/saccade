# Browser Shell Basics Report

Date: 2026-06-14

## What Changed

- Saccade dogfood windows now show browser state in the native title bar:
  - rendering profile,
  - load state,
  - Back/Forward availability,
  - page title,
  - current URL.
- Active `<select>` popovers keep the current URL visible while showing the selected option and shortcut hint.
- Existing keyboard browser controls remain:
  - `Cmd+L`: focus the address command in the native title bar,
  - `Cmd+R`: reload,
  - `Cmd+[`: back,
  - `Cmd+]`: forward.
- Mouse Back/Forward buttons now navigate browser history when the hardware exposes them.
- Address command mode keeps page layout untouched:
  - type a URL in the title bar prompt,
  - bare domains such as `ign.com` become `https://ign.com`,
  - localhost-style inputs such as `localhost:3000` become `http://localhost:3000`,
  - Enter opens the URL,
  - Esc cancels.
- Page mouse presses now recover page focus from shell modes:
  - clicking the page cancels the title-bar address command,
  - clicking the page dismisses an active native `<select>` handoff,
  - the same click is still forwarded to Servo.

This is still a first shell stage, not the final browser chrome. It gives dogfood users enough visible state and direct navigation to know where the agent is acting without squeezing or overlaying page content.

## Verification

Commands:

```sh
cargo test -p saccade_browser shell_title
cargo check -p saccade-shell
cargo run -p saccade-shell -- browse --url https://example.com --width 900 --height 650 --smoke-seconds 2 --rendering-profile servo-modern
```

macOS title smoke on the local form fixture returned:

```text
Saccade [servo-modern] load=complete back=n fwd=n | Parity Form Controls | file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/visual_parity/form_controls/index.html
```

## Still Open

- Clickable editable URL bar. The temporary address command is keyboard-only through `Cmd+L`.
- Visible clickable Back, Forward, Reload, and Stop controls.
- Visible chrome affordance for focus recovery and active shell mode.
- Error state beyond load-state text.

Ledger: BP-003 remains `investigating` until the clickable toolbar exists.
