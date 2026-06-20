# Saccade Dogfood Handoff

Date: 2026-06-15

## Purpose

This file keeps the game-building session and the Saccade browser session from
overlapping.

The game session owns the game. The Saccade session owns browser truth, safety,
input, replay, and dogfood diagnostics.

## Current Saccade Baseline

```text
Use the latest SACCADE main branch unless Wayne says otherwise.
```

The local reflex runner defaults to `--policy visual`, uses live
`semantic_object_seen` facts to choose motor commands, and writes a
human-readable `review.html` at the end of each successful run.

## Ownership

Game session:

- May edit only the game repository.
- Verifies gameplay in Chrome first.
- Uses Saccade as a dogfood browser and evidence source.
- Does not patch the game around Saccade bugs unless the game is relying on
  invalid or fragile browser behavior.

Saccade session:

- May edit only this repository.
- Maintains the browser fact stream, semantic classification, reflex motor,
  safety policy, replay, and report artifacts.
- Investigates pages where Chrome passes but Saccade fails.
- Keeps failures reproducible through local fixtures, reports, or run artifacts.

## Triage Rule

Use this first question:

```text
Does Chrome fail too?
```

- Chrome fails: treat it as a game bug and fix it in the game session.
- Chrome passes, human Saccade fails: treat it as a Saccade browser/render/input
  bug.
- Chrome passes, human Saccade passes, agent Saccade fails: treat it as a
  Saccade truth/detector/motor/replay bug.

## Issue Report Format

When handing a problem to the Saccade session, append a short report with this
shape:

```text
Title:
Game commit:
Game URL:
Saccade commit:
Browser tested:
Expected:
Actual:
Repro steps:
Evidence:
Classification guess:
Does Chrome pass:
Does human Saccade pass:
Does agent Saccade pass:
```

Allowed classification guesses:

```text
render
input
resize
canvas
performance
agent_facts
motor
safety
unknown
```

## Current Reflex Acceptance

For the local game reflex gate, the Saccade session should produce:

- a release ServoShell run,
- `report.json`,
- `replay.jsonl`,
- `facts.jsonl`,
- `semantic_facts.jsonl`,
- `review.html`,
- command receipts,
- a short summary of `fill_delta`, `hp_delta`, `drop_delta`, command count,
  semantic fact count, and dispatch latency.

The game session can keep iterating while Saccade bugs are being investigated,
as long as Chrome remains a valid gameplay reference.

## Command For The Game Session

When the local game is running at `http://127.0.0.1:4173/`, the game session can
ask the Saccade session to run:

```bash
node scripts/run_local_game_reflex_loop.js \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url http://127.0.0.1:4173/ \
  --headless \
  --window-size 1280x900 \
  --duration-ms 15000 \
  --policy visual \
  --visual-fact-interval-ms 1000 \
  --output-dir runs/local_game_reflex/<run_name>
```

The command prints `report`, `review`, `replay`, `facts`, and
`semantic_facts` paths. `review.html` is the first artifact to open for a quick
human read.

## Prompt For Other Codex Sessions

Give another session this prompt when it needs to dogfood web work through
Saccade:

```text
Use Saccade first for local/owned/public low-risk browser checks.

Repo:
/Users/waynema/Documents/GitHub/SACCADE

Before using Saccade, read:
- docs/dogfood_browser_quickstart.md
- docs/site_policy_matrix.md
- docs/SACCADE_DOGFOOD_HANDOFF.md
- docs/dogfood_release_plan.md

Preferred same-machine dogfood release:
./scripts/build_dogfood_release.sh
dist/saccade-dogfood-current/check-saccade
dist/saccade-dogfood-current/open-saccade <URL>

For public article/tutorial learning pages, prefer the one-shot reader:
dist/saccade-dogfood-current/read-article <URL>

This waits for the ServoShell bridge page, extracts article/main text through
`article_text`, writes a JSON report under the kit's `runs/article/`, and exits.
Use it for Chrome-vs-Saccade speed/accuracy/token comparisons on long learning
pages.

The dogfood release defaults to ServoShell 0.3 bridge. Do not use the legacy
embedded `saccade-shell browse` path unless the Saccade session explicitly asks
for an old Servo 0.2 regression check.

Optional legacy fallback for old regression checks only:
SACCADE_INCLUDE_LEGACY_SHELL=1 ./scripts/build_dogfood_release.sh
dist/saccade-dogfood-<timestamp>/open-legacy-saccade <URL>

For exact mainstream rendering, use Chrome/reference as the comparison browser.
For local game reflex evidence, ask the Saccade session to run
dist/saccade-dogfood-current/run-local-game-reflex http://127.0.0.1:4173/.

Safety policy:
- Green: local dev, file fixtures, public docs/pages, owned domains. Saccade can read/click/fill non-sensitive fields.
- Yellow: logged-in low-risk dashboards, GitHub/Gist/forum drafts. Saccade may draft/fill/check, but submit/publish/delete requires the user.
- Orange: App Store Connect, cloud consoles, government forms, healthcare, financial, job/reputation workflows. Use Saccade only for redacted analysis/checklists/drafts.
- Red: login, password, OTP, CAPTCHA, account recovery, payment, legal signature, security settings. Human-only.

Never ask Saccade to bypass anti-bot, CAPTCHA, login, payment, release, signing,
or security controls.

If a high-risk site blocks Saccade:
1. Record the visible error/request id if safe.
2. Do not screenshot sensitive content.
3. Let the user handle login/submit/payment/release manually in Safari/Chrome/the official app.
4. If AI help is needed, create a redacted-note packet from user-supplied
   redacted text.
5. Read the generated ai_review_prompt.md and answer with:
   Risk And Context Assessment
   Questions For Human
   Edited Draft
   Final Human Confirmation Checklist
```

Convenience command for step 4:

```bash
node scripts/create_redacted_note_packet.js \
  --source-url https://appstoreconnect.apple.com/apps \
  --title "App Store Connect review note" \
  --task evaluate_edit \
  --audience "Apple app review reply" \
  --text-file /path/to/redacted-note.txt
```

The script calls `saccade.report.redacted_note` through the MCP stdio server and
writes `note.json`, `redacted_note.md`, and `ai_review_prompt.md` under
`runs/redacted_notes/note_*/`. It does not access the live site.
