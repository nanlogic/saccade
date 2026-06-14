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
  - `Cmd+R`: reload,
  - `Cmd+[`: back,
  - `Cmd+]`: forward.

This is a first shell stage, not the final browser chrome. It gives dogfood users enough visible state to know where the agent is acting without squeezing or overlaying page content.

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

- Editable URL bar.
- Clickable Back, Forward, Reload, and Stop controls.
- Visible page/chrome focus recovery.
- Error state beyond load-state text.

Ledger: BP-003 remains `investigating` until the clickable toolbar exists.
