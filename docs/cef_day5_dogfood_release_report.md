# CEF Day 5 Dogfood Release Report

Date: 2026-07-15
Status: engineering gate passed; two human/external retests remain

## What shipped

- The official CEF Chrome Runtime remains the human browser surface, including
  its tabs, address bar, navigation controls, menus, and Chromium renderer.
  Saccade adds only a thin native trust strip for the active profile and Agent
  Off/On/Paused state.
- Agent grants follow the focused CEF tab. Opening a child tab creates a new
  tab identity; closing it returns control to the remaining visible tab without
  killing the bridge.
- Normal profiles persist outside the repository. Incognito profiles are
  disposable. The official CEF `root_cache_path` and `cache_path` are set from
  the selected user-data directory.
- `article_text` provides a bounded, redacted reading route without exposing
  cookies, storage, sensitive control values, or arbitrary JavaScript.
- The macOS dogfood builder creates a fixed-identity signed app, exact
  CEF/Chromium metadata, license inventory, portable SHA-256 checksums, and
  owner-only launch/grant tools.

## Measured gates

| Gate | Result | Evidence |
| --- | --- | --- |
| No-CDP reflex | PASS: 300/300, zero misses; run p95 3.1-3.4 ms | `runs/cef_day5/day3_3x100/aggregate.json` |
| Original MouseAccuracy | PASS: START plus 12/12 live targets; 10.6 ms p95 | `runs/cef_day5/mouseaccuracy/report.json` |
| FORMMAX | PASS: 672 verified fills, 3 protected fields blocked, 2 page receipts, no values logged | `runs/cef_day5/formmax/report.json` |
| Forms, safety, screenshot, replay | PASS: 17 controls, six safe fills, four unsafe writes rejected, sensitive screenshot blocked, sentinel scan clean | `runs/cef_day5/form_safety_article2/report.json` |
| Tabs and profile restart | PASS three consecutive times; child-tab focus, close recovery, and local state persistence | `runs/cef_day5/session_consistency_1/report.json` through `_3/report.json` |
| Public article | PASS: 9,360 redacted characters and headings, no CDP or screenshot | `runs/cef_day5/public_article/report.json` |
| Public Gist | OBSERVED: collector ready, 24 actions, no cookie/storage read and no submit; profile was logged out | `runs/cef_day5/github_gist/report.json` |

## Keychain behavior

CEF/Chromium uses the macOS `Chromium Safe Storage` Keychain item to encrypt
persistent cookies. A fixed Developer ID signature is therefore mandatory for
Normal profiles. The first signed launch may require one macOS `Always Allow`
authorization. Repeated prompts indicate that an ad-hoc/debug app or a changing
path was used. Saccade does not use CEF's test-only mock Keychain in saved-profile
product runs, and the agent bridge never receives the Keychain secret or raw
cookies.

## Remaining acceptance

1. Open the packaged signed app from one stable path, log into GitHub/Gist once,
   quit, reopen, and confirm the login remains without another Keychain prompt.
   Then fill a harmless draft without submitting it.
2. Start the local game at `http://127.0.0.1:4173/` and rerun the CEF reflex
   gate. The server was not running during this Day 5 closeout, so no new CEF
   game-action result is claimed here.

This is a signed local macOS dogfood build, not a notarized public release.
Public distribution still requires a project-license decision, hardened-runtime
and notarization policy, and a clean-machine installation test.
