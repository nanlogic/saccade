# AI-017 Real Dogfood Flow Matrix

Date: 2026-06-19
Status: complete

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
| Logged-in low-risk draft flow | Yellow/logged-in draft | PASS | `dist/saccade-dogfood-current/runs/ai017_gist_live/`: Wayne completed login in the same Saccade session, Saccade navigated to `https://gist.github.com/new`, inspected 7 editor candidates, filled 3 harmless draft fields, verified editor state, and did not submit/publish. |

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
- The Gist flow initially opened to `https://gist.github.com/starred` after
  login/profile restoration. The bridge navigation call moved the same session
  to `https://gist.github.com/new` before filling. This keeps login/2FA
  human-owned while allowing same-session draft assistance.
- Gist replay artifacts did not contain the draft strings used for the test;
  the control replay records field counts, lengths, policy, and verification,
  with `values_logged=false`.
- `open-saccade` on the real GitHub URL felt broken to the user because the
  visible window/ready state lagged behind process startup. The process was
  running and eventually wrote the grant, but this should become a launch UX
  follow-up: foreground/activate the window reliably and expose a visible
  "launching/ready" state.

## Commands

```bash
dist/saccade-dogfood-current/check-saccade
dist/saccade-dogfood-current/run-formmax ai017_formmax_wrapper
dist/saccade-dogfood-current/read-article https://www.therookies.co/blog/breakdowns/step-by-step-guide-blender-environment-art ai017_rookies_article
node scripts/create_redacted_note_packet.js --source-url https://appstoreconnect.apple.com/apps --title "AI-017 App Store Connect blocked fallback" --task evaluate_edit --audience "developer handling App Store Connect" --text-file dist/saccade-dogfood-current/runs/ai017_flow_matrix/redacted_input.txt
SACCADE_REFLEX_DURATION_MS=5000 SACCADE_REFLEX_FACT_INTERVAL_MS=500 dist/saccade-dogfood-current/run-local-game-reflex http://127.0.0.1:4173/ ai017_local_game_reflex_after_partial_fix
dist/saccade-dogfood-current/open-saccade https://gist.github.com/new
# Wayne logs in, then:
# shell_status -> inspect_editors -> navigate https://gist.github.com/new -> inspect_editors -> draft_editor_fill -> inspect_editors
```

## Next

Follow up on launch UX:

1. `open-saccade` should foreground/activate the ServoShell window reliably.
2. Long real-site startup should show a clear launching/ready state.
3. The bridge should avoid making the user think no browser opened while
   WebDriver/session readiness is still settling.
