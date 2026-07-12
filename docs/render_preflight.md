# Render Preflight

Render preflight runs before Saccade asks the user to enter task data or asks
the agent to fill a form. It checks whether the browser's actionable facts are
consistent with the page's apparent authoring surface.

The first live API is:

```text
saccade.web.render_preflight
```

It returns only a verdict, reason codes, field/editor counts, and an engine
route. It does not return field values, cookies, storage, screenshots, or page
text.

## Routing

| Verdict | Meaning | Default route |
| --- | --- | --- |
| `green` | A visible authoring editor or eligible ordinary field exists. | Servo, subject to normal field policy. |
| `yellow` | The page is not clearly an actionable form, or safety filtering left no ordinary fields. | Human review. |
| `red` | The page advertises an authoring surface but Saccade sees only hidden or zero-rect editor candidates. | Chrome compatibility. |

The GitHub New Issue canary is the first `red` case: the page title says `New
Issue`, but the bridge sees only zero-rect editor candidates. Saccade must not
guess a hidden backing field and write to it.

## Screenshot Escalation

Screenshots are optional diagnostic evidence, not a per-page default. The
normal path is zero-screenshot structural preflight.

Only a public, logged-out, no-user-input page may opt into a local Chrome vs
Servo screenshot comparison when structural preflight is inconclusive. That
comparison should return metrics and a route recommendation to the agent; raw
images stay local unless the user explicitly asks to inspect them. Logged-in,
private, or user-filled pages stay on the no-screenshot path by default.

## Privacy Boundary

Preflight cannot authorize actions. Page text and labels remain untrusted. A
`green` verdict means only that the visible and semantic form surfaces are
consistent enough to apply the normal current-tab policy. Sensitive values,
submission, publishing, payment, signing, login, OTP, and account changes stay
human-controlled.
