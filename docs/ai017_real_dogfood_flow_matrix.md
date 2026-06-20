# AI-017 Real Dogfood Flow Matrix

Date: 2026-06-19
Status: automated flows mostly green; live logged-in draft flow still needs Wayne present

## Purpose

Use the packaged dogfood kit on a small evidence-first matrix. Do not promote
unknown sites by guesswork; record measured pass/fail and route limits.

Current kit:

```text
dist/saccade-dogfood-current -> saccade-dogfood-ai016-20260619-204157
Saccade commit at run start: 422e1b8
```

## Results

| Flow | Policy Lane | Result | Evidence |
| --- | --- | --- | --- |
| Package self-check | Green/local | PASS | `dist/saccade-dogfood-current/runs/ai017_flow_matrix/check_saccade.json`: `ok=true`, `storage=profile_dir`, `termination=graceful_servo_shutdown`. |
| FORMMAX long form/table fill | Green/local fixture | PASS | `dist/saccade-dogfood-current/runs/formmax/ai017_formmax_wrapper/result.json`: rows=96, pages=2, filled=672, blocked_sensitive=3, receipt_verified=true, value leak check passed. |
| Public article learning | Yellow/unmeasured public site | PASS | `dist/saccade-dogfood-current/runs/ai017_flow_matrix/article.json`: Rookies article, 9392 chars, selector `main.layout-content`, clean shutdown. |
| High-risk blocked-site fallback | Orange/App Store Connect | PASS | `runs/redacted_notes/note_1781920365720/note.json`: no live-site access, user-supplied redacted text, site policy orange, AI review packet written. |
| Local game reflex | Green/local game | PASS after runner resilience fix | `runs/local_game_reflex/ai017_local_game_reflex_after_partial_fix/report.json`: `live_game_reflex_readback_green`, 506/506 readbacks, 57 semantic facts, 17 commands/17 receipts, hp_delta=0. |
| Logged-in low-risk draft flow | Yellow/logged-in draft | PENDING HUMAN | Needs Wayne present in the same Saccade session. Use GitHub/Gist or another low-risk draft surface; do not submit/publish. |

## Notes

- The first local game attempt produced partial artifacts but no final report
  after a WebDriver execute abort. `scripts/run_local_game_reflex_loop.js` now
  writes `run_error` into replay and still produces `report.json` on preflight
  or runtime errors.
- The second local game run passed, so the local reflex capability remains
  green; the runner resilience fix is still useful for future failures.
- The public article remains `unmeasured_unknown` Yellow despite passing,
  because one successful article extraction does not promote the whole domain.
- App Store Connect remains Orange. Saccade should help via redacted notes and
  checklists, not live action/fill/release flows.

## Commands

```bash
dist/saccade-dogfood-current/check-saccade
dist/saccade-dogfood-current/run-formmax ai017_formmax_wrapper
dist/saccade-dogfood-current/read-article https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art ai017_rookies_article
node scripts/create_redacted_note_packet.js --source-url https://appstoreconnect.apple.com/apps --title "AI-017 App Store Connect blocked fallback" --task evaluate_edit --audience "developer handling App Store Connect" --text-file dist/saccade-dogfood-current/runs/ai017_flow_matrix/redacted_input.txt
SACCADE_REFLEX_DURATION_MS=5000 SACCADE_REFLEX_FACT_INTERVAL_MS=500 dist/saccade-dogfood-current/run-local-game-reflex http://127.0.0.1:4173/ ai017_local_game_reflex_after_partial_fix
```

## Next

Run the pending logged-in low-risk draft flow with Wayne present:

1. Open `dist/saccade-dogfood-current/open-saccade https://gist.github.com/new`.
2. Wayne completes login/2FA if needed.
3. Agent inspects editors without returning text values.
4. Agent fills a harmless draft title/body.
5. Agent verifies draft fields and stops before Create/Publish/Submit.
