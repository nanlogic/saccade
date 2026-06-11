# Blockers

## CEF/Chromium fallback trigger

Because M-1 ended with `GO_SERVO_WITH_BACKUP`, pivot to a CEF/Chromium prototype if M0 or M1 shows that stock Servo cannot:

- build and boot a minimal embedded WebView on the target benchmark platform,
- produce rendered frame readback from the painted browser surface,
- deliver ordinary mouse move/down/up through the browser input pipeline, or
- load and operate the real mouseaccuracy.com classic page well enough for M1 recon.

Arena-only is not considered a replacement for the real browser claim.

## M0 macOS `servo-fonts` crate compile failure

On macOS arm64, `cargo check -p mousemax` with stock `servo-fonts 0.2.0` fails in `platform/macos/font.rs` with `E0716: temporary value dropped while borrowed`.

Workaround applied for M0: vendor `servo-fonts 0.2.0` under `vendor/servo-fonts-0.2.0` and patch only the temporary `CFString` lifetime. This is a local macOS build workaround, not a Servo version bump.

Status: workaround verified by `cargo check -p mousemax` and `cargo run -p mousemax -- selftest-boot` on macOS arm64.
