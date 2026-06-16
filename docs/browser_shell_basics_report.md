# Browser Shell Basics Report

Date: 2026-06-14
Updated: 2026-06-15

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
- Dogfood windows now paint a native GL toolbar overlay above the page:
  - Back,
  - Forward,
  - Reload,
  - address command hit-zone,
  - Copilot grant hit-zone.
- Toolbar clicks are consumed by the shell and are not forwarded into the page,
  so page truth/action maps stay free of injected toolbar DOM.
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
- The dogfood same-WebView control endpoint now exposes shell navigation:
  - `shell_status`,
  - `navigate`,
  - `reload`,
  - `back`,
  - `forward`.
- MCP now exposes the same already-granted visible-tab shell navigation through
  `saccade.browser.navigate`:
  - `status`,
  - `navigate`,
  - `reload`,
  - `back`,
  - `forward`.
- The named MCP tool is restricted to Human-owned tabs with explicit agent input
  grant and a same-WebView dogfood control endpoint; it does not inject DOM into
  the page.

This is still a first shell stage, not the final browser chrome. It gives dogfood users enough visible state and direct navigation to know where the agent is acting without injecting browser UI into the page.

## Verification

Commands:

```sh
cargo test -p saccade_browser shell_title
cargo check -p saccade-shell
cargo run -p saccade-shell -- browse --url https://example.com --width 900 --height 650 --smoke-seconds 2 --rendering-profile servo-modern
RUST_LOG=error cargo run -q -p saccade-shell -- browse --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/current_tab_copilot/index.html --width 900 --height 650 --smoke-seconds 8 --rendering-profile servo-modern
```

macOS title smoke on the local form fixture returned:

```text
Saccade [servo-modern] load=complete back=n fwd=n | Parity Form Controls | file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/visual_parity/form_controls/index.html
```

Same-WebView shell navigation smoke:

```text
SHELL_NAV PASS runtime=saccade-dogfood-control-v0 initial=current_tab_copilot navigated=formmax reload_changed=true back_changed=true forward_changed=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/same_webview_shell_nav_smoke_1781579239152.json grant=/Users/waynema/Documents/GitHub/SACCADE/runs/current_tab_grants/mcp_shell_nav_smoke.json
```

MCP named browser navigation gate:

```text
MCP PASS tools_registered=21 tab_scoping=true local_dev_audit=true policy_gate=true report=/Users/waynema/Documents/GitHub/SACCADE/runs/mcp/selftest_1781583895286/report.json
```

Visible toolbar smoke screenshot:

```text
runs/browser_shell/visible_toolbar_file_smoke.png
```

## Still Open

- Stop button and final loading/stop behavior.
- URL text is still edited through the title-bar address prompt; the toolbar
  address strip is a clickable hit-zone, not a fully painted text editor yet.
- The v0 toolbar overlays the top 44 CSS px of page content. A final browser
  chrome should use a compositor/viewport arrangement that does not obscure the
  page.
- More polished visible chrome affordance for focus recovery and active shell
  mode.
- Error state beyond load-state text.
- `saccade.browser.navigate` is v0 and intentionally only targets already
  granted same-WebView dogfood tabs; broader browser/session routing is still a
  product API question.

Ledger: BP-003 is partially mitigated by the native clickable toolbar v0 and
remains `investigating` until the final non-obscuring browser chrome exists.
