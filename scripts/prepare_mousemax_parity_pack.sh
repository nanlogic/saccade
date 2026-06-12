#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="${1:-runs/real/run_1781193985}"
ABS_RUN_DIR="$ROOT/$RUN_DIR"

cd "$ROOT"

if [[ ! -f "$ABS_RUN_DIR/result.json" ]]; then
  echo "missing result.json in $ABS_RUN_DIR" >&2
  exit 1
fi

if [[ ! -f "$ABS_RUN_DIR/replay.jsonl" ]]; then
  echo "missing replay.jsonl in $ABS_RUN_DIR" >&2
  exit 1
fi

cargo run -q -p mousemax -- replay \
  "$RUN_DIR/replay.jsonl" \
  --summary \
  --render-summary "$RUN_DIR/click_map.png" \
  > "$ABS_RUN_DIR/replay_summary.txt"

cargo run -q -p mousemax -- validate-run "$RUN_DIR" --require-click-map \
  | tee "$ABS_RUN_DIR/validator.txt"

cat > "$ABS_RUN_DIR/demo_parity_manifest.json" <<EOF
{
  "url": "https://mouseaccuracy.com/classic/",
  "difficulty": {
    "spawn_speed": "Epic",
    "target_size": "Tiny",
    "duration_s": 15
  },
  "saccade_artifacts": {
    "before": "before.png",
    "after": "after.png",
    "click_map": "click_map.png",
    "result": "result.json",
    "replay": "replay.jsonl",
    "validator": "validator.txt",
    "replay_summary": "replay_summary.txt"
  },
  "comparison_status": {
    "saccade_verified_click_run": "complete",
    "chrome_urlbar_reference": "pending until chrome_options_urlbar.png is added",
    "safari_urlbar_reference": "pending until safari_options_urlbar.png is added",
    "chrome_automated_click_run": "deferred_until_chrome_adapter_v0"
  },
  "reference_artifacts_expected": {
    "chrome_options_urlbar": "chrome_options_urlbar.png",
    "safari_options_urlbar": "safari_options_urlbar.png",
    "chrome_result_urlbar_optional": "chrome_result_urlbar.png",
    "safari_result_urlbar_optional": "safari_result_urlbar.png",
    "chrome_click_video_optional": "chrome_click_video.mp4",
    "saccade_replay_video_optional": "saccade_replay_video.mp4"
  },
  "note": "Chrome/Safari screenshots should include the browser URL bar. Saccade screenshots are embedded Servo content screenshots without browser chrome. Full automated Chrome click comparison is deferred until the Chrome adapter gate."
}
EOF

image_or_placeholder() {
  local file="$1"
  local label="$2"
  if [[ -f "$ABS_RUN_DIR/$file" ]]; then
    printf '<img src="%s" alt="%s">' "$file" "$label"
  else
    printf '<div class="placeholder">Missing: %s<br>Capture %s with the URL bar visible.</div>' "$file" "$label"
  fi
}

chrome_options="$(image_or_placeholder chrome_options_urlbar.png "Chrome options reference")"
safari_options="$(image_or_placeholder safari_options_urlbar.png "Safari options reference")"
chrome_result="$(image_or_placeholder chrome_result_urlbar.png "Chrome result reference")"
safari_result="$(image_or_placeholder safari_result_urlbar.png "Safari result reference")"
chrome_click_reference="$(image_or_placeholder chrome_result_urlbar.png "Chrome click/reference result")"

