# AI-020 Human-In-Loop Site Matrix

Date: 2026-07-01
Status: in progress

## Purpose

Saccade should not claim "works on the web" from one demo. The responsible
claim is narrower:

```text
For measured sites, Saccade can open/read, draft/fill non-sensitive content,
preserve user-owned auth/secrets, and leave final side effects to a current
user gesture or explicit current-session confirmation.
```

This matrix lists what to test next and how to classify failures.

## Current Dogfood Runtime

Use this kit for the next measurements:

```text
dist/saccade-dogfood-20260705-174747/
dist/saccade-dogfood-current -> saccade-dogfood-20260705-174747
Saccade commit: 20f52e2
ServoShell: /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell
```

Baseline checks:

```text
check-saccade: PASS
artifact: dist/saccade-dogfood-current/runs/check/bridge_smoke/report.json

public article reader: PASS on The Rookies modular environment article
article_text_length: 9352
artifact: dist/saccade-dogfood-current/runs/article/rookies_20260701/report.json

AI-020 live draft harness local fixture: PASS
artifact: runs/ai020_live/local_forum_fixture_release2/report.json
```

These checks prove the current dogfood kit opens, attaches, extracts useful
truth, preserves the normal profile mode, and shuts ServoShell down cleanly.
They do not prove any logged-in third-party draft surface until that exact site
is measured.

## Current Bug Triage

| Area | Current status | Decision |
| --- | --- | --- |
| GitHub account/profile dropdown | Routed to upstream Servo/Web API compatibility. Source-release and official ServoShell both miss APIs GitHub/Primer uses, including `IntersectionObserver` and adopted stylesheets. | Do not treat as a Saccade window-resize bug. Do not claim GitHub account-menu parity. Use normal browsers for account/logout UI until Servo/API compatibility changes or a measured polyfill exists. |
| Saccade-owned dropdowns / browser chrome UI | Product-owned. If our URL bar, Copilot menu, app dropdown, or agent controls overflow or cannot be clicked, it is our bug. | Fix when reproduced on a Saccade-owned UI surface. Keep separate from third-party GitHub profile-menu parity. |
| Auth/profile | Local profile persistence and clean Servo shutdown are fixed. Same-session human login handoff works for Gist draft fill. | Good enough for dogfood. Do not claim cross-provider persistent login. Real providers can still require fresh 2FA/device trust. |
| WebGL/Canvas | Local game reflex/readback gate is green with ServoShell source-release. Some third-party Canvas/WebGL sites remain unmeasured or upstream-limited. | Not a launch blocker for draft/form/productivity dogfood. Keep broad WebGL parity as measured backlog. |
| Publish/submit policy | Do not add a blanket posting block for ordinary drafting. | Product behavior is "agent drafts, human posts" unless there is explicit current-session confirmation. Login/OTP/payment/security/destructive actions stay human-owned. |

## Test Levels

Each site gets four separate statuses:

| Level | Meaning | Pass condition |
| --- | --- | --- |
| Read | Can open and extract useful truth/text/actions. | Title/body/actions are meaningful; no blank shell after settle. |
| Draft | Can fill or edit non-sensitive draft fields. | Field detection, fill, and verification pass without value logging. |
| Handoff | User can review and complete the side effect. | Submit/publish remains a visible user-owned action. |
| Replay | Evidence is reproducible. | Report/replay/control artifacts exist and redact sensitive values. |

## Priority Site Matrix

