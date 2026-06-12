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
scripts/selftest_visual_parity.sh
cargo run -q -p devmax -- audit --engine chrome --url <local-url> --replay
```

It uses an installed Chrome-family browser through the Chrome DevTools Protocol to render page content at a fixed viewport and writes:

- `chrome_page.png`
- `chrome_reference_manifest.json`
- `chrome_truth.json`
- `chrome_network.json`

The default `balanced` block policy uses CDP `Network.setBlockedURLs` to block common ad and analytics hosts before navigation completes. This is not a full ad-block extension; it is a deterministic stability policy for reference captures and local audits. The manifest records the policy mode, pattern hash, blocked request count, and network summary.

`devmax audit --engine chrome` now wraps this capture path and returns a normal DEVMAX report with Chrome screenshot/truth/network artifacts. `saccade.dev.audit_page(engine=chrome)` exposes the same path through MCP for local/file URLs.

`scripts/visual_parity_compare.py` now captures the same local fixture with Chrome and Saccade, computes pixel-diff metrics, and writes an HTML comparison report with Chrome/Saccade/diff columns. The current fixture set covers dashboard layout, forms, modal overlays, scroll/sticky behavior, canvas/SVG, and responsive cards.

Privacy note: page screenshots capture visible page values. Use this script on local fixtures or non-sensitive pages only until redacted artifact capture exists.

## What This Is Not

This is not the final Chrome adapter.

It does not yet provide:

- browser UI / URL bar screenshots,
- Chrome-side click verification,
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

1. Use Chrome reference screenshots in MOUSEMAX and DEVMAX parity pages.
2. Root-cause the layout differences exposed by `runs/visual_parity/parity_1781288579228/index.html`.
3. Add Chrome-side click verification through CDP.
4. Add user-profile/session handoff only behind explicit permission.
5. Keep Servo as a controlled evidence engine until Chrome adapter coverage is strong enough.