cat > "$ABS_RUN_DIR/parity_review.html" <<EOF
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>MOUSEMAX Demo Parity Review</title>
  <style>
    body { margin: 0; background: #f5f6f8; color: #1f242b; font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    main { max-width: 1280px; margin: 0 auto; padding: 24px; }
    h1, h2, p { margin: 0; }
    h1 { font-size: 28px; margin-bottom: 8px; }
    h2 { font-size: 18px; margin: 24px 0 10px; }
    p { line-height: 1.45; }
    .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; align-items: start; }
    .two { grid-template-columns: repeat(2, 1fr); }
    figure { margin: 0; background: #fff; border: 1px solid #d7dce3; border-radius: 8px; padding: 10px; }
    figcaption { font-size: 13px; color: #59616c; margin-bottom: 8px; }
    img { width: 100%; height: auto; border: 1px solid #e0e4e9; background: #fff; display: block; }
    pre { white-space: pre-wrap; background: #111821; color: #e6edf7; border-radius: 8px; padding: 14px; overflow: auto; }
    .placeholder { min-height: 220px; display: grid; place-items: center; text-align: center; background: #fff7e6; border: 1px dashed #b88322; color: #5d3d00; border-radius: 6px; padding: 16px; }
    .note { background: #fff; border: 1px solid #d7dce3; border-radius: 8px; padding: 14px; margin-top: 14px; }
    .status { display: grid; grid-template-columns: repeat(4, 1fr); gap: 8px; margin: 16px 0 6px; }
    .status div { background: #fff; border: 1px solid #d7dce3; border-radius: 8px; padding: 10px; font-size: 13px; }
    .status strong { display: block; color: #20252c; margin-bottom: 3px; }
    .wide img { max-height: 620px; object-fit: contain; }
  </style>
</head>
<body>
<main>
  <h1>MOUSEMAX Demo Parity Review</h1>
  <p>URL under test: <code>https://mouseaccuracy.com/classic/</code></p>
  <div class="note">
    <p>Saccade uses an embedded Servo window, so it does not look like Chrome or Safari. This review proves page and artifact parity: same public URL, same visible controls, same result wording, replay log, click map, and validator output.</p>
  </div>
  <section class="status">
    <div><strong>Saccade click run</strong>Complete: replay + click map + validator.</div>
    <div><strong>Chrome URL reference</strong>Pending until <code>chrome_options_urlbar.png</code> is added.</div>
    <div><strong>Safari URL reference</strong>Pending until <code>safari_options_urlbar.png</code> is added.</div>
    <div><strong>Chrome click baseline</strong>Deferred until Chrome adapter v0.</div>
  </section>

  <h2>Target Click Evidence</h2>
  <section class="grid two">
    <figure class="wide"><figcaption>Saccade replay-derived click map</figcaption><img src="click_map.png" alt="Saccade click map"></figure>
    <figure class="wide"><figcaption>Chrome result/reference, optional</figcaption>$chrome_click_reference</figure>
  </section>
  <div class="note">
    <p>The current comparison shows Saccade's verified click run next to Chrome/Safari references for the same public page. It is not yet a Chrome automated click-run baseline.</p>
  </div>

  <h2>Options Page Reference</h2>
  <section class="grid">
    <figure><figcaption>Chrome reference with URL bar</figcaption>$chrome_options</figure>
    <figure><figcaption>Safari reference with URL bar</figcaption>$safari_options</figure>
    <figure><figcaption>Saccade / Servo before run</figcaption><img src="before.png" alt="Saccade before run"></figure>
  </section>

  <h2>Result Reference</h2>
  <section class="grid">
    <figure><figcaption>Chrome result reference, optional</figcaption>$chrome_result</figure>
    <figure><figcaption>Safari result reference, optional</figcaption>$safari_result</figure>
    <figure><figcaption>Saccade / Servo after run</figcaption><img src="after.png" alt="Saccade after run"></figure>
  </section>

  <h2>Replay Evidence</h2>
  <section class="grid two">
    <figure><figcaption>Replay-derived click map</figcaption><img src="click_map.png" alt="Saccade click map"></figure>
    <figure><figcaption>Validator output</figcaption><pre>$(sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' "$ABS_RUN_DIR/validator.txt")</pre></figure>
  </section>

  <h2>Checklist</h2>
  <pre>1. Chrome/Safari references show https://mouseaccuracy.com/classic/ in the URL bar.
2. Chrome/Safari/Saccade show the same Mouse Accuracy controls.
3. Saccade result.json shows Epic + Tiny, 47 hits, 0 misses, instrumentation=none.
4. Saccade replay.jsonl and click_map.png exist.
5. validator.txt reports VALIDATE PASS.</pre>
</main>
</body>
</html>
EOF

echo "MOUSEMAX PARITY PACK READY run=$RUN_DIR"
echo "review=$RUN_DIR/parity_review.html"
echo "manifest=$RUN_DIR/demo_parity_manifest.json"
echo "Add Chrome/Safari URL-bar screenshots before publishing."
