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

## MOUSEMAX Chrome visual parity gap

A non-engineering viewer can see that the current Servo window does not look like Chrome/Safari on `mouseaccuracy.com`. The mismatch may come from browser engine support, CSS/layout differences, font metrics, viewport/device-scale behavior, and site JavaScript choosing a different code path.

Status: deferred. The current MOUSEMAX evidence proves replayable action correctness on the real public URL, not Chrome visual equivalence. Before public marketing, resolve this through Chrome adapter v0 or a visual parity layer, then capture Chrome/Safari URL-bar references and an explicit comparison artifact.

## Dogfood browser shell UX gaps

During the first real GitHub Gist human-in-the-loop dogfood run, the Saccade worker was usable but uncomfortable as a human-facing browser:

- Page content did not reflow/resize with the enlarged macOS window, leaving large blank areas and making the useful page region hard to read. Measurement confirmed runtime rendering geometry can resize while the page JS/layout viewport remains at the original startup size.
- The worker shell has no browser chrome: URL bar, Back, Forward, Reload, visible current URL, or page title/status controls.
- GitHub's user menu popover could remain open and cover page content; the shell needs better Esc/click-outside handling and visible focus state.

Status: partially mitigated. The worker now supports startup viewport sizing through `browser-session-worker --width --height` and defaults to `1600x1000`; dogfood/demo should start at the desired viewport, for example `1920x1080`. True runtime resize/reflow remains blocked on a Servo adapter/patch that updates page layout viewport details after window resize.
