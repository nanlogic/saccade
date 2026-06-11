# Saccade M7 Benchmark Report

Date: 2026-06-11

Saccade runs `https://mouseaccuracy.com/classic/` in stock Servo, selects Epic spawn speed and Tiny target size, detects targets from browser-owned evidence, and sends Servo input events. The realtime loop makes zero LLM calls.

## Result

M7 passed.

The runner completed five consecutive real-site `observe_only` runs at 1920x1080. It also completed one real-site `instrumentation=none` run, where detection used only RGBA pixels read from the rendered frame.

## Acceptance Evidence

| Run | Instrumentation | Result | Hits | Misses | Targets Seen | Clicks | False Positives | Unknown | Detect p95 ms | First Visible to Dispatch p95 ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `run_1781193407` | `observe_only` | PASS | 47 | 0 | 47 | 47 | 0 | 0 | 0.1 | 10.0 |
| `run_1781193433` | `observe_only` | PASS | 47 | 0 | 47 | 47 | 0 | 0 | 0.1 | 8.0 |
| `run_1781193458` | `observe_only` | PASS | 48 | 0 | 48 | 48 | 0 | 0 | 0.1 | 6.3 |
| `run_1781193482` | `observe_only` | PASS | 47 | 0 | 47 | 47 | 0 | 0 | 0.1 | 8.0 |
| `run_1781193507` | `observe_only` | PASS | 47 | 0 | 47 | 47 | 0 | 0 | 0.1 | 8.0 |
| `run_1781193985` | `none` | PASS | 47 | 0 | 47 | 47 | 0 | 0 | 6.3 | 16.0 |

Strong artifact:

- Result: `/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/result.json`
- Replay: `/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/replay.jsonl`
- Replay click map: `/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/click_map.png`
- Before screenshot: `/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/before.png`
- After screenshot: `/Users/waynema/Documents/GitHub/SACCADE/runs/real/run_1781193985/after.png`

Both screenshots are 1920x1080 PNG files.

## Commands

Five-run stability:

```bash
for i in 1 2 3 4 5; do
  RUST_LOG=error cargo run -q -p mousemax -- run \
    --site real \
    --spawn-speed epic \
    --target-size tiny \
    --duration 15 \
    --window-width 1920 \
    --window-height 1080 \
    --replay
done
```

Pixel-only run:

```bash
RUST_LOG=error cargo run -q -p mousemax -- run \
  --site real \
  --spawn-speed epic \
  --target-size tiny \
  --duration 15 \
  --window-width 1920 \
  --window-height 1080 \
  --instrumentation none \
  --replay
```

Validation:

```bash
cargo check -p mousemax
cargo test -p saccade_detect
scripts/e2e_arena.sh
```

All three passed.

## Implementation Notes

Saccade uses Servo `WebView`, `WindowRenderingContext`, `read_to_image`, and `notify_input_event`. The runner owns the window, the rendered pixels, the target detector, the tracker, the motor policy, and the replay log.

Two detection modes matter for M7:

- `observe_only`: reads visible `.target` rectangles from the live page and uses Servo input events for clicks.
- `none`: disables DOM target data and detects red targets from rendered RGBA pixels.

The pixel-only path uses a red connected-component detector with sparse seed scanning. It does not read target DOM state. It still reads the page score at the end, because the benchmark result needs the site's own hit and miss counters.

The motor clicks each target once, rejects stale reports, and stops target dispatch near the 15 second boundary. That end guard prevents a click against a target rectangle that arrived after the site already hid the game area.

## Measured Caveat

The replay timestamps measure when `notify_input_event` returned from Servo's embedder API. They do not prove when page script processed the mouse event. Phase E can add in-engine input receipts if that distinction matters.

## Milestone Template

MILESTONE: M7 Real site defeated

GATE: Real-site Epic+Tiny, 5 consecutive runs, replay logs, screenshots, plus one pure pixel run -> PASS

MEASURED: observe-only p95 first-visible-to-dispatch ranged from 6.3 ms to 10.0 ms. Pixel-only p95 detect was 6.3 ms and p95 first-visible-to-dispatch was 16.0 ms. Every M7 acceptance run had 0 misses, 0 false positives, 0 unknown verifications, and `hits == targets_seen == clicks_sent`.

DEVIATIONS: The run used macOS arm64 with stock Servo `0.2.0` and Rust `1.88.0`. The project spec still names Linux/X11 as the final benchmark target.

SERVO API NOTES: Stock Servo APIs covered M7: `WebView`, `WindowRenderingContext`, `read_to_image`, `evaluate_javascript`, and `notify_input_event`. No Servo fork was needed.

RISKS RAISED/RETIRED: Retired the real-site compatibility risk for `mouseaccuracy.com/classic/` on the tested stock-Servo/macOS path. Remaining risk: repeat the full gate on the final Linux/X11 target.

NEXT: M8 agent API, replay visualization, and launch packaging.
