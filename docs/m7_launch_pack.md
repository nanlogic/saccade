# Saccade M7 Launch Pack

This pack assumes the public artifact will be the benchmark report or project site. Submit that original URL to Hacker News. Cross-post to DEV only after setting a canonical URL.

## Posting Order

1. Publish the benchmark report on the project site or GitHub.
2. Upload a short YouTube demo.
3. Add the YouTube link near the top of the report.
4. Cross-post to DEV with `canonical_url` pointing to the original report.
5. Submit the original report URL to Hacker News as a Show HN.

## Hacker News

Use Show HN if the repo or article lets readers inspect and run the work. Keep the title plain.

Title options:

- `Show HN: Saccade, a Servo runner for the Mouse Accuracy benchmark`
- `Show HN: A stock-Servo browser runner that plays Mouse Accuracy`
- `Show HN: Saccade, a browser reflex loop built on Servo`

Suggested first comment:

```text
I built Saccade to test whether a stock Servo embedder can run a browser-owned reflex loop against mouseaccuracy.com/classic.

The M7 run passes Epic + Tiny at 1920x1080. The report includes five consecutive observe-only runs and one pixel-only run. The pixel-only run disables DOM target data and uses rendered RGBA pixels for detection.

The realtime loop makes no LLM calls. Servo provides the rendered pixels, layout probes where enabled, and input dispatch. The replay logs include monotonic timestamps for target first-seen, decision, and input dispatch.

I would like feedback on the measurement, the benchmark framing, and whether the pixel-only path is a fair stock-browser claim.
```

HN checklist:

- Submit the original report URL, not the YouTube URL.
- Do not ask anyone to upvote.
- Reply to technical criticism with measured artifacts.
- Keep the title free of hype words.
- Put the repo link and YouTube link inside the report.

References:

- HN Show rules: `https://news.ycombinator.com/showhn.html`
- HN guidelines: `https://news.ycombinator.com/newsguidelines.html`

## DEV Post

Front matter:

```yaml
---
title: "Saccade: a stock-Servo runner for the Mouse Accuracy benchmark"
published: false
tags: rust, browser, servo, benchmark
canonical_url: https://YOUR_SITE/YOUR_REPORT_URL
---
```

Opening:

```markdown
I wanted a browser benchmark where the browser has to perceive, decide, and act inside a real page.

Saccade runs `mouseaccuracy.com/classic/` in stock Servo, selects Epic spawn speed and Tiny target size, and clicks targets through Servo input events. The realtime loop makes no LLM calls.

The M7 run passed at 1920x1080:

- Five consecutive observe-only runs
- One pixel-only run with DOM target data disabled
- Zero misses in every acceptance run
- Pixel-only detect p95: 6.3 ms
- Pixel-only first-visible-to-dispatch p95: 16.0 ms

The benchmark report has the replay paths, screenshots, commands, and caveats.
```

DEV reference:

- Editor guide and `canonical_url`: `https://dev.to/p/editor_guide`

## YouTube Video

Target length: 60 to 90 seconds.

Before recording, prepare the parity pack:

```bash
scripts/prepare_mousemax_parity_pack.sh runs/real/run_1781193985
```

Add Chrome and Safari reference screenshots with the URL bar visible:

```text
runs/real/run_1781193985/chrome_options_urlbar.png
runs/real/run_1781193985/safari_options_urlbar.png
```

Recording command:

```bash
scripts/record_m7_demo.sh runs/real/m7_pixel_demo.mov
```

If macOS returns no video, grant Screen Recording permission to the terminal or Codex app in System Settings, then rerun the script.

Shot list:

1. Parity: show Chrome and Safari at `https://mouseaccuracy.com/classic/` with URL bars visible.
2. Terminal: show the Saccade command with `--site real --spawn-speed epic --target-size tiny --instrumentation none`.
3. Browser window: show the 1920x1080 Saccade run in progress.
4. Terminal: show final JSON with `PASS`, hits, misses, and detector counts.
5. Artifacts: show `before.png`, `after.png`, `click_map.png`, `result.json`, `replay.jsonl`, and `validator.txt`.
6. Optional: show `parity_review.html`.

Suggested title:

`Saccade M7: stock Servo plays Mouse Accuracy at Epic + Tiny`

Suggested description:

```text
Saccade runs mouseaccuracy.com/classic in stock Servo and passes Epic + Tiny at 1920x1080.

This demo shows the pixel-only M7 artifact: DOM target data disabled, rendered RGBA pixels used for target detection, Servo input events used for clicks, and no LLM calls inside the realtime loop.

Benchmark report:
YOUR_REPORT_URL

Repo:
YOUR_REPO_URL
```

## Website Placement

Put the report at a stable URL:

`/benchmarks/mouseaccuracy-m7/`

Page structure:

- One sentence result.
- Video embed.
- Acceptance table.
- Pixel-only artifact links.
- Commands to reproduce.
- Caveats.
- Repo link.

Keep the page technical. Readers should see the run data before any story.

## Visual Parity

Do not publish a demo that relies only on the Servo window. The embedded browser chrome looks different from Chrome and Safari. Include `parity_review.html` so readers can compare the public site in Chrome/Safari against the Saccade artifacts.
