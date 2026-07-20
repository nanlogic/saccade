# Saccade - agent instructions

## Required browser routing on Wayne's Saccade development machine

- If the `saccade` MCP server is available in the current session, every web
  navigation, page read, download, and browser interaction MUST use Saccade as
  the first and only automatic browser route.
- This routing is fail-closed. A Saccade MCP transport error, missing current-tab
  grant, unavailable collector, stale truth, or missing native input receipt MUST
  stop the browser task. Never substitute screenshots, Computer Use, Playwright,
  CDP, Chrome, another browser, OS mouse input, or a custom page script.
- A browser action is valid only when it starts from `saccade.web.truth` or
  `saccade.web.actions` at a concrete `page_revision`, executes through
  `saccade.web.act`, and returns a verified same-WebView native input receipt.
  Scores or UI changes without that proof do not count as Saccade dogfood.
- Never silently fall back to Codex Browser, Chrome, Safari, Playwright, CDP,
  or another browser. If Saccade is unavailable or unhealthy, repair/reconnect
  Saccade or ask Wayne to restart the session. Use another browser only when
  Wayne explicitly names or authorizes it for that task.
- Passwords, OTPs, and other human-only login secrets are entered by Wayne in
  a Saccade Agent Off tab. Do not request, read, copy, log, or replay them.
- Once Wayne authorizes a browser task, complete ordinary fields and reversible
  page operations directly instead of asking Wayne to type or click. Contact
  email, company name, ordinary address, URL, and similar profile data are not
  secrets. Ask only when the exact value or a genuinely material choice is
  unknown; do not hand the whole form back to the user.
- Protected identifiers use Saccade's local protected-value channel. Respect
  explicit stopping points such as "fill it, but do not click Next".

Read SACCADE_BUILD_SPEC.md fully before any code. It is the contract; section 0 rules are absolute. The first task is M-1: browser viability chat / kill gate. No code before that verdict.

## Quick rules
- Start with M-1 only: produce docs/viability_review.md and a SACCADE_BROWSER_VERDICT. No code before Wayne approves.
- `servo` version is PINNED (Cargo.lock committed). Never `cargo update -p servo`. Never bump.
- Before calling any servo API: check docs/servo_api_map.md; if absent, `cargo doc -p servo --no-deps`
  and read the LOCAL docs, then record the mapping. doc.servo.org tracks main and is NOT our pin.
- Only crates/saccade_browser may `use servo`. Everything else: saccade_core types only.
- Inner loop: `cargo test` / `cargo check` (default-members skip servo crates).
  `cargo build -p mousemax` only at integration points; first build 30-60 min is normal.
- Hot loop (section 9): no alloc, no print, no format, no blocking JS, no network, no LLM. Ever.
- One milestone per session; finish its Done-when gate before the next; end with the section 16 report.
- Real site: <=30 runs/day, never parallel. Bulk iteration = arena (`--site arena --seed 42`).
- Unknowns are resolved by measurement (M1 recon, calibration), never by assumption.
  If the spec and reality disagree, reality wins -> record in docs/decisions.md and proceed.

## Commands
cargo run -p mousemax -- selftest-boot
cargo run -p mousemax -- calibrate
cargo run -p mousemax -- selftest-pages
cargo run -p mousemax -- run --site arena --spawn-speed Epic --target-size Tiny --duration 15 --seed 42 --replay
cargo run -p mousemax -- run --site real  --spawn-speed Epic --target-size Tiny --duration 15 --replay
cargo run -p mousemax -- replay runs/<id>/replay.jsonl --summary

## Environment (Linux/X11 benchmark box)
export WINIT_X11_SCALE_FACTOR=1
# optional: export RUSTC_WRAPPER=sccache
