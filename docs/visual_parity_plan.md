# Saccade Visual Parity Plan

Date: 2026-06-12

## Decision

For real user-facing work, the user should see a mainstream browser rendering path. Chrome/Firefox visual parity is a product requirement, not a polish item.

Servo remains useful for the current evidence chain and controlled browser experiments, but UI review and ordinary product demos need Chrome/Firefox parity.

## Current v0

Added a Chrome CDP reference capture path:

```bash
scripts/capture_chrome_reference.sh <url> <output-dir> [width] [height]
scripts/selftest_chrome_reference.sh
scripts/visual_parity_compare.py --timeout-sec 60
scripts/visual_parity_compare.py --timeout-sec 60 --rendering-profile servo-modern
scripts/selftest_visual_parity.sh
cargo run -q -p devmax -- audit --engine chrome --url <local-url> --replay
scripts/build_demo_comparison_pack.py --fixtures dashboard --timeout-sec 60
```

It uses an installed Chrome-family browser through the Chrome DevTools Protocol to render page content at a fixed viewport and writes:

- `chrome_page.png`
- `chrome_reference_manifest.json`
- `chrome_truth.json`
- `chrome_network.json`
- optional `chrome_click_verification.json` when Saccade action points are provided

The default `balanced` block policy uses CDP `Network.setBlockedURLs` to block common ad and analytics hosts before navigation completes. This is not a full ad-block extension; it is a deterministic stability policy for reference captures and local audits. The manifest records the policy mode, pattern hash, blocked request count, and network summary.

`devmax audit --engine chrome` now wraps this capture path and returns a normal DEVMAX report with Chrome screenshot/truth/network artifacts. `saccade.dev.audit_page(engine=chrome)` exposes the same path through MCP for local/file URLs.

`scripts/visual_parity_compare.py` now captures the same local fixture with Chrome and Saccade, computes pixel-diff metrics, verifies enabled non-sensitive Saccade action points against Chrome with a non-mutating hit-test, and writes an HTML comparison report with browser-frame previews plus Chrome/Saccade/diff columns. The current fixture set covers a dedicated layout probe, dashboard layout, forms, modal overlays, scroll/sticky behavior, canvas/SVG, and responsive cards.

The runner can select Saccade profiles with `--rendering-profile servo-safe|servo-modern|chrome-reference`. The legacy `--saccade-grid on|off` switch remains as a compatibility shim for the Grid experiment.

The browser-frame previews are report wrappers around page-content screenshots. They make URL context visible for public/demo review, but they are not native Chrome/Saccade browser-UI screenshots.

`scripts/build_demo_comparison_pack.py` now combines native Chrome/Safari browser UI capture attempts, visual parity evidence, and Chrome hit-test summaries into `demo_review.html`. On macOS hosts without Screen Recording permission it still produces the pack and records native screenshots as `capture_unavailable`.

Privacy note: page screenshots capture visible page values. Use this script on local fixtures or non-sensitive pages only until redacted artifact capture exists.

## What This Is Not

This is not the final Chrome adapter.

It does not yet provide:

- native browser UI / URL bar screenshots require macOS Screen Recording permission,
- human-profile Chrome session reuse,
- Firefox reference capture.

## Target Architecture

Use this boundary:

```text
Human View: Chrome/Firefox rendered page
Agent View: redacted actionable truth
Trusted Local Mediator: observes/classifies/masks locally, never exposes raw sensitive values to the LLM
Replay: records actions, statuses, and masked boundaries without secrets
```

## Next

1. Grant Screen Recording permission and rerun the demo pack to produce native Chrome/Safari screenshots.
2. Use Chrome reference screenshots in MOUSEMAX and DEVMAX parity pages.
3. Add user-profile/session handoff only behind explicit permission.
4. Keep Servo as a controlled evidence engine until Chrome adapter coverage is strong enough.