| Priority | Site / class | Why AI is useful | Risk lane | Test action | Current status |
| --- | --- | --- | --- | --- | --- |
| P0 | Owned GitHub issue/discussion draft | Developer users constantly need issue/PR/discussion drafting. | Yellow after login | User logs in; Saccade drafts title/body; user submits if desired. | Untested; next target. |
| P0 | GitHub Gist draft | Already useful for code notes/snippets. | Yellow after login | User logs in; Saccade fills description/filename/body; no publish by default. | Passed in AI-017. |
| P1 | Hacker News submit/comment draft | High-signal public technical forum, low UI complexity. | Yellow after login | Draft title/text/comment; user posts. | Read/editor/draft dry-run passed on a real HN thread; visible human handoff/post rehearsal pending. |
| P1 | Reddit post/comment draft | Common forum workflow; useful for editing and summarization. | Yellow/Orange depending subreddit/account reputation | Draft post/comment; user posts. | Untested; likely provider/policy friction. |
| P1 | dev.to / Medium / Hashnode article draft | Long-form developer writing. | Yellow after login | Draft title/body/tags; user publishes. | Untested. |
| P1 | Discourse forum draft | Common forum engine; reusable across communities. | Yellow after login | Draft topic/reply; user posts. | Untested. |
| P2 | LinkedIn post/comment draft | High user demand, high reputation impact. | Orange/Yellow depending action | Draft only; user posts. Avoid connection/message automation. | Untested; treat reputation actions carefully. |
| P2 | Facebook post/group/comment draft | Common but policy/automation-sensitive. | Orange/Yellow depending surface | Draft only; user posts. Avoid messaging, scraping, groups at scale. | Untested; likely provider friction. |
| P2 | X/Twitter/Threads/Bluesky draft | Short public writing and replies. | Yellow/Orange | Draft only; user posts. | Untested. |
| P2 | Stack Overflow / Stack Exchange draft | Technical Q/A editing. | Orange/Yellow due reputation/moderation | Draft question/answer/comment; user posts. | Untested. |
| P2 | Notion / Google Docs / Coda document edit | Real productivity writing. | Yellow after login | Draft/edit document content; user reviews. | Untested; rich editors may expose API gaps. |
| P2 | Jira / Linear / GitHub Projects issue fields | Real PM/dev workflow. | Yellow after login | Fill ticket title/body/labels/non-sensitive fields; user creates. | Untested. |
| P3 | AdMob / App Store Connect / cloud console | Important admin workflows but high impact. | Orange | Redacted analysis/checklists only unless a specific low-risk action is measured. | Redacted fallback passed for App Store Connect. |

## Next Measurement Batch

Run these sequentially, never parallel real-site runs:

1. GitHub issue/discussion draft on an owned or throwaway repo.
2. Hacker News visible login/handoff rehearsal, building on the existing dry-run.
3. Discourse draft on a low-risk public/community instance.
4. Reddit draft only if login and UI behave.
5. LinkedIn post draft only after two lower-risk forum surfaces pass.

Preferred procedure for each site:

```text
1. Open with dist/saccade-dogfood-current/open-saccade <URL>.
2. Human logs in if needed. Login/password/OTP/CAPTCHA are human-only.
3. Human grants the current tab / tells the agent to continue.
4. Agent reads title/body/actions and records read_status.
5. Agent drafts harmless non-sensitive content into the editor.
6. Agent verifies editor state without logging raw sensitive values.
7. Human reviews. Human alone clicks post/submit/publish if desired.
8. Record artifact paths and verdict in this file.
```

Harness command:

```bash
printf 'Saccade AI-020 draft rehearsal. Human will review and decide whether to submit.\n' > /tmp/saccade-draft.txt
dist/saccade-dogfood-current/run-ai020-live-draft \
  --site hn_comment \
  --url https://news.ycombinator.com/item?id=48706714 \
  --body-file /tmp/saccade-draft.txt \
  --manual-gate
```

For each run, record:

```text
site:
url:
profile/session:
read_status:
draft_status:
handoff_status:
replay_status:
screenshots_used: no by default
values_logged: false
publish_attempted: false unless user explicitly confirms
provider_block_or_warning:
chrome/reference_needed:
artifact_paths:
verdict:
```

## Measurements

### Dogfood Kit Baseline

