# Render Preflight

Render preflight runs before Saccade asks the user for task data or fills a
form. It checks the live page for structural disagreement: fields removed by
safety policy, hidden or zero-rect editors, missing visible authoring controls,
and a page revision that changes while Saccade reads it.

The first live API is:

```text
saccade.web.render_preflight
```

It returns a verdict, typed reason codes, field/editor counts, the observation
revision, and an engine route. It does not return field values, cookies,
storage, screenshots, or page text.

The live response labels itself `scope=structural_preflight` and
`full_agreement_measured=false`. A structural green result permits the normal
field policy. It does not claim that Saccade measured visual parity.

## Routing

| Verdict | Meaning | Default route |
| --- | --- | --- |
| `green` | A visible authoring editor or eligible ordinary field exists. | Servo, subject to normal field policy. |
| `yellow` | The page is not clearly an actionable form, or safety filtering left no ordinary fields. | Human review. |
| `red` | The page advertises an authoring surface but Saccade sees only hidden or zero-rect editor candidates. | Chrome compatibility. |

A revision change during preflight also returns `red`, but routes to
`refresh_replan`. Saccade must not combine an inventory from one revision with
editor geometry from another.

The GitHub New Issue canary is the first `red` case: the page title says `New
Issue`, but the bridge sees only zero-rect editor candidates. Saccade must not
guess a hidden backing field and write to it.

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
