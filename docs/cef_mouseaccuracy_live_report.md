# CEF MouseAccuracy Live Action Report

Date: 2026-07-15

## Verdict

PASS. The signed CEF Release build operated the original MouseAccuracy site
through the Saccade owner bridge. Merely loading the page was not counted.

## Verified Chain

1. The renderer collector became ready after Vue hydration and exposed the
   visible `START` RouterLink as an action fact.
2. The browser adapter dispatched a native CEF pointer click using the action
   id and current page revision.
3. The renderer returned a matching verified receipt for `START`.
4. The page moved to `https://mouseaccuracy.com/game`.
5. The collector emitted live `.target` facts and the bridge produced matching
   verified receipts for 12 targets.

No CDP, WebDriver, screenshot, extension, host-supplied page coordinate, or
page JavaScript action call was used.

## Results

| Check | Result |
| --- | --- |
| START receipt | verified |
| Final route | `/game` |
| Live targets receipted | 12/12 |
| Median fact-to-receipt | 4.55 ms |
| P95 fact-to-receipt | 6.2 ms |
| Maximum fact-to-receipt | 6.2 ms |
| Collector error | none |

The unchanged hidden fixture was also rerun after the real-page changes:
100/100 hits, zero misses, and 3.9 ms p95.

## Rerun

```bash
python3 scripts/probe_cef_mouseaccuracy_live.py \
  --output-dir runs/cef_mouseaccuracy_live/live_rerun \
  --hits 12
```

## Remaining Boundary

This proves visible top-frame button, link, and DOM-target pointer actions. It
does not close keyboard/form safety, cross-frame facts, same-page action
invalidation, or replay. Those remain Day 4 gates.
