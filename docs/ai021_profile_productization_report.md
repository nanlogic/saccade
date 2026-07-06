# AI-021A Profile Productization Report

Date: 2026-07-05
Status: first two slices complete

## What Changed

### AI-021A Wrapper Profile Controls

The dogfood release kit now exposes profile/session state as a product surface,
not only hidden wrapper environment variables:

- `profile-status` prints JSON with profile mode, name, persistence, profile
  directory, current grant file existence, and safety booleans.
- `SACCADE_PROFILE_NAME=<name>` creates named local profiles under
  `runs/dogfood_profile/<name>` unless `SACCADE_PROFILE_DIR` is explicitly set.
- `clear-profile` supports `--dry-run`, typed confirmation, and `--yes`; it
  clears only the current normal named profile by default and refuses custom
  profile paths unless `--force-custom` is supplied.
- Resolved profiles get a `.saccade-profile.json` marker with mode/name metadata.

No command prints raw cookies, storage dumps, passwords, or sensitive field
values.

### AI-021B Browser Chrome Profile Badge

The Saccade ServoShell thin fork now draws a second trusted browser-chrome badge
for profile state, separate from the Copilot/Agent badge:

- Saccade bridge writes a `profile` object into the same trusted status JSON as
  the Copilot badge.
- ServoShell reads that JSON from `SACCADE_COPILOT_STATUS_PATH` and displays
  `Normal`, `Incognito`, or `Profile: <name>` in egui browser chrome.
- The profile badge tooltip states persistence and safety boundaries: raw
  cookies hidden, raw storage hidden, sensitive values hidden.
- Unsafe status JSON that claims raw cookie/storage/sensitive exposure renders
  `Profile Error`.

This remains browser chrome, not page DOM. A webpage cannot spoof the badge by
changing title text, CSS, or page markup.

## Verification

Final kit:

```text
dist/saccade-dogfood-ai021-profile-badge-20260705/
dist/saccade-dogfood-current -> saccade-dogfood-ai021-profile-badge-20260705
```

Evidence:

```text
runs/profile_productization/ai021_profile_commands_final_20260705/
runs/profile_productization/ai021_check_saccade_final_20260705/check_saccade.json
runs/profile_productization/ai021_incognito_check_final_20260705/check_saccade_incognito.json
```

Measured results:

```text
profile-status normal: ok=true, mode=normal, persistent=true, name=work
clear-profile --dry-run: ok=true, action=dry_run
clear-profile --yes: ok=true, action=clear_profile, dummy profile file removed
profile-status incognito: ok=true, mode=incognito, persistent=false
check-saccade normal: ok=true, profile_mode=normal, profile_persistent=true
check-saccade incognito: ok=true, profile_mode=incognito, profile_persistent=false, remaining temp profile dirs=0
```

Browser chrome tests:

```text
cargo check -p saccade-servoshell
cargo test -p servoshell saccade_
cargo build -p servoshell --release
```

Browser chrome visual evidence:

```text
runs/ai021_profile_badge/profile_badge_final_20260705/browser_chrome.png
runs/ai021_profile_badge/profile_badge_final_20260705/smoke_stdout.json
runs/ai021_profile_badge/profile_badge_final_20260705/profile_status.json
runs/ai021_profile_badge/profile_badge_smoke_20260705/browser_chrome.png
runs/ai021_profile_badge/profile_badge_smoke_20260705/smoke_stdout.json
```

The internal browser chrome screenshot shows two separate trusted chrome badges:
`Profile: work` and `Copilot`. This screenshot is captured from ServoShell's
browser chrome framebuffer, not by macOS screen recording and not by page DOM.

ServoShell badge tests passed:

```text
saccade_profile_badge_reads_bridge_json
saccade_profile_badge_marks_incognito
saccade_profile_badge_rejects_agent_storage_exposure
saccade_copilot_badge_reads_bridge_json
saccade_copilot_badge_rejects_spoofable_page_dom_status
```

## Still Open

This closes wrapper-level profile controls and the read-only browser chrome
profile badge. Remaining AI-021 work:

- separate profile state from agent grant state,
- profile picker / switcher in browser chrome,
- user-facing clear-profile confirmation without terminal commands.
