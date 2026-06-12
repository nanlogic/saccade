# Native Input Profile

Date: 2026-06-12

## Gate

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input
```

Observed result:

```text
NATIVE_INPUT PASS focused=true value_len=9 keydown=9 keypress=9 beforeinput=0 input=9 keyup=9 handled_keyboard=18 consumed_keyboard=0 dispatch_failed=0
```

The gate passed three consecutive local runs after switching the click target from a fixed coordinate to the input's measured `getBoundingClientRect()` center.

## What This Proves

- A Servo-loaded page can receive focus through Saccade's native mouse event path, using a measured page-space input center.
- `InputEvent::Keyboard(KeyboardEvent::from_state_and_key(...))` can insert plain ASCII text into a focused `<input>`.
- Page JavaScript observed normal `keydown`, `keypress`, `input`, and `keyup` events for `saccade42`.
- No keyboard dispatch failures were reported by `WebViewDelegate::notify_input_event_handled`.

## Current Caveats

- Pinned Servo `0.2.0` did not emit `beforeinput` for this path.
- `InputEventResult::Consumed` stayed false even though DOM value and input events updated, so Saccade should verify input by page state and DOM/user-visible evidence rather than that flag alone.
- FORMMAX still uses the trusted fixture DOM transaction runner. The next hardening step is to replace or supplement field writes with this native keyboard path.
