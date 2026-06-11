# Saccade - agent instructions

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

