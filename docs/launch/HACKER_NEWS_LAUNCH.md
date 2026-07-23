# Hacker News Launch Preparation

Status: preparation only

Rules checked on 2026-07-23:

- [Hacker News Guidelines](https://news.ycombinator.com/newsguidelines.html)
- [Show HN Guidelines](https://news.ycombinator.com/showhn.html)

HN asks submitters to use the original article title, avoid editorialized or
promotional titles, submit the original source, and refrain from soliciting
votes or comments. We will submit the article URL and leave the HN text field
empty. HN's generated or AI-edited text prohibition appears under its comment
rules; it does not prohibit publishing an AI-edited article on our own site.
Wayne should write every HN comment and reply in his own words.

## Phase 1: regular article submission

This is the correct first submission while Saccade has a public repository and
technical evidence but no supported public binary. The submission is a link to
the article, not a duplicate post written inside HN.

### Submission fields

Title:

```text
Building a browser agent that can prove what it clicked
```

URL:

```text
[PUBLIC_ARTICLE_URL]
```

Text:

```text
Leave empty. This is a URL submission.
```

The article page must use the same title. Do not add Saccade, NaN Logic,
"launch," benchmark adjectives, or an exclamation point to the HN title.

### Go or no-go gate

- [ ] The live article has no unresolved placeholders.
- [ ] Its strongest measured claim links to a public report and replay.
- [ ] The iframe result states its current status without implying universal
      support.
- [ ] The repository is readable from a logged-out browser.
- [ ] The evidence pack contains no credentials, capabilities, personal paths,
      URL queries, notifications, or account details.
- [ ] Wayne can stay available for the first discussion window.
- [ ] No teammate, friend, mailing list, or social post asks for HN votes or
      comments.

### Founder comment briefing

HN prohibits generated or AI-edited comments, so this kit does not contain a
paste-ready first comment. Wayne can write a short comment in his own words
using any facts he wants from this list:

- the nested-iframe dogfood failure that motivated the article;
- why one-tab grants and same-WebView receipts matter to him;
- the exact current release status and what readers can try today;
- one limitation he wants help testing;
- his role in the project and how long he has worked on it.

Write from memory after reviewing the facts. Do not send a draft through an AI
editor before posting it to HN.

### Questions to prepare for

These are research prompts for Wayne, not generated HN replies.

| Likely question | Facts to review before answering |
| --- | --- |
| Why build a browser instead of a Playwright wrapper? | Per-tab human grant, same visible session, protected-value path, renderer receipt, value-free replay |
| Does the reflex demo use an LLM for every click? | One MCP call; local fact-to-native-input loop; zero hot-loop LLM calls; exact report link |
| Does a receipt prove the website did what the user wanted? | It proves scoped browser input and observed postcondition, not the truth of a site's claims |
| Can it fill cross-origin iframes? | State the measured corpus and the current nested-iframe result |
| How do passwords and OTPs work? | Human/browser-owned entry; completion state may be visible; raw values never enter model context |
| Is the comparison with Playwright fair? | Name the exact matched task, configuration, sample size, and non-claims |
| Can I install it? | Give the current public release status and avoid private handoff promises |
| Why CEF instead of a Chrome extension? | Same-WebView ownership, native browser chrome, persistent shared tab, local MCP contract |
| Is the replay safe to publish? | Value-free schema, sanitizer, checksum, leak review, known audit limits |

## Phase 2: Show HN

Show HN requires something readers can try. A blog post, signup page, waitlist,
or video does not qualify. Prepare this only after the product lane in
`WEB_BLOG_RELEASE_CHECKLIST.md` passes.

Candidate title:

```text
Show HN: Saccade, a browser with revision-bound actions and receipts
```

Candidate URL:

```text
[PUBLIC_TRY_OR_RELEASE_URL]
```

The linked page should put the install command or download, system
requirements, five-minute local demo, source, security boundary, and known
limitations above any newsletter or company material.

### Show HN gate

- [ ] A logged-out reader can download and run the published build.
- [ ] The build has a stable version, signature/notarization, checksums, and a
      matching public source commit.
- [ ] The quick start works on a clean supported machine.
- [ ] A local fixture demonstrates grant, fact, action, receipt, and replay
      without requiring a third-party account.
- [ ] Readers can report a failure without exposing page values or credentials.
- [ ] Wayne has tested uninstall and profile preservation.

## Publication-day conduct

- Submit once. Do not delete and repost because the first submission is slow.
- Do not coordinate votes or seed comments.
- Answer the strongest version of a question and state uncertainty.
- Link to evidence when a reply contains a number.
- Correct factual errors in the article with a timestamp.
- Flag abuse without announcing the flag in the thread.
- Keep product bugs in the issue tracker and link the issue when it helps the
  discussion.
