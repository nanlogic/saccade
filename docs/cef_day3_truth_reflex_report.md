# CEF Day 3 Truth + Reflex Report

Date: 2026-07-14
Status: PASS for the bounded DOM-target gate

## Result

Saccade's CEF engine now carries redacted renderer facts to the browser
process over CEF IPC, exposes them through the owner-only engine transport,
dispatches native CEF pointer input, and returns a renderer-observed receipt.
The measured loop does not use CDP, WebDriver, screenshots, or an extension.

The unchanged `test_pages/chrome_truth_reflex/index.html` fixture passed three
independent Release runs:

| Run | Hits | Misses | Sensitive leaks | Full-loop p95 |
| --- | ---: | ---: | ---: | ---: |
| 1 | 100/100 | 0 | 0 | 3.2 ms |
| 2 | 100/100 | 0 | 0 | 3.2 ms |
| 3 | 100/100 | 0 | 0 | 3.2 ms |

Across the runs, renderer-fact-to-host p95 was 1.147-1.315 ms and host
receive-to-dispatch-start p95 was 0.001 ms. The largest full-loop sample was
5.8 ms, below the 20 ms p95 gate.

Evidence:

- `runs/cef_truth_reflex/day3_3x100_final-20260714/aggregate.json`
- `runs/cef_truth_reflex/day3_3x100_final-20260714/run1/report.json`
- `runs/cef_truth_reflex/day3_3x100_final-20260714/run2/report.json`
- `runs/cef_truth_reflex/day3_3x100_final-20260714/run3/report.json`

## Data boundary

The collector is installed in `CefRenderProcessHandler::OnContextCreated`,
before page scripts run. It captures its native emitter in a closure and then
deletes the page-global reference. It does not add attributes or elements to
the page. Target identity stays in a renderer `WeakMap`.

The browser process accepts only allowlisted message shapes. Sensitive
controls emit their kind and completion state; their values never cross the
renderer boundary. The SSN and password sentinels were absent from every
report and browser log.

The host acts by `action_id` plus an exact page revision. Coordinates are kept
inside the browser adapter. Input uses `CefBrowserHost::SendMouseMoveEvent`
and `SendMouseClickEvent`, and a renderer capture listener verifies the page
receipt.

## Product boundary

This proves the DOM target truth/reflex path. It does not prove canvas,
WebGL, shadow DOM, cross-frame truth, hostile-page integrity, safe form fill,
or replay. Keyboard dispatch remains disabled until Day 4 adds focused-field
ownership and sensitive-field policy; exposing an unguarded keyboard primitive
would weaken the existing safety boundary.

The gate uses hidden incognito state and Chromium's test-only mock keychain.
Normal product profiles still use the platform credential store.

## Commands

```sh
engines/cef/scripts/build_macos.sh
engines/cef/scripts/test_day3_macos.sh
engines/cef/scripts/test_day2_macos.sh
cargo test
```
