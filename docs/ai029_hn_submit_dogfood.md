# AI-029 Hacker News Submit Dogfood

Date: 2026-07-06
Status: complete

## Purpose

Use Hacker News as a real public human-in-loop dogfood target without posting
from automation. The target flow is: Saccade opens the real submit page, reads
only safe structure, fills a draft, writes redacted replay/report artifacts, and
leaves the final submit click to the human.

## Result

Saccade can now draft all three visible Hacker News submit fields:

```text
title -> description slot
url   -> filename slot
text  -> body slot
```

The bridge still requires `block_sensitive=true` and `no_submit=true`; the
profile also requires the page URL to match `https://news.ycombinator.com/submit`
and all requested slots to be filled before reporting success.

## Evidence

Initial real-site structure probe:

```text
runs/ai029_hn_dogfood/submit_structure/report.json
title=Submit | Hacker News
hasTitleField=true
hasUrlField=true
hasTextField=true
hasSubmitButton=true
hasLoginFields=false
```

Release-source gate:

```text
runs/ai029_hn_dogfood/hn_submit_live_draft_release/report.json
ok=true
draft_profile=hn_submit
draft_fields_requested=3
draft_fields_filled=3
draft_fields_rejected=0
submit_attempted=false
value_leak_check.ok=true
```

Packaged dogfood wrapper regression:

```text
runs/ai029_hn_dogfood/hn_submit_packaged_wrapper/report.json
ok=true
draft_fields_requested=3
draft_fields_filled=3
draft_fields_rejected=0
submit_attempted=false
value_leak_check.ok=true
```

Current dogfood package:

```text
dist/saccade-dogfood-current -> dist/saccade-dogfood-ai029-hn-submit-20260706
```

## Boundary

This does not claim that Saccade posts to Hacker News. It proves the useful
product behavior: agent drafts, human reviews, human submits.
