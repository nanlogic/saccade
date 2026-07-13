# Chrome Engine Truth + Reflex POC

Date: 2026-07-13
Status: PASS for the bounded Chrome/CDP proof

## Question

Can Saccade keep its two defining properties on a Chrome engine?

1. The agent receives structured, redacted browser facts instead of guessing
   from screenshots.
2. A fact can drive verified browser input with millisecond p95 latency.

## Result

Yes, for the measured DOM target scope. Three independent 100-target runs
passed the gate:

| Metric | Combined result |
| --- | ---: |
| Targets hit | 300 / 300 |
| Misses | 0 |
| Sensitive values exposed | 0 |
| Screenshots used | 0 |
| Truth to dispatch start p95 | 0.024 ms |
| Host receive to page input receipt p95 | 1.669 ms |
| Full renderer fact to page receipt p95 | 3.8 ms |
| Full-loop p99 | 36.5 ms |
| Full-loop max | 63.5 ms |

The individual full-loop p95 values were 3.2 ms, 2.3 ms, and 7.7 ms. All
three are below the 20 ms gate.

Artifacts:

- `runs/chrome_truth_reflex/poc_headless_100_20260713/report.json`
- `runs/chrome_truth_reflex/poc_headless_100_repeat2_20260713/report.json`
- `runs/chrome_truth_reflex/poc_headless_100_repeat3_20260713/report.json`

## What the POC does

`scripts/probe_chrome_truth_reflex.py` launches a temporary Chrome profile and
installs a renderer observer through CDP. The observer emits only new target
identity, geometry, and timing through `Runtime.addBinding`. The host converts
that fact into `Input.dispatchMouseEvent` and waits for a page-side input
receipt. It then uses the existing Chrome reference inventory to verify that
the fixture's SSN and password controls are classified without exporting their
values.

The fixture is `test_pages/chrome_truth_reflex/index.html`. Its sensitive
sentinels make a false redaction pass detectable in the serialized report.

No screenshot, cookie, storage record, field value, or temporary Chrome
profile is retained.

## What this proves

- Saccade's truth/action contract is not dependent on Servo.
- Blink/Chrome can emit a compact, structured fact stream without screenshots.
- The host motor is sub-millisecond at p95 once the fact reaches it.
- Browser input is receipted by the page with zero misses.
- The current redaction model can be enforced before facts leave the engine.

## What this does not prove

- It does not prove canvas, WebGL, accessibility-tree, shadow-DOM, or browser
  chrome truth.
- It does not prove hard real-time behavior. CDP produced rare 24-64 ms truth
  delivery outliers even though p95 passed.
- It does not provide a Saccade-owned Chromium window, profile UX, downloads,
  permissions, dialogs, updates, signing, or crash recovery.
- CDP injection is a prototype transport, not the intended product security
  boundary.

## Why CEF is the next transport

CEF is Chromium-based and explicitly supports browser-process/render-process
IPC, asynchronous JavaScript bindings, a generic message router, windowed
browser hosting, GPU acceleration, profiles, and browser input APIs. Its
official documentation says Blink/V8 work happens in the renderer process and
application logic usually lives in the browser process, with asynchronous IPC
between them. That is the boundary Saccade needs for redaction-before-export.

Primary references:

- https://chromiumembedded.github.io/cef/general_usage.html
- https://github.com/chromiumembedded/cef-project
- https://cef-builds.spotifycdn.com/docs/145.0/annotated.html

The CEF version should use a renderer-side redaction/inventory handler and a
browser-side Saccade engine adapter. It should not route the reflex loop through
DevTools in production.

## Work estimate

These are engineering estimates, not release commitments.

| Slice | Estimated focused work |
| --- | ---: |
| CEF macOS shell, helper processes, one visible tab | 2-4 days |
| Renderer truth IPC + browser input + this 3x100 gate | 2-4 days |
| Existing grant/policy/replay/MCP contract adapter | 4-8 days |
| Profiles, navigation, dialogs, downloads, crash recovery | 1-3 weeks |
| Signing, updater, release QA, cross-platform packaging | 1-2+ months |

A bounded CEF proof is roughly one focused week. A useful macOS dogfood browser
is roughly 2-4 additional weeks because Saccade already owns the engine-neutral
grant, action, safety, and replay layers. A public general browser remains a
larger product effort.

## Decision gate

Do not replace Servo or rename the product yet. Add a Chromium engine adapter
behind the existing contract and require this gate before migrating product
work:

1. Three independent 100-target CEF runs.
2. Zero misses and zero sensitive-value leaks.
3. Full fact-to-receipt p95 at or below 20 ms in every run.
4. No CDP dependency in the measured loop.
5. Same capability, grant, policy, and replay schema as the current engines.

If this passes, Chromium/CEF becomes the compatibility-first human browser
engine while Servo remains the deeper engine/research path. The product stays
Saccade: browser-native truth, verified actions, safety boundaries, and replay
are the product; the rendering engine is an adapter.
