# AI-018 Dogfood Launch Visibility

Date: 2026-06-19
Status: complete

## Problem

`open-saccade https://gist.github.com/new` could feel like no browser opened.
The process was alive, but GitHub/WebDriver readiness lagged enough that the
window/grant state was not obvious to the user.

## Fix

- Visible bridge launches now start ServoShell on a local
  `test_pages/servoshell_launch/index.html` page.
- After the WebDriver bridge attaches, Saccade navigates the same browser
  session to the requested target URL.
- On macOS headed launches, Saccade makes a best-effort `System Events`
  foreground/position/resize call for the ServoShell process.
- `open-saccade` prints immediate stderr status:
  target URL, local launch page note, and later `SACCADE_SERVOSHELL_BRIDGE READY`.

## Evidence

Command:

```bash
dist/saccade-dogfood-current/open-saccade https://gist.github.com/new
```

Result:

```text
stderr status appeared immediately
bridge ready within the observed 12s window
report launch.visible_bootstrap=true
report launch.foreground_attempted=true
report launch.browser_launch_url=file:///.../test_pages/servoshell_launch/index.html
grant url=https://gist.github.com/new
grant title=Create a new Gist
macOS frontmost=true
window title=Create a new Gist
window position=80,80
window size=1360x772
```

Artifact:

```text
dist/saccade-dogfood-current/runs/servoshell_bridge/report.json
dist/saccade-dogfood-current/current_tab_grant.json
```

## Remaining Caveat

Foreground activation is best-effort because macOS accessibility/windowing can
still depend on user permissions and Spaces. The user now gets immediate stderr
status plus the local launch page path in the report, so failures are observable
instead of silent.
