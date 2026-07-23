# Web And Blog Release Checklist

Owner: NaN Logic LLC

This list defines the smallest credible Saccade web launch. A technical article
can ship before a public binary. A product launch cannot.

## 1. Choose the launch lane

### Lane A: technical article

Use this lane for the first publication.

- [ ] Publish on an owned, stable HTTPS domain.
- [ ] Link to the public GitHub repository.
- [ ] Link to one dated, sanitized evidence pack.
- [ ] Label Saccade as experimental dogfood software.
- [ ] Offer source and architecture for inspection without promising a stable
      installer.

### Lane B: product and Show HN

Wait until every item below passes.

- [ ] A signed, notarized macOS download or an equivalent supported platform
      artifact is available from a stable URL.
- [ ] A new user can install, connect the MCP, open an Agent tab, and run a
      local demo in less than ten minutes.
- [ ] Trying the product requires no waitlist, email capture, or private build
      request.
- [ ] The release page includes checksums, exact version, system requirements,
      uninstall steps, license, privacy boundary, and known limitations.
- [ ] The release commit and public evidence pack match the downloadable build.

## 2. Initial page list

| Page | Purpose | Required for article | Required for product |
| --- | --- | --- | --- |
| Home | One-sentence product definition, demo, status, repository link | Yes | Yes |
| Article | Explain the receipt model through a concrete failure and measured run | Yes | Yes |
| Evidence | Dated report, replay, full video, preview, checksums, non-claims | Yes | Yes |
| How it works | Grant, fact, revision, native action, receipt, verification | Optional | Yes |
| Install | Supported build, prerequisites, MCP setup, first local task | No | Yes |
| Compatibility | Tested surfaces, blocked surfaces, iframe/custom-control status | Yes | Yes |
| Security and privacy | Protected values, capability boundary, reporting route | Yes | Yes |
| Changelog | Build date, commit, platform, evidence links | No | Yes |

The first article release can combine Home, Compatibility, and Security into
the article and repository. The product launch should give each subject a
stable page.

## 3. Homepage content

- [ ] Use the current definition: "An experimental agent-native desktop browser
      with revision-bound facts, native actions, and verified receipts."
- [ ] Show one six-second muted preview linked to the full run.
- [ ] Put "experimental" and supported platforms beside the primary link.
- [ ] Give GitHub and Evidence equal visual weight with Download or Build.
- [ ] Explain Agent On/Off and per-tab grants in one image or short sequence.
- [ ] State that video illustrates a run; structured same-WebView evidence
      determines its verdict.
- [ ] Avoid "fully autonomous," "works on every site," "unhackable," or
      universal speed and token claims.

## 4. Evidence gate

- [ ] Run one installed-build 15-second reflex demonstration and retain the
      settled result screen in the uncut recording.
- [ ] Run one nested-iframe form case with all requested fields discovered and
      verified, or publish the missing-field block as a limitation.
- [ ] Build the public pack with
      `scripts/build_reflex_evidence_pack.py`.
- [ ] Inspect `report.json` and `replay.jsonl` for secrets, personal data,
      capability values, full local paths, and URL queries.
- [ ] Verify `SHA256SUMS` and confirm the build, commit, date, and platform.
- [ ] Watch the full MP4 and the loop. Check for notifications, unrelated tabs,
      account details, microphone audio, and private bookmarks.
- [ ] Keep the exact failed and blocked cases. Do not edit a failed run into a
      passing narrative.

## 5. Article gate

- [ ] Replace every `[EVIDENCE_*]`, `[BUILD]`, `[COMMIT]`, and `[PUBLIC_*]`
      placeholder with a verified value.
- [ ] Make each measured number link to its report.
- [ ] Use one product name: Saccade.
- [ ] Keep the CEF/Chromium engine status and experimental release status
      accurate.
- [ ] Give the nested-iframe failure its final disposition: fixed and tested,
      still blocked, or routed.
- [ ] Ask one engineer who did not write the article to reproduce the evidence
      links and challenge the strongest claim.
- [ ] Remove internal milestone labels and local filesystem paths.

## 6. Publishing mechanics

- [ ] Stable canonical URL without a date-dependent preview path.
- [ ] Page title under 60 characters and a plain-language description.
- [ ] Open Graph image at 1200 x 630 with no tiny benchmark text.
- [ ] `og:title`, `og:description`, `og:image`, canonical URL, author, publish
      date, and updated date.
- [ ] RSS/Atom entry and sitemap entry.
- [ ] Accessible video controls, captions or transcript, poster, and a reduced
      motion fallback.
- [ ] Descriptive alt text for diagrams and the preview.
- [ ] Mobile, narrow desktop, dark mode, keyboard, and 200% zoom checks.
- [ ] No autoplay audio, blocking signup modal, or analytics that exposes local
      evidence URLs.
- [ ] All external links return a successful status and all repository links
      point to public files.

## 7. Release sequence

### One week before

- [ ] Freeze the article claim set.
- [ ] Produce the real evidence pack and independent review.
- [ ] Prepare the live pages without indexing or announcements.

### One day before

- [ ] Test the production URL from a logged-out session and a phone.
- [ ] Confirm GitHub README, issue templates, SECURITY, and license links.
- [ ] Rehearse the local demo from a clean user profile.
- [ ] Read the Hacker News guidelines again.

### Publication day

- [ ] Publish the article and verify its canonical URL.
- [ ] Wait for caches and link previews to settle.
- [ ] Submit the original article title to Hacker News without editorializing.
- [ ] Wayne stays available to answer questions in his own words.
- [ ] Record bugs and recurring questions. Do not argue about votes.

### After publication

- [ ] Add corrections with a visible timestamp.
- [ ] Convert repeated questions into documentation or issues.
- [ ] Save traffic and download counts without collecting browser content.
- [ ] Decide whether the next artifact should be a form demo, release build, or
      Show HN based on the questions readers asked.
