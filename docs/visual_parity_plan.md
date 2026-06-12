# Saccade Visual Parity Plan

Date: 2026-06-12

## Decision

For real user-facing work, the user should see a mainstream browser rendering path. Chrome/Firefox visual parity is a product requirement, not a polish item.

Servo remains useful for the current evidence chain and controlled browser experiments, but UI review and ordinary product demos need Chrome/Firefox parity.

## Current v0

Add a small Chrome reference capture tool:

```bash
scripts/capture_chrome_reference.sh <url> <output-dir> [width] [height]
```

It uses an installed Chrome-family browser to render page content at a fixed viewport and writes:

- `chrome_page.png`
- `chrome_reference_manifest.json`

This proves the repo can produce a real Chrome-rendered reference artifact without changing the product runtime yet.

Privacy note: page screenshots capture visible page values. Use this script on local fixtures or non-sensitive pages only until redacted artifact capture exists.

## What This Is Not

This is not the final Chrome adapter.

It does not yet provide:

- browser UI / URL bar screenshots,
- CDP action maps,
- Chrome-side click verification,
- redacted agent truth from Chrome,
- replay integration.

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
2. Add Chrome adapter v0 for page truth and screenshots.
3. Add action map and click verification through Chrome/CDP.
4. Keep Servo as a controlled evidence engine until Chrome adapter coverage is strong enough.
