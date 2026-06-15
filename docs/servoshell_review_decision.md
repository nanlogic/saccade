# ServoShell Review Decision

Date: 2026-06-14

## Decision

Adopt the external review recommendation:

```text
Option A first, bounded by Saccade product/safety gates.
Option B ready as the safety fallback.
Avoid Option C for now.
```

In practical terms:

- Build `saccade-servoshell-adapter` over official ServoShell WebDriver first.
- Treat WebDriver as a privileged local control channel, not as the safety
  boundary.
- Keep a thin official ServoShell fork design ready, but do not start it until a
  concrete Saccade gate fails.
- Do not invest in upgrading the old embedded `servo=0.2.0` path right now.

## Why

The installed official ServoShell reports:

```text
ServoShell 0.3.0
Servo 0.3.0-302457869
```

It can run the local game at:

```text
http://127.0.0.1:4173/
```

The current embedded Saccade path uses crates.io `servo=0.2.0` and has
canvas/game/runtime issues. Therefore the fastest high-confidence path is to
use official ServoShell as the browser runtime and keep Saccade focused on
agent logic, safety policy, and replay.

## Hard Rules

### WebDriver Is Privileged

The adapter must:

- bind only to `127.0.0.1`,
- use a random port,
- keep the port private to the launch manager,
- avoid exposing generic WebDriver access to plugins/pages/users,
- use a minimal explicit WebDriver dialect rather than Chrome/Selenium defaults.

### Screenshots Are Unsafe By Default

Screenshot modes:

- `forbidden`: no screenshot; use redacted DOM truth only.
- `guarded_diagnostic`: run sensitive-surface preflight; capture only if safe.
- `trusted_render`: only available in a future in-browser bridge/fork where
  sensitive regions can be masked before pixels leave the browser.

Option A uses `forbidden` by default and `guarded_diagnostic` only for local
fixtures, local game pages, and explicit low-risk allowlists.

### Truth Crosses The Boundary Redacted

Never use raw page source or raw DOM dumps for replay. The adapter injects a
versioned Saccade JS bundle that returns only redacted truth:

```json
{
  "page": { "url": "...", "title": "...", "origin": "..." },
  "safety": {
    "visible_sensitive_surface": false,
    "capture_allowed": true
  },
  "actions": [
    {
      "id": "a_17",
      "kind": "click",
      "label": "Submit",
      "role": "button",
      "rect": [120, 440, 80, 32],
      "sensitive": false,
      "confidence": 0.94
    }
  ],
  "redactions": [
    { "selector_hash": "...", "kind": "password", "value": "[REDACTED]" }
  ]
}
```

## Adapter Gate

Stay on Option A only if these pass through official ServoShell:

| Gate | Required evidence |
| --- | --- |
| Browser smoke | session, JS truth, element action, post-truth verification |
| Screenshot policy | sensitive pages block screenshot before capture |
| Safety redaction | password/token/email/hidden/autofill/contenteditable fixtures leak no values |
| FORMMAX | live fill works with same safe-field policy |
| Focused typing | agent text reaches intended non-sensitive target only |
| Login handoff | user can log in without Saccade capturing sensitive DOM/screenshots |
| Replay integrity | pre-truth, safety decision, action, post-truth, screenshot policy are logged |
| Local game | official ServoShell adapter reaches and captures usable low-risk game evidence |
| Isolation | fresh profile, random loopback port, private endpoint, clean teardown |
| Upgradeability | adapter works against pinned official ServoShell and one newer build/nightly |

## Fork Triggers

Switch to a thin official ServoShell fork if any of these cannot be solved
externally:

- screenshot safety needs pre-compositor masking,
- trusted UI can be spoofed by page content,
- login handoff cannot be made safe externally,
- manual/agent input provenance must be enforced in-process,
- WebDriver action semantics diverge from required native input semantics,
- required telemetry needs in-process hooks rather than polling.

## Thin Fork Rules

If Option B is triggered:

- patch only `ports/servoshell` and a small Saccade bridge module,
- keep the bridge behind a feature flag,
- do not fork layout, script engine, WebRender, WebGL/WebGPU, network, DOM, or
  CSS behavior,
- rebase frequently onto upstream,
- upstream general ServoShell fixes,
- keep bridge API narrow: truth, safe action, screenshot policy, session policy,
  replay hooks.

## Immediate Queue

1. Create a minimal WebDriver client/adapter with direct HTTP calls:
   `status`, `session`, `execute/sync`, `element`, `click`, `value`,
   `screenshot`, `delete session`.
2. Port Saccade truth/action-map JS into one versioned adapter bundle.
3. Implement screenshot preflight and `forbidden`/`guarded_diagnostic` modes.
4. Implement action dispatch with pre-truth, safety decision, WebDriver action,
   post-truth, and replay record.
5. Re-run browser smoke, safety, FORMMAX, focused typing, and local game through
   official ServoShell.
6. Only then decide whether the adapter is enough or a thin fork is required.
