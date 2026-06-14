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
EDITOR_REDUCTION PASS editors=6 zero_rect=2 sensitive=1 visible_body=true visible_shell=true replay=runs/browser_session_worker/worker_1781437343123_80312/replay.jsonl
```

## Interpretation

- Local `inspect_editors` can distinguish visible editor surfaces from hidden backing fields.
- Zero-rect editor candidates are counted rather than treated as safe targets.
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
