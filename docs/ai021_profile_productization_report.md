# AI-021A Profile Productization Report

Date: 2026-07-05
Status: first slice complete

## What Changed

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

## Verification

Final kit:

```text
dist/saccade-dogfood-ai021-profile-20260705-final/
dist/saccade-dogfood-current -> saccade-dogfood-ai021-profile-20260705-final
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

## Still Open

This closes the wrapper-level product semantics, not the full browser UI. The
next AI-021 slice should put this state into visible browser chrome:

- profile badge/picker near the address bar,
- separate profile state from agent grant state,
- user-facing clear-profile confirmation without terminal commands.
