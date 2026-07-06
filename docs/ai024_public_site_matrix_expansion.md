# AI-024 Public Site Matrix Expansion

Date: 2026-07-05
Status: complete for the first extended public-read slice

## Purpose

AI-023 added a small public smoke matrix. AI-024 expands that into a reusable
core/extended matrix so Saccade can be dogfooded on recognizable public sites
without hand-writing URLs each time.

This is still a read-only public-web gate. It does not log in, fill forms,
submit, post, publish, bypass anti-bot checks, or claim Chrome visual parity.

## Tooling

Reusable matrix files:

```text
site_matrices/public_core.json
site_matrices/public_extended.json
```

Run from the repo:

```bash
python3 scripts/run_public_site_smoke_matrix.py \
  --matrix extended \
  --output-dir runs/ai024_public_site_matrix/extended_20260705 \
  --timeout-sec 45
```

Run from the dogfood kit:

```bash
dist/saccade-dogfood-current/run-public-site-smoke-matrix extended --matrix extended
```

The runner now supports `required=false` exploratory sites. Required failures
fail the aggregate gate. Optional failures are still recorded but do not fail the
core public-read gate.

## Result

Report:

```text
runs/ai024_public_site_matrix/extended_20260705/report.json
```

Summary:

| Site | Required | Result | Time | Article Text | Title |
| --- | --- | --- | ---: | ---: | --- |
| example.com | yes | pass | 8.342s | 129 chars | Example Domain |
| Hacker News | yes | pass | 3.925s | not requested | Hacker News |
| Wikipedia Servo page | yes | pass | 4.039s | 7804 chars | Servo (software) - Wikipedia |
| The Rookies modular environment article | yes | pass | 5.054s | 9352 chars | Step-by-Step Guide to Modular Environment Art: From Blender to UE5 |
| GitHub Servo repo | yes | pass | 10.137s | not requested | servo/servo |
| Gist discover | yes | pass | 4.382s | not requested | Discover gists |
| Stack Overflow Rust tag | no | pass | 5.613s | not requested | Newest 'rust' Questions - Stack Overflow |
| Reddit Rust subreddit | no | pass | 5.074s | not requested | The Rust Programming Language |

Aggregate:

```text
site_count=8
pass_count=8
fail_count=0
required_pass_count=6
required_fail_count=0
optional_pass_count=2
optional_fail_count=0
same_webview_control=true on every site
termination=graceful_servo_shutdown on every site
```

## Interpretation

Saccade now has measured read-only public coverage across:

- simple static pages
- public forum listing
- public encyclopedia/reference page
- long tutorial/article extraction
- public GitHub repository
- public Gist discovery page
- public Q&A listing
- public Reddit subreddit listing

This is a meaningful dogfood gate for "can open and observe real public pages."
It is not evidence for logged-in drafting, rich editor compatibility, visual
parity, or provider policy acceptance.

## Next

Keep `public_core` as the fast required gate. Use `public_extended` before
sharing a dogfood kit with another session or using Saccade on a new class of
public page. Logged-in GitHub issue/discussion, Reddit draft, Discourse draft,
and LinkedIn/Facebook/AdMob/App Store Connect remain separate human-in-loop
measurements with explicit user review.
