# AI-028 MouseAccuracy Original Gate

Date: 2026-07-06

## Goal

Verify the real MouseAccuracy site through the current source-release
ServoShell path, with a visible user window, instead of relying on the old
simplified arena or the legacy MOUSEMAX harness.

## Environment

- Saccade commit: `66433b5` plus local AI-028 probe script
- ServoShell: `/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell`
- Mode: release ServoShell, headed window
- Probe: `scripts/probe_mouseaccuracy_original_gate.py`

## Results

| Target | Result | Evidence |
| --- | --- | --- |
| `https://mouseaccuracy.com/classic/` | Green | `runs/ai028_mouseaccuracy_original/classic_gate_headed_release_2/report.json` |
| `https://mouseaccuracy.com/game` | Green | `runs/ai028_mouseaccuracy_original/modern_game_headed_release_3/report.json` |

Classic gate:

- Loaded the original classic page in a visible release ServoShell window.
- Selected Epic speed and Tiny target size.
- Started the game.
- Dispatched browser pointer actions to real `.target` elements.
- Score moved from `0` to `8`.

Modern gate:

- Loaded the original modern `/game` route in a visible release ServoShell
  window.
- Game countdown completed and the live game started.
- Page exposed one canvas plus `.target` DOM facts with usable rectangles.
- Dispatched browser pointer actions to two target rectangles.
- Score moved from `0` to `12`.

## Important Findings

- The current MouseAccuracy pages are not the simplified Saccade arena. These
  are real public site URLs.
- The modern page has a canvas layer, but the clickable targets are exposed as
  DOM `.target` elements, so the current truth/action route can use target
  rectangles instead of screenshot pixel guessing.
- The persistent macOS/Servo `GLD_TEXTURE_INDEX_2D` warning still appears, but
  it did not block either original MouseAccuracy gate.
- A repeated WebDriver screenshot loop during the modern game can timeout. The
  green route avoids hot-loop screenshot dependency by using DOM target facts.

## Non-Claims

- This is not yet a full 30-second highest-difficulty public benchmark run.
- This does not prove every WebGL/canvas-heavy site is healthy.
- This does not prove Chrome-perfect visual parity for the modern settings
  screen.
- This does not claim that MouseAccuracy currently uses WebGL directly. Current
  shipped assets observed by this probe expose canvas/DOM behavior; GL warnings
  are still tracked under the Servo/WebRender graphics path.

## Rerun

```bash
python3 scripts/probe_mouseaccuracy_original_gate.py \
  --mode classic \
  --url https://mouseaccuracy.com/classic/ \
  --window-size 1280x900 \
  --max-clicks 8 \
  --headed \
  --output-dir runs/ai028_mouseaccuracy_original/classic_gate_headed_release_rerun

python3 scripts/probe_mouseaccuracy_original_gate.py \
  --mode modern \
  --url https://mouseaccuracy.com/game \
  --window-size 1280x900 \
  --max-clicks 3 \
  --headed \
  --output-dir runs/ai028_mouseaccuracy_original/modern_game_headed_release_rerun
```

## Next

Promote this to AI-029 only if we need the public launch benchmark:

- full 30-second modern/or classic run,
- highest difficulty / smallest target settings where available,
- score/hit/miss summary,
- replay/review page,
- optional Chrome reference video for visual comparison.