```text
site: local Saccade dogfood kit
url: file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/browser_session/index.html
profile/session: normal persisted Saccade dogfood profile
read_status: pass; title Browser Session Smoke, readyState complete
draft_status: n/a
handoff_status: n/a
replay_status: pass; same_webview_control=true, control replay/report written
screenshots_used: no
values_logged: false
publish_attempted: false
provider_block_or_warning: macOS GLD texture warning observed; non-blocking for this route
chrome/reference_needed: no
artifact_paths:
- `dist/saccade-dogfood-current/runs/check/bridge_smoke/report.json`
- `dist/saccade-dogfood-current/runs/check/bridge_smoke/control/report.json`
- `dist/saccade-dogfood-current/runs/check/bridge_smoke/control/replay.jsonl`
verdict: current kit is ready for same-machine dogfood.
```

### Public Article Reader Baseline

```text
site: The Rookies
url: https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art
profile/session: normal persisted Saccade dogfood profile
read_status: pass; title correct, bodyTextLength=9680, article_text_length=9352
draft_status: n/a
handoff_status: n/a
replay_status: pass; control report/replay written
screenshots_used: no
values_logged: false
publish_attempted: false
provider_block_or_warning: IntersectionObserver error and macOS GLD texture warning observed; non-blocking for article extraction
chrome/reference_needed: no for this reader path
artifact_paths:
- `dist/saccade-dogfood-current/runs/article/rookies_20260701/report.json`
- `dist/saccade-dogfood-current/runs/article/rookies_20260701/control/report.json`
- `dist/saccade-dogfood-current/runs/article/rookies_20260701/control/replay.jsonl`
verdict: current kit is green for public long-form article learning; not a claim about logged-in drafting.
```

### AI-020 Live Draft Harness Local Fixture

```text
site: local forum draft fixture
url: file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/forum_draft/index.html
profile/session: normal persisted Saccade dogfood profile
read_status: pass; title Saccade Forum Draft Fixture
draft_status: pass; one body textarea filled
handoff_status: pending_human_review_submit
replay_status: pass; control report/replay present
screenshots_used: no
values_logged: false; final report candidate plus control report/replay checked
publish_attempted: false; submit_attempted=false
provider_block_or_warning: none
chrome/reference_needed: no
artifact_paths:
- `runs/ai020_live/local_forum_fixture_release2/report.json`
- `runs/ai020_live/local_forum_fixture_release2/bridge/control/report.json`
- `runs/ai020_live/local_forum_fixture_release2/bridge/control/replay.jsonl`
verdict: the reusable AI-020 harness is green on a local low-risk draft fixture and is ready for a visible real-site human handoff run.
```

### Hacker News Comment Draft Dry-Run

```text
site: Hacker News
url: https://news.ycombinator.com/item?id=48706714
profile/session: normal persisted Saccade dogfood profile
read_status: pass; title and 54k body chars extracted
draft_status: pass; generic `draft_editor_fill` filled the visible comment textarea
handoff_status: pending visible user login/review/post rehearsal
replay_status: pass; control report/replay written
screenshots_used: no
values_logged: false; draft sentinel grep found no artifact leak
publish_attempted: false
provider_block_or_warning: none for read/draft; persistent macOS GLD warning remains unrelated
chrome/reference_needed: no
artifact_paths:
- `runs/ai020_hn/thread_read_recheck_20260630-192018/control/report.json`
- `runs/ai020_hn/hn_draft_dryrun_wait_20260630-192058/control/report.json`
- `runs/ai020_hn/hn_draft_dryrun_wait_20260630-192058/control/replay.jsonl`
verdict: measured green for read, editor detection, and no-submit draft fill; not yet a claim about visible human posting.
```

## Acceptance For AI-020

AI-020 is complete when at least one new low-risk real draft surface beyond Gist
has measured evidence, or when the matrix records a provider/engine blocker
with artifact paths and a fallback route.

Do not promote any site from unmeasured to "works" without artifacts.
