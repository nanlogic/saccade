# ServoShell Adapter Migration Plan

Date: 2026-06-14

## Goal

Move Saccade's browser runtime onto official ServoShell while keeping Saccade as
the agent, safety, replay, and protocol layer.

The target shape is:

```text
Saccade agent core
  -> BrowserAdapter trait/protocol
    -> official ServoShell 0.3.x adapter
```

Saccade should not own browser UI or rendering behavior unless official
ServoShell cannot expose the required control surface.

## Plan

### S0: Freeze Current Saccade Browser Evidence

Status: done enough for pivot.

- Current embedded Saccade path is based on crates.io `servo=0.2.0`.
- Official downloaded Servo.app is ServoShell `0.3.0`.
- Official Servo.app can run the local game at `http://127.0.0.1:4173/`.
- `ign.com` also has issues in official Servo.app, so it is not a Saccade-only
  blocker.

### S1: Probe Official ServoShell External Control

Status: pass for the first external-adapter gate.

Try the least invasive bridge first:

- `servoshell --webdriver=<port>`
- `servoshell --devtools=<host:port>`

Done when we know whether an external Saccade process can:

- connect to the control endpoint,
- evaluate JavaScript against the current page,
- collect redacted truth/actions,
- dispatch click/text actions,
- capture screenshot or route screenshots through a supported API.

Decision:

- If enough control exists, build `saccade-servoshell-adapter` outside the
  browser process.
- If control is too thin, fork/integrate official ServoShell source.

Evidence:

```sh
python3 scripts/probe_servoshell_webdriver.py
python3 scripts/probe_servoshell_webdriver.py --url http://127.0.0.1:4173/ --port 7084 --timeout-sec 25
```

Results:

- Fixture probe: `runs/servoshell_webdriver/probe_1781478373425/report.json`
- Local game probe: `runs/servoshell_webdriver/probe_1781478373430/report.json`

Observed capabilities:

- WebDriver `/status` returns ready.
- New WebDriver session succeeds.
- `execute/sync` can read page title, URL, body text length, viewport, and DPR.
- Element lookup and click works on the local button fixture; page revision
  changes from `0` to `1`.
- WebDriver screenshot returns PNG artifacts.
- The local game page is reachable through official ServoShell WebDriver and
  reports title `Blend or Die - Prototype`.

Decision from S1: implement Route A first. Forking official ServoShell source is
still the fallback, not the next immediate step.

Review packet for external architecture review:

- `docs/servoshell_integration_review_packet.md`

### S2: Extract Browser Adapter Boundary

Create a stable browser boundary that Saccade owns:

```text
observe()
actions()
act(action_id)
fill_agent_fields()
inspect_fields()
type_focused_text()
screenshot()
replay_event()
```

The existing `browser_session_worker` can become the legacy `servo-0.2`
implementation of that boundary while ServoShell gets a new implementation.

### S3: External ServoShell Adapter

If S1 passes, implement an external adapter:

- launch official ServoShell with a temporary profile,
- connect over WebDriver/DevTools,
- inject Saccade's redacted truth/action-map JS,
- dispatch actions through the official control channel,
- write replay artifacts in the same schema used today.

This keeps official ServoShell upgradable.

### S4: Source Integration Fallback

If S1 does not expose enough control, clone/build official Servo source and add
the Saccade bridge inside ServoShell:

- keep ServoShell UI/runtime intact,
- add Saccade command server as a small sidecar module,
- preserve safety/redaction/replay in Saccade-owned code,
- avoid changing Servo renderer/layout unless absolutely necessary.

### S5: Product Gate

Saccade-on-ServoShell is ready to replace the current dogfood browser when it
passes:

- local game at `http://127.0.0.1:4173/`,
- local safety/user-flow tests,
- FORMMAX live fill,
- native dropdown/input gate,
- screenshot/replay artifact parity,
- manual dogfood open/click/type/scroll/back/forward.

## Current Next Step

Build the first external adapter around official ServoShell WebDriver:

- launch official ServoShell,
- create/reuse a WebDriver session,
- inject the existing Saccade truth/action-map JavaScript,
- dispatch click actions through WebDriver,
- save screenshot and replay artifacts,
- keep all safety/redaction logic in Saccade-owned code.

Keep a thin official ServoShell fork as the escalation path if WebDriver cannot
enforce trusted-tab isolation, screenshot policy, or native input ownership.
