# Saccade N1 Trusted Tabs Runtime Profile

Date: 2026-06-11

## Result

N1 minimal Trusted Tabs selftest passed on macOS arm64 with pinned Servo `0.2.0`.

Command:

```bash
cargo run -q -p saccade-shell -- selftest-tabs
```

Observed output:

```text
TABS PASS webviews=2 cookie_shared=true storage_shared=true input_isolated=true read_policy_enforced=true
```

## What The Selftest Does

The test starts a local fixture under:

`/Users/waynema/Documents/GitHub/SACCADE/test_pages/login_handoff/`

It creates two WebViews under one Servo instance:

- Tab 1: `owner=Human`, `read_grant=None`, loads `login.html?auto=1`.
- Tab 2: `owner=Agent`, `read_grant=FullTruth`, loads `dashboard.html` after the Human tab logs in.

The Human tab sets:

- `saccade_session=demo` cookie
- `localStorage["saccade_storage"]="shared"`

The Agent tab then opens the dashboard and checks whether the session and localStorage state are visible.

## Measured Behavior

- Multiple WebViews under one Servo instance: passed.
- Cookie sharing between WebViews on same origin: true.
- localStorage sharing between WebViews on same origin: true.
- Agent input policy refuses Human-owned tabs: true.
- Agent truth policy refuses Human tabs with `ReadGrant::None`: true.

## Policy Model

Core tab types now live in `saccade_core`:

- `TabId`
- `TabOwner`
- `ReadGrant`
- `TabVisualMarker`
- `TabInfo`

`TabInfo::agent_input_allowed()` allows input only when `owner=Agent`.

`TabInfo::agent_truth_allowed()` allows truth access when `owner=Agent` or `read_grant != None`.

## Caveats

This is a minimal runtime profile, not the full tab shell UI.

Still pending:

- Physical keyboard/mouse focus routing across user-selected tabs.
- Manual user takeover of an Agent tab.
- Replay events with `tab_id`, `owner`, `actor`, and `page_revision`.
- Sensitive-field masking inside live tab truth.

Servo printed transient storage warnings while the test ran, but cookie and localStorage sharing both measured true.

## Next

N1B adds the explicit login handoff fixture and gate in:

`/Users/waynema/Documents/GitHub/SACCADE/docs/login_handoff_profile.md`

The handoff flow is:

```text
Human tab opens login page
User completes login
User clicks Done
Shell verifies safe logged-in marker
Agent tab continues with inherited session
Password and OTP values remain unavailable to agent truth
```
