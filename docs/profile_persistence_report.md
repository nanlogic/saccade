# Profile Persistence Report

Date: 2026-06-14

## Goal

Make real-site dogfood possible without asking the user to log in for every worker process. The target behavior is:

1. User logs in inside a Saccade browser using a persistent Saccade profile.
2. A later Saccade worker opens the same site with the same profile dir.
3. The worker sees the Saccade-owned session cookie/storage while sensitive fields remain governed by policy.

## Implementation

- `saccade-shell browse` now accepts `--profile-dir`.
- `saccade-shell browser-session-worker` now accepts `--profile-dir`.
- Both paths pass Servo `Opts.config_dir` to the pinned Servo 0.2.0 API.
- Worker and dogfood shutdown now explicitly drop their WebView before exiting. This breaks the `WorkerState/DogfoodBrowserState -> WebView -> delegate Rc<State>` cycle so Servo can shut down and write persistent site data such as `cookie_jar.json`.

## Verification

Command:

```sh
cargo run -q -p saccade-shell -- selftest-profile-persistence
```

Latest pass:

```text
PROFILE_PERSISTENCE PASS cookie_status=present profile_dir=/Users/waynema/Documents/GitHub/SACCADE/runs/profile_persistence/profile_1781437956546 replay=runs/browser_session_worker/worker_1781437959145_11233/replay.jsonl
```

Additional checks after this change:

```sh
cargo check -p saccade-shell
cargo run -q -p saccade-shell -- selftest-editor-reduction
cargo test -p saccade_browser shell_title
```

## Limits

- This shares Saccade-owned profiles. It does not import Chrome/Safari/Firefox cookies.
- Google/GitHub login still requires the human to log in inside Saccade first.
- Friendly profile picker, profile locking UX, and password-manager integration are not implemented.
- Authenticated Gist editor retest is still pending; unauthenticated Gist probes only reached the search UI.
