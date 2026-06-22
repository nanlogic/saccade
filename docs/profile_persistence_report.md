# Profile Persistence Report

Date: 2026-06-14

Updated: 2026-06-19 for the ServoShell 0.3 bridge path.

Updated: 2026-06-22 for dogfood release profile selection.

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
- `saccade-servoshell bridge` now also flushes profiles on the ServoShell 0.3
  path. The important detail is that Servo's WebDriver extension shutdown route
  is `DELETE /session/{id}/servo/shutdown`, not `POST`. Saccade now calls that
  route, waits for `graceful_servo_shutdown`, and only then falls back to
  `DELETE /session` + SIGTERM if ServoShell does not exit in time.
- Dogfood release wrappers now default to the stable profile
  `runs/dogfood_profile/default` through `SACCADE_PROFILE_DIR`, instead of each
  rebuilt kit using its own `dist/saccade-dogfood-*/profile/default`. This keeps
  login state across dogfood rebuilds while still allowing explicit profile
  separation with `SACCADE_PROFILE_DIR=/path/to/profile`.

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

ServoShell 0.3 bridge verification:

```sh
cargo check -p saccade-servoshell --quiet
cargo build -p saccade-servoshell --quiet
python3 scripts/probe_servoshell_profile_persistence.py \
  --output-dir runs/profile_persistence/ai005c_delete_shutdown_fix_20260619 \
  --timeout-sec 20 \
  --fixture-port 7805
```

Latest pass:

```text
SERVOSHELL_PROFILE_PERSISTENCE ok=true report=runs/profile_persistence/ai005c_delete_shutdown_fix_20260619/report.json
```

The local fixture confirms that source-release ServoShell, official Servo.app,
and the Saccade ServoShell bridge can write and reuse profile cookies when the
correct shutdown route is used. The bridge run records
`termination=graceful_servo_shutdown`.

## Limits

- This shares Saccade-owned profiles. It does not import Chrome/Safari/Firefox cookies.
- Google/GitHub login still requires the human to log in inside Saccade first.
- Keeping a stable Saccade profile means local cookies/session storage persist on
  disk, like Chrome. Saccade artifacts still redact sensitive field values and do
  not print cookies, but the profile directory itself should be treated as local
  browser profile data and kept out of git/backups unless intentionally managed.
- Profile data is human-owned browser state. Agent access is a current-tab or
  current-session grant over redacted truth/actions; it is not permission to
  read raw cookies, password-manager state, storage dumps, or sensitive field
  values.
- Real providers may still invalidate or refuse cross-restart authenticated
  sessions because of their own session-cookie, device trust, 2FA, or security
  policy. The local fixture proves Saccade profile flushing; it does not promise
  every provider will preserve login across restarts.
- Friendly normal/incognito/named profile UI, profile locking UX,
  clear-profile, and password-manager integration are not implemented.
- AI-005B closed same-process authenticated Gist draft fill. Cross-restart
  authenticated Gist reuse remains a real-site dogfood check, not a local
  storage primitive blocker.
