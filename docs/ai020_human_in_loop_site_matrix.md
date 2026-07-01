# AI-020 Human-In-Loop Site Matrix

Date: 2026-06-20
Status: planned

## Purpose

Saccade should not claim "works on the web" from one demo. The responsible
claim is narrower:

```text
For measured sites, Saccade can open/read, draft/fill non-sensitive content,
preserve user-owned auth/secrets, and leave final side effects to a current
user gesture or explicit current-session confirmation.
```

This matrix lists what to test next and how to classify failures.

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

## First Measurement Batch

Run these sequentially, never parallel real-site runs:

1. Owned GitHub issue/discussion draft.
2. Hacker News or Discourse draft.
3. Reddit draft if login and UI behave.
4. LinkedIn post draft only after the first two pass.

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
