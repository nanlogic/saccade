# AI-025 Live Draft Profiles

Date: 2026-07-06
Status: complete for local profile/fixture gate

## Purpose

AI-020 proved the human-in-loop live draft harness on Gist and Hacker News. The
next step is making it usable for more real draft surfaces without making the
bridge fill arbitrary fields.

AI-025 adds narrow draft profiles to `run_ai020_live_draft.py`. The profiles map
human-facing field names such as `title` or `comment` onto the existing safe
bridge slots:

```text
description
filename
body
```

The bridge still only fills visible non-sensitive draft authoring fields, does
not click submit/publish, preserves existing user values by default, and records
no draft text values in report/replay artifacts.

## Profiles

Supported profiles:

```text
raw
gist
generic_body
hn_comment
discourse_reply
reddit_comment
github_issue
github_discussion
```

Known `--site` values infer profiles automatically:

```text
hn_comment -> hn_comment
local_forum -> generic_body
github_issue -> github_issue
github_discussion -> github_discussion
discourse_reply -> discourse_reply
reddit_comment -> reddit_comment
```

For `github_issue` and `github_discussion`, `title` maps to the internal
`description` slot because the bridge already targets title/description-like
visible text inputs through that slot. `body` maps to the body editor.

## Verification

Issue-style local fixture:

```bash
python3 scripts/run_ai020_live_draft.py \
  --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/issue_draft/index.html \
  --site github_issue \
  --draft-profile github_issue \
  --title-file /tmp/saccade-ai025-title.txt \
  --body-file /tmp/saccade-ai025-body.txt \
  --headless \
  --output-dir runs/ai025_live_draft_profiles/local_issue_fixture_20260706 \
  --run-name local_issue_fixture \
  --bin dist/saccade-dogfood-current/bin/saccade-servoshell \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell
```

Result:

```text
ok=true
draft_profile=github_issue
draft_fields_requested=2
draft_fields_filled=2
draft_fields_rejected=0
filled slots=description, body
submit_attempted=false
value_leak_check.ok=true
```

Comment-style local regression:

```bash
python3 scripts/run_ai020_live_draft.py \
  --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/forum_draft/index.html \
  --site local_forum \
  --comment-file /tmp/saccade-ai025-comment.txt \
  --headless \
  --output-dir runs/ai025_live_draft_profiles/local_forum_regression_20260706 \
  --run-name local_forum_regression \
  --bin dist/saccade-dogfood-current/bin/saccade-servoshell \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell
```

Result:

```text
ok=true
draft_profile=generic_body
draft_fields_requested=1
draft_fields_filled=1
draft_fields_rejected=0
filled slot=body
submit_attempted=false
value_leak_check.ok=true
```

Packaged dogfood wrapper regression:

```text
runs/ai025_live_draft_profiles/packaged_issue_fixture_20260706/report.json
ok=true
draft_profile=github_issue
draft_fields_filled=2
draft_fields_rejected=0
submit_attempted=false
value_leak_check.ok=true
```

## Boundary

This does not claim real GitHub issue/discussion, Discourse, or Reddit logged-in
drafts are green yet. It proves the reusable profile layer and local issue/forum
draft gates are ready for a visible real-site human login/review run.

Next measured target: owned GitHub issue or discussion draft. The user logs in
and owns final submit; Saccade fills title/body only after the user grants the
visible session.
