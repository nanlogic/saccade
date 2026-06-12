# Native Input Profile

Date: 2026-06-12

## Gate

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input
```

Observed result:

```text
NATIVE_INPUT PASS focused=true value_len=9 keydown=9 keypress=9 beforeinput=0 input=9 keyup=9 handled_keyboard=18 consumed_keyboard=0 dispatch_failed=0 select_value=gamma select_input=1 select_change=1 select_controls=1
```

The gate passed three consecutive local runs after switching click targets from fixed coordinates to measured `getBoundingClientRect()` centers.

## What This Proves

- A Servo-loaded page can receive focus through Saccade's native mouse event path, using a measured page-space input center.
- `InputEvent::Keyboard(KeyboardEvent::from_state_and_key(...))` can insert plain ASCII text into a focused `<input>`.
- Page JavaScript observed normal `keydown`, `keypress`, `input`, and `keyup` events for `saccade42`.
- No keyboard dispatch failures were reported by `WebViewDelegate::notify_input_event_handled`.
- A trusted native click on `<select>` triggers Servo's `EmbedderControl::SelectElement`; Saccade can choose an option through the delegate, submit it back to Servo, and the page receives `input` plus `change`.

## Current Caveats

- Pinned Servo `0.2.0` did not emit `beforeinput` for this path.
- `InputEventResult::Consumed` stayed false even though DOM value and input events updated, so Saccade should verify input by page state and DOM/user-visible evidence rather than that flag alone.
- FORMMAX now uses the native keyboard path for one real text field before the full fixture DOM transaction. Native select/dropdown handling is proven separately but is not yet integrated into the FORMMAX `owner` field.
