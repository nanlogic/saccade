# Gist Editor Probe Report

Date: 2026-06-14

## Goal

Retest BP-004 with the shared Saccade profile path and make real-site editor probing reusable without typing, publishing, or returning editor values.

## What Changed

- Added `saccade-shell inspect-editors`.
- The command runs the live `browser-session-worker`, sends `inspect_editors`, then exits.
- It supports `--profile-dir`, `--width`, `--height`, and `--rendering-profile`.
- Output includes editor kind, labels/placeholders, rects, sensitivity class, and `value_len`; it does not print editor values.
- Editor routing now separates writable controls from content-authoring editors, so a lone search box no longer produces a false green result.

## Verification

Local reduction:

```sh
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-editor-reduction
```

Result:

```text
EDITOR_REDUCTION PASS editors=6 zero_rect=2 sensitive=1 route=usable_ignore_hidden_backing_fields visible_body=true visible_shell=true replay=runs/browser_session_worker/worker_1781442830875_95837/replay.jsonl
```

Real Gist with shared profile:

```sh
RUST_LOG=error cargo run -q -p saccade-shell -- inspect-editors --url https://gist.github.com/new --profile-dir runs/dogfood_profile/default --width 1440 --height 900
```

Result:

```text
INSPECT_EDITORS PASS url=https://gist.github.com/new editors=1 zero_rect=0 visible_writable=1 visible_authoring=0 sensitive=0 route=route_login_or_non_authoring_page replay=runs/browser_session_worker/worker_1781442831330_95838/replay.jsonl
EDITOR index=0 kind=input tag=input id=q name=q label=Search Gists placeholder=Search... rect=74.0x32.0 hidden=false readonly=false active=false sensitivity=none value_len=0
```

## Interpretation

The current `runs/dogfood_profile/default` profile is not authenticated for Gist. Saccade reached a page with only GitHub's global search input, not the new-Gist authoring editor.

This is a good route result: the worker now says "login or non-authoring page" instead of treating the search box as a usable Gist editor.

## Next Step

Open Saccade with the same profile, let Wayne log in, then rerun the exact `inspect-editors` command. If authenticated Gist exposes visible authoring editors, BP-004 can move toward `routed/fixed`; if it exposes only zero-rect editor targets, route to user focus handoff or Chrome-live.
