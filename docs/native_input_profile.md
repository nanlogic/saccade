# Native Input Profile

Date: 2026-06-12

## Gate

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input
```

Demo artifact gate:

```text
RUST_LOG=error cargo run -q -p saccade-shell -- selftest-native-input-demo
```

Observed result:

```text
NATIVE_INPUT PASS focused=true value_len=9 keydown=9 keypress=9 beforeinput=0 input=9 keyup=9 handled_keyboard=18 consumed_keyboard=0 dispatch_failed=0 select_value=gamma select_input=1 select_change=1 select_controls=1
```

Latest demo artifact result:

```text
NATIVE_INPUT_DEMO PASS select_value=gamma select_input=1 select_change=1 select_controls=1 report=/Users/waynema/Documents/GitHub/SACCADE/runs/native_input_demo/demo_1781386930568/report.json review=/Users/waynema/Documents/GitHub/SACCADE/runs/native_input_demo/demo_1781386930568/review.html
```

The gate passed three consecutive local runs after switching click targets from fixed coordinates to measured `getBoundingClientRect()` centers.

## What This Proves

- A Servo-loaded page can receive focus through Saccade's native mouse event path, using a measured page-space input center.
- `InputEvent::Keyboard(KeyboardEvent::from_state_and_key(...))` can insert plain ASCII text into a focused `<input>`.
- Page JavaScript observed normal `keydown`, `keypress`, `input`, and `keyup` events for `saccade42`.
- No keyboard dispatch failures were reported by `WebViewDelegate::notify_input_event_handled`.
- A trusted native click on `<select>` triggers Servo's `EmbedderControl::SelectElement`; Saccade can choose an option through the delegate, submit it back to Servo, and the page receives `input` plus `change`.
- The demo gate writes a small review page with before/after screenshots: `Alpha` before selection, `Gamma` after selection, plus `embedder_controls_shown=1`, `options_seen=3`, `input_events=1`, and `change_events=1`.

## Current Caveats

- Pinned Servo `0.2.0` did not emit `beforeinput` for this path.
- `InputEventResult::Consumed` stayed false even though DOM value and input events updated, so Saccade should verify input by page state and DOM/user-visible evidence rather than that flag alone.
- The screenshot evidence shows the select before and after selection. The transient embedder dropdown popup itself is represented by `embedder_controls_shown=1`, not by a captured OS popup frame.
- FORMMAX now uses the native keyboard path for one real text field before the full fixture DOM transaction. Native select/dropdown handling is proven separately but is not yet integrated into the FORMMAX `owner` field.
