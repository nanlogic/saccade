# Render Preflight

Render preflight runs before Saccade asks the user for task data or fills a
form. It checks the live page for structural disagreement: fields removed by
safety policy, hidden or zero-rect editors, missing visible authoring controls,
center points that hit a different rendered surface, and a page revision that
changes while Saccade reads it.

The first live API is:

```text
saccade.web.render_preflight
```

The host may bind preflight to a known task surface:

```json
{"expected_surface":"github_issue"}
```

Supported values are `page` (default), `github_issue`, and
`github_discussion`. A mismatch returns `red` with
`recommended_route=navigate_task_surface` before Saccade treats unrelated
writable controls as task evidence.

It returns a verdict, typed reason codes, field/editor counts, the observation
revision, renderer hit-test counts, and an engine route. It does not return
field values, cookies, storage, screenshots, or page text.

The live response labels itself `scope=structural_preflight` and
`full_agreement_measured=false`. A structural green result permits the normal
field policy. It does not claim that Saccade measured visual parity.

## Routing

| Verdict | Meaning | Default route |
| --- | --- | --- |
| `green` | An eligible field has geometry and renderer hit agreement. | Current engine adapter, subject to normal field policy. |
| `yellow` | The page is not clearly an actionable form, or safety filtering left no ordinary fields. | Human review. |
| `red` | Task URL, revision, visibility, or renderer hit evidence disagrees. | Explicit `navigate_task_surface`, `refresh_replan`, `block`, or measured compatibility route. |

A revision change during preflight also returns `red`, but routes to
`refresh_replan`. Saccade must not combine an inventory from one revision with
editor geometry from another.

The route is measured per page and engine. Servo previously routed a GitHub New
Issue page that exposed only zero-rect backing editors. Current CEF exposes a
visible title and body on the measured repository and returns structural green.
The compatibility result is not generalized to every GitHub repository.

## CEF Snapshot

CEF performs field classification, geometry reads, and center-point
`elementFromPoint` checks in one synchronous renderer command. The browser
process then adds its trusted current URL plus start/end page revisions. A
revision change or expected-task mismatch overrides any renderer green result.

This evidence is labeled renderer-observed. It is stronger than composing
separate DOM calls, but it is not an OS-native hit-test and cannot authorize a
side effect. An actual browser-input receipt remains a separate action gate.

## Full Agreement Gate

Use the offline full gate for a complex canary, a rendering regression, or a
page where the visible result conflicts with structural truth:

```bash
python3 scripts/check_human_agent_agreement.py \
  --reference-truth runs/<case>/reference_truth.json \
  --observed-truth runs/<case>/observed_truth.json \
  --hit-test runs/<case>/hit_test.json \
  --reference-screenshot runs/<case>/reference.png \
  --observed-screenshot runs/<case>/saccade.png \
  --output-dir runs/<case>/agreement
```

The report schema is `saccade.human_agent_agreement/1`. It measures
visible-control recall, actionable precision, hidden and duplicate
contamination, geometry drift, native hit-test accuracy, revision consistency,
and optional screenshot difference. The JSON contains no field values or image
pixels.

## Screenshot Escalation

Screenshots are optional diagnostic evidence, not a per-page default. The
normal path is zero-screenshot structural preflight.

Only a public, logged-out, no-user-input page may opt into a local Chrome vs
Servo screenshot comparison when structural preflight is inconclusive. The
comparison returns metrics and a route recommendation; raw images stay local
unless the user asks to inspect them. Logged-in, private, and user-filled pages
stay on the no-screenshot path by default. The CLI writes an overlay only when
the caller passes `--safe-visual-artifact` after checking that the image has no
protected values.

## Privacy Boundary

Preflight cannot authorize actions. Page text and labels remain untrusted. A
`green` verdict means only that the visible and semantic form surfaces are
consistent enough to apply the normal current-tab policy. Sensitive values,
submission, publishing, payment, signing, login, OTP, and account changes stay
human-controlled.

## Current Evidence

- `runs/agreement_gate/live_structural_preflight_20260713/report.json`: the live
  official ServoShell bridge returned structural green on the 17-field form
  fixture at one revision, with no screenshots and no protected fixture values.
- `runs/agreement_gate/offline_responsive_cards_green_20260713/report.json`:
  full offline gate passed the responsive-card fixture with matched facts,
  geometry, hit-tests, and screenshot metrics.
- `runs/agreement_gate/offline_textarea_red_20260713/report.json`: the gate
  routed the known textarea mismatch for duplicate facts, geometry escape, and
  failed hit-tests instead of treating complete-looking truth as safe.
- `runs/ai034_human_agent_agreement/`: task-scoped GitHub Dashboard/New Issue
  preflight plus separate native and shim account-menu hit-test evidence.
- `runs/cef_ai034/local_gate_20260715/report.json`: CEF local green, explicit
  task mismatch routing, and occluded-point blocking without screenshots or
  value leakage.
- `runs/cef_ai034/github_canary_20260715_final/report.json`: logged-in GitHub
  New Issue structural green at 3/3 renderer hit agreement plus a separate
  fact-bound native CEF account-menu receipt; no write, submit, Sign out, or
  screenshot.
