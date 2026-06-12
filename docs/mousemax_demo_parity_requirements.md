# MOUSEMAX Demo Parity Requirements

Date: 2026-06-11

## Problem

Saccade uses an embedded Servo window. It does not look like Chrome or Safari. A public demo must prove that Saccade ran the real `mouseaccuracy.com` page, not a local clone or a page styled to look similar.

The demo should show page parity, not browser chrome parity.

## Required Evidence

For the public MOUSEMAX demo, include a parity pack next to the benchmark result.

Required Saccade artifacts:

- `before.png`
- `after.png`
- `click_map.png`
- `result.json`
- `replay.jsonl`
- `validator.txt`

Required reference artifacts:

- Chrome screenshot of `https://mouseaccuracy.com/classic/` with the URL bar visible.
- Safari screenshot of `https://mouseaccuracy.com/classic/` with the URL bar visible.
- The screenshots must show the same visible page controls: spawn speed options, target size options, and Start button.
- If possible, capture Chrome and Safari at 1920x1080.
- Optional Chrome result screenshot after a manual or baseline run.
- Optional Chrome click-run video.

Do not crop out the browser URL bar in Chrome/Safari reference screenshots. The URL bar is part of the trust evidence.

## Click Comparison Status

The existing Saccade artifact is a replay-derived click map:

```text
runs/real/run_1781193985/click_map.png
```

That file shows the circles Saccade clicked. It is the current target-click evidence.

The Chrome/Safari artifacts are currently page references unless a Chrome result screenshot or video is added. Do not describe them as automated Chrome click baselines until the Chrome adapter produces a `run.json` or equivalent replay artifact.

Use this wording until Chrome adapter v0 exists:

```text
This comparison shows Saccade's verified click run next to Chrome/Safari references for the same public page.
The full Chrome-engine automated click comparison is a later adapter gate.
```

## Demo Framing

Say this plainly:

```text
Saccade is not trying to skin Servo as Chrome. The browser chrome looks different.
The page content, URL target, options, result text, and replay evidence show this is the real Mouse Accuracy site.
```

## Side-By-Side Layout

Show:

1. Chrome reference page with URL bar.
2. Safari reference page with URL bar.
3. Saccade before screenshot.
4. Saccade after screenshot.
5. Saccade click map.
6. Validator output.
7. Optional Chrome result screenshot or video if captured.

The viewer should see:

- same site URL,
- same page title or visible page text,
- same Epic/Tiny controls,
- same result wording,
- zero misses in `result.json`,
- replay-derived click map.

## Commands

Prepare the parity pack:

```bash
scripts/prepare_mousemax_parity_pack.sh runs/real/run_1781193985
```

Manual reference screenshots:

```text
runs/real/run_1781193985/chrome_options_urlbar.png
runs/real/run_1781193985/safari_options_urlbar.png
```

Optional result screenshots:

```text
runs/real/run_1781193985/chrome_result_urlbar.png
runs/real/run_1781193985/safari_result_urlbar.png
```

Optional video/reference artifacts:

```text
runs/real/run_1781193985/chrome_click_video.mp4
runs/real/run_1781193985/saccade_replay_video.mp4
```

## Minimum Bar For HN/YouTube

Do not publish a MOUSEMAX demo until the parity review page exists:

```text
runs/real/run_1781193985/parity_review.html
```

If Chrome/Safari screenshots are missing, say they are missing. Do not imply visual parity without the references.

## What This Does Not Prove

This does not prove Chrome-compatible rendering. It proves the tested Servo embedder loaded and operated the same public site.

Chrome compatibility comes later through the planned Chrome adapter.
