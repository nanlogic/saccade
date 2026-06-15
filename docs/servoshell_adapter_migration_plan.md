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

External review decision:

- `docs/servoshell_review_decision.md`
- `docs/servoshell_adapter_product_gate.md`

Summary: use the external ServoShell WebDriver adapter first, but treat it as a
bounded product/safety gate. Prepare a thin official ServoShell fork as fallback.
Avoid upgrading the old embedded `servo=0.2.0` path for now.

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

Decision from S1 and external review: implement Route A first for product and
safety gates. Forking official ServoShell source remains the fallback for
trusted UI, screenshot policy, login handoff, input provenance, and now the
local-game/MOUSEMAX reflex gate.

Update: Route A is not accepted as the final reflex runtime. It is too close to
ordinary WebDriver/Playwright automation for the millisecond game demo.

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

Adapter safety rules:

- WebDriver is a privileged local control channel, not the safety boundary.
- Bind only to `127.0.0.1` on a random private port.
- Do not expose generic WebDriver access to plugins, pages, or untrusted local
  processes.
- Truth/action maps must be redacted before crossing the adapter boundary.
- Raw `getPageSource()` or raw DOM dumps are not replay inputs.

### S3: External ServoShell Adapter

If S1 passes, implement an external adapter:

- launch official ServoShell with a temporary profile,
- connect over WebDriver/DevTools,
- inject Saccade's redacted truth/action-map JS,
- dispatch actions through the official control channel,
- write replay artifacts in the same schema used today.

This keeps official ServoShell upgradable.

Implementation sequence:

1. Direct WebDriver client with explicit Servo-compatible capabilities.
2. Versioned Saccade truth/action JS bundle.
3. Guarded screenshot policy:
   - default `forbidden`,
   - `guarded_diagnostic` after sensitive-surface preflight,
   - no raw screenshots for login/payment/account/messaging/medical/bank/admin
     or password-manager surfaces.
4. Safe action dispatch:
   - pre-truth,
   - safety decision,
   - WebDriver click/keys/actions,
   - post-truth,
   - postcondition check,
   - replay record.
5. Optional DevTools only for diagnostics, not action/safety authority.

### S4: Source Integration / Reflex Bridge

If S3 fails a concrete Saccade safety/product gate, or if it cannot meet the
local-game reflex gate, clone/build official Servo source and add the Saccade
bridge inside ServoShell:

- keep ServoShell UI/runtime intact,
- add Saccade command server as a small sidecar module,
- preserve safety/redaction/replay in Saccade-owned code,
- avoid changing Servo renderer/layout unless absolutely necessary.

Fork trigger examples:

- screenshot safety requires pre-compositor masking,
- trusted UI can be spoofed by page content,
- login handoff cannot be made safe externally,
- manual/agent input provenance must be enforced in-process,
- WebDriver action semantics diverge from required native input behavior.
- WebDriver cannot provide ms-level frame truth and input control for local
  games/MOUSEMAX.

### S5: Product Gate

Saccade-on-ServoShell is ready to replace the current dogfood browser when it
passes:

- local game at `http://127.0.0.1:4173/`,
- local safety/user-flow tests,
- FORMMAX live fill,
- native dropdown/input gate,
- screenshot/replay artifact parity,
- manual dogfood open/click/type/scroll/back/forward.

Expanded product gate:

- Browser smoke: session, JS truth, element action, post-truth verification.
- Safety redaction: password/token/email/hidden/autofill/contenteditable values
  leak nowhere.
- Screenshot policy: sensitive pages block screenshot before capture.
- Replay integrity: pre-truth, safety decision, action, post-truth, screenshot
  policy, and artifacts are logged.
- Isolation: fresh profile, random loopback port, private endpoint, clean
  teardown.
- Upgradeability: pinned official ServoShell plus one newer official build or
  nightly.

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
