# Editor Reduction Report

Date: 2026-06-14

## Goal

Reproduce the GitHub/Gist class of editor problem locally: the page can show a large editor while the browser exposes hidden or zero-size backing controls that are unsafe action targets.

## Fixture

`test_pages/editor_reduction/index.html` includes:

- visible Gist-like title input,
- visible contenteditable editor,
- visible CodeMirror-like shell,
- hidden zero-rect backing textarea,
- hidden zero-rect Ace-like text input,
- sensitive legal-attestation textarea.

The fixture includes `LEAK_SENTINEL_DO_NOT_RETURN` inside editor values/text. The selftest fails if the worker returns that sentinel in stdout or replay.

## Result

Command:

```sh
cargo run -q -p saccade-shell -- selftest-editor-reduction
```

Latest pass:

```text
EDITOR_REDUCTION PASS editors=6 zero_rect=2 sensitive=1 route=usable_ignore_hidden_backing_fields visible_body=true visible_shell=true replay=runs/browser_session_worker/worker_1781438229505_20098/replay.jsonl
```

## Interpretation

- Local `inspect_editors` can distinguish visible editor surfaces from hidden backing fields.
- Zero-rect editor candidates are counted rather than treated as safe targets.
- `inspect_editors` now returns a route decision. The local reduction classifies as `usable_ignore_hidden_backing_fields`, meaning visible writable editors exist and hidden zero-rect backing fields should be ignored.
- Sensitive editor-like fields are counted without exposing values.
- The remaining BP-004 question is real-site behavior: if real Gist exposes only a zero-rect writable target, Saccade should route to user focus handoff or Chrome-live rather than pretending the action map is safe.

## Real-Site Probe

Safe GET-only probes were run against GitHub Gist without publishing or typing:

```text
https://gist.github.com/     -> editor_count=1 zero_rect_count=0 label="Search Gists"
https://gist.github.com/new  -> editor_count=1 zero_rect_count=0 label="Search Gists"
```

Artifacts:

- `runs/browser_session_worker/worker_1781437457716_88455/replay.jsonl`
- `runs/browser_session_worker/worker_1781437482073_88545/replay.jsonl`

Interpretation: the independent worker did not inherit the earlier logged-in Gist session, so it never reached the authenticated new-Gist editor. Authenticated real-site BP-004 remains pending on shared profile/login handoff.

## Shared-Profile Probe Update

`saccade-shell inspect-editors` now exposes the same editor inspection path as a reusable CLI and supports `--profile-dir`.

Latest shared-profile Gist probe:

```text
RUST_LOG=error cargo run -q -p saccade-shell -- inspect-editors --url https://gist.github.com/new --profile-dir runs/dogfood_profile/default --width 1440 --height 900
```

Result:

```text
INSPECT_EDITORS PASS url=https://gist.github.com/new editors=1 zero_rect=0 visible_writable=1 visible_authoring=0 sensitive=0 route=route_login_or_non_authoring_page replay=runs/browser_session_worker/worker_1781442831330_95838/replay.jsonl
EDITOR index=0 kind=input tag=input id=q name=q label=Search Gists placeholder=Search... rect=74.0x32.0 hidden=false readonly=false active=false sensitivity=none value_len=0
```

Interpretation: the default shared Saccade profile is still not authenticated for Gist. The route is now correctly non-green because the only visible writable control is a search box, not an authoring editor.

See `docs/gist_editor_probe_report.md`.
