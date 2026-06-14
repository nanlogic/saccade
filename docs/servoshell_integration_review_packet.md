# ServoShell Integration Review Packet

Date: 2026-06-14

Audience: GPT-5.5 Pro / Claude / external architecture review

## Context

Saccade started with an embedded Servo path using crates.io:

```text
servo = "=0.2.0"
```

That path produced strong agent evidence:

- truth/action map
- safe field redaction
- login handoff
- FORMMAX live fill
- replay artifacts
- manual input forwarding

But browser-productization work exposed rendering/runtime gaps. In particular,
canvas/game pages can fail or differ from Chrome/reference.

Wayne then tested the downloaded official macOS Servo.app:

```text
/Applications/Servo.app
```

It reports:

```text
ServoShell 0.3.0
servoshell --version => Servo 0.3.0-302457869
```

Official Servo.app can run the local game:

```text
http://127.0.0.1:4173/
```

That means the local game is not a hard Servo-engine impossibility. The likely
gap is Saccade's old embedder/runtime path, version pin, or screenshot/readback
path.

`ign.com` also behaves badly in official Servo.app, so IGN should be treated as
an upstream Servo/site compatibility limitation, not a Saccade-specific blocker.

## New Evidence

Official ServoShell exposes WebDriver:

```sh
/Applications/Servo.app/Contents/MacOS/servoshell --webdriver=<port> <url>
```

Saccade added a repeatable probe:

```sh
python3 scripts/probe_servoshell_webdriver.py
python3 scripts/probe_servoshell_webdriver.py --url http://127.0.0.1:4173/ --port 7084 --timeout-sec 25
```

Results:

- Fixture probe report: `runs/servoshell_webdriver/probe_1781478373425/report.json`
- Game probe report: `runs/servoshell_webdriver/probe_1781478373430/report.json`

Observed via official ServoShell WebDriver:

- server `/status` is ready,
- new WebDriver session succeeds,
- synchronous JS execution works,
- element lookup works,
- WebDriver click works on a local button fixture,
- DOM change after click is observed (`revision: 0 -> 1`),
- screenshot endpoint returns PNG,
- local game page is reachable and reports title `Blend or Die - Prototype`.

## Decision Options

### Option A: External ServoShell Adapter

Run official ServoShell as the browser process and connect from Saccade over
WebDriver/DevTools.

Saccade owns:

- browser truth JS,
- action-map extraction,
- safe field policy,
- sensitive redaction,
- replay logging,
- user/agent protocol,
- launch/profile orchestration.

ServoShell owns:

- rendering,
- browser UI,
- WebView lifecycle,
- canvas/WebGL runtime,
- macOS app packaging baseline.

Pros:

- fastest path to dogfood official ServoShell behavior,
- least invasive,
- easiest to upgrade official ServoShell,
- avoids Saccade maintaining browser UI/rendering internals.

Cons:

- WebDriver may be too thin for trusted-tab isolation,
- user-visible manual input and agent input may be harder to separate,
- screenshots can leak visible sensitive values unless Saccade controls capture
  policy carefully,
- WebDriver action semantics may not exactly match native Servo input semantics,
- may need a sidecar process and launch manager.

### Option B: Thin Fork of Official ServoShell

Fork official ServoShell source and add a small Saccade bridge inside the app.

Keep official browser UI/runtime intact. Add:

- Saccade command server,
- redacted truth/action-map endpoint,
- safe action dispatcher,
- replay hooks,
- trusted tab/session policy hooks.

Pros:

- strongest control over safety boundaries,
- can share native input/event lifecycle with ServoShell,
- can avoid WebDriver limitations,
- product can ship as one app.

Cons:

- higher maintenance cost,
- we must track upstream ServoShell changes,
- build complexity is much heavier,
- risk of drifting into browser-engine maintenance.

### Option C: Upgrade Existing Saccade Embedder

Keep current Saccade shell architecture, but upgrade from crates.io `servo=0.2.0`
to a git/source Servo matching official ServoShell.

Pros:

- keeps existing Saccade ownership boundaries,
- reuses current worker/MCP/test structure.

Cons:

- may still differ from official ServoShell UI/runtime,
- likely heavy API migration,
- may continue to make Saccade responsible for browser shell details,
- least aligned with Wayne's desire to avoid building UI/rendering ourselves.

## Recommended Plan

Use a two-track plan, with Option A first and Option B prepared as fallback:

1. Build `saccade-servoshell-adapter` over official ServoShell WebDriver.
2. Port existing truth/action-map/safety JS to WebDriver execution.
3. Reproduce current local gates through official ServoShell:
   - browser session smoke,
   - safety redaction,
   - FORMMAX live fill,
   - focused typing,
   - local game screenshot/truth.
4. If WebDriver cannot enforce safety, login handoff, or native input policy,
   switch to a thin fork of official ServoShell.
5. Avoid Option C unless official ServoShell source integration proves too hard
   and WebDriver is insufficient.

## Specific Questions For Review

1. Is the two-track plan sound, or should Saccade go straight to a thin fork of
   official ServoShell?
2. Is WebDriver a sufficient long-term adapter boundary for an agent browser, or
   is it only a bootstrap/prototype bridge?
3. Which safety guarantees cannot be enforced from outside the browser process?
4. How should Saccade handle screenshots when user-visible pages may contain
   sensitive values?
5. Should the adapter use WebDriver, DevTools, or both?
6. What is the cleanest way to keep a forked ServoShell close to upstream?
7. What minimal product gate should decide between external adapter and source
   fork?
8. Are there obvious ServoShell 0.3.0 build/release traps on macOS that should
   be researched before coding?

## Prompt To Send

Please review this architecture decision for Saccade.

Saccade is an agent browser layer with safety/redaction/replay. It currently
uses an embedded Servo 0.2.0 path, but the official downloaded ServoShell 0.3.0
can run our local game where the embedded path has rendering/runtime issues.
Official ServoShell also exposes WebDriver, and our probe confirms session
creation, JS execution, element click, DOM-change verification, screenshot
capture, and local game reachability.

We are deciding between:

1. external adapter over official ServoShell WebDriver/DevTools,
2. thin fork of official ServoShell with Saccade bridge inside,
3. upgrading the existing Saccade embedder to a matching git/source Servo.

Please critique the plan above. Focus on safety boundaries, maintainability,
upgrade strategy, macOS packaging/build risk, and the minimum evidence needed to
choose external adapter vs source fork. Give a concrete recommendation and an
ordered implementation plan.
