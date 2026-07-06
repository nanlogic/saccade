# AI-023 Public Site Smoke Matrix

Date: 2026-07-05
Status: complete for the first no-login public-site slice

## Purpose

After the profile/session product gate, Saccade needs measured evidence on more
than local fixtures. This slice adds a repeatable public-site smoke matrix that
opens low-risk pages through the current ServoShell bridge, collects redacted
same-WebView truth, optionally extracts article text, and writes per-site
artifacts.

It does not log in, fill forms, post, submit, bypass anti-bot checks, or claim
visual parity with Chrome.

## Tool

```bash
python3 scripts/run_public_site_smoke_matrix.py \
  --output-dir runs/ai023_public_site_matrix/default_20260705 \
  --timeout-sec 45
```

The dogfood release builder now packages the same tool as:

```bash
dist/saccade-dogfood-current/run-public-site-smoke-matrix
```

Custom matrices can be supplied with `--sites-json`. Runs are sequential and
refuse more than 8 sites at once.

## Result

Report:

```text
runs/ai023_public_site_matrix/default_20260705/report.json
```

Summary:

| Site | Result | Time | Article Text | Notes |
| --- | --- | ---: | ---: | --- |
| example.com | pass | 7.982s | 129 chars | simple public page |
| Hacker News | pass | 4.311s | not requested | public forum read smoke only |
| Wikipedia Servo page | pass | 4.238s | 7804 chars | public reference article |
| The Rookies modular environment article | pass | 5.496s | 9352 chars | known tutorial regression |

All four runs reported:

```text
same_webview_control=true
termination=graceful_servo_shutdown
raw login/form side effects: none
```

## Boundaries

- These are current public web measurements, not permanent site guarantees.
- Public pages classify as Yellow unless explicitly Green/owned/local; this is
  conservative and correct for third-party sites.
- For exact UI rendering judgment, continue to use Chrome/Safari reference.
- For logged-in drafts, use `run-ai020-live-draft` with human login/review.
- For high-risk pages, use the redacted-note fallback instead of live agent
  access.

## Next

Use the same matrix runner to add measured no-login coverage for Reddit read
pages, public GitHub/Gist read pages, and any owned public pages Wayne wants to
dogfood. Logged-in LinkedIn/Facebook/AdMob/App Store Connect flows remain
separate human-in-loop measurements because auth, CAPTCHA, account changes, and
submits are human-owned.
