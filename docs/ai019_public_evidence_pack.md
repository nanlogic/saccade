# AI-019 Public Evidence Pack

Date: 2026-06-20
Status: complete

## Scope

This is the current publishable evidence packet for Saccade dogfood. It freezes
what is proven, what is not proven, and which commands/artifacts another
session can rerun.

Current local kit:

```text
dist/saccade-dogfood-current -> saccade-dogfood-ai016-20260619-204157
Saccade commit: 1028bd6
Branch: main
Default runtime: ServoShell 0.3 bridge
ServoShell binary: /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell
```

## Proven Claims

| Claim | Evidence | Public wording |
| --- | --- | --- |
| Visible dogfood launch works | `docs/ai018_dogfood_launch_visibility.md`; `dist/saccade-dogfood-current/runs/servoshell_bridge/report.json` recorded `visible_bootstrap=true`, `foreground_attempted=true`, target `https://gist.github.com/new`. | Saccade opens visibly, shows a local launch page, then attaches the bridge and navigates to the target tab. |
| Package self-check works | `dist/saccade-dogfood-current/check-saccade`; `dist/saccade-dogfood-current/runs/check/bridge_smoke/report.json` has `ok=true`, `storage=profile_dir`, `termination=graceful_servo_shutdown`. | The current dogfood kit can validate itself and writes machine-readable evidence. |
| Public article extraction works | `dist/saccade-dogfood-current/runs/ai017_flow_matrix/article.json`; The Rookies article title correct, `bodyTextLength=9763`, clean shutdown. | Saccade can read long public tutorial pages into cleaner article/source packets than raw page HTML. |
| FORMMAX long form fill works | `dist/saccade-dogfood-current/runs/formmax/ai017_formmax_wrapper/result.json`; rows=96, pages=2, filled=672, blocked_sensitive=3, `receipt_verified=true`, validation_errors=0, leak check passed. | Saccade can fill large non-sensitive form/table workflows while preserving sensitive fields for the user. |
| Local game reflex works | `runs/local_game_reflex/ai008d_live_game_release_1781810191/report.json`; 1292/1292 readbacks, 176 semantic facts, 53 commands/receipts, `fill_delta=12`, `hp_delta=0`. | Saccade can use browser facts plus low-latency motor receipts on a live local game. |
| Same-session logged-in draft help works | `dist/saccade-dogfood-current/runs/ai017_gist_live/draft_editor_fill.json`; Wayne completed login/2FA, Saccade filled 3 visible draft fields, wrote 191 chars, `submit_attempted=false`, `values_logged=false`. | The user can log in, then Saccade can draft in the same visible session without taking over login or publishing. |
| High-risk fallback works | `runs/redacted_notes/note_1781920365720/`; App Store Connect redacted note packet created without live-site action. | For high-risk sites, Saccade can still help by analyzing redacted user-provided text and producing checklists/drafts. |

## Commands To Rerun

```bash
dist/saccade-dogfood-current/check-saccade
dist/saccade-dogfood-current/open-saccade https://gist.github.com/new
dist/saccade-dogfood-current/read-article https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art ai019_rookies_article
dist/saccade-dogfood-current/run-formmax ai019_formmax
SACCADE_REFLEX_DURATION_MS=15000 dist/saccade-dogfood-current/run-local-game-reflex http://127.0.0.1:4173/ ai019_local_game_reflex
```

Logged-in draft flow rerun:

```text
1. Run: dist/saccade-dogfood-current/open-saccade https://gist.github.com/new
2. User completes login/2FA in the visible Saccade window.
3. Agent uses the existing bridge grant to inspect editors and fill draft-only
   fields.
4. User reviews and decides whether to submit/publish.
```

## Human-In-Loop Publishing Policy

Do not add a broad "block everything" layer for ordinary drafting.

Use this product behavior:

- Agent may draft, edit, fill non-sensitive fields, verify visible state, and
  point out the submit/publish control.
- Final publish/submit/delete/release/payment/security actions require a
  current user gesture or explicit current-session confirmation.
- Login, password, OTP, CAPTCHA, account recovery, payment, legal signature,
  and security settings stay human-owned.

This keeps normal posting workflows usable while preserving accountability for
side effects.

## Do Not Claim

- Do not claim Chrome/Safari visual parity for all sites.
- Do not claim GitHub account-menu/dropdown parity; current evidence routes
  that to Servo API compatibility gaps around `IntersectionObserver` and
  adopted stylesheets.
- Do not claim high-risk site live operation for App Store Connect, cloud
  consoles, government, healthcare, finance, or payment flows.
- Do not claim anti-bot/CAPTCHA bypass.
- Do not claim a signed/notarized public macOS app yet.
- Do not claim general WebGL parity. Current local-game reflex evidence is
  green, but broader Canvas/WebGL sites remain measured case by case.
- Do not claim agent-owned automatic publishing. The proven user flow is
  "agent drafts, human posts."

## Video / Article Shot List

1. Show `check-saccade` returning JSON with `ok=true`.
2. Run `open-saccade https://gist.github.com/new`; show the local launch page,
   foregrounded window, then target navigation.
3. Run FORMMAX; show 96 rows / 2 pages filled, 3 sensitive fields preserved for
   the user, and replay/report artifacts.
4. Run `read-article` on The Rookies article; show article text extraction and
   clean exit.
5. Run local game reflex; show semantic facts, command receipts, and
   `review.html`.
6. Show the logged-in Gist draft flow: user logs in, Saccade fills draft fields,
   no submit/publish, no value logging.
7. Show redacted App Store Connect fallback as the safety boundary.

## Next Real-Site Matrix

Use `docs/ai020_human_in_loop_site_matrix.md` as the canonical next matrix.
Start with low-risk draft surfaces and record measured evidence before making
site-specific claims:

| Target | Purpose | Expected behavior |
| --- | --- | --- |
| Owned GitHub issue/discussion draft | Developer workflow proof | User logs in; Saccade drafts title/body; user submits if desired. |
| Hacker News / Discourse / dev.to draft | Public posting workflow proof | Saccade drafts/edits; user posts. |
| Reddit / LinkedIn / Facebook draft | Common social/forum proof | Draft only first; user posts; record provider friction and reputation risk. |
| AdMob/App Store Connect redacted packet | Safety-boundary proof | No live fill/submit; user supplies redacted text for analysis. |

These are not required for the current evidence pack. They are the next
dogfood expansion if we want broader proof of human-agent collaboration.
