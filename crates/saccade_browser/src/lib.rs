//! Servo-backed browser boundary. This is the only crate that may import `servo`.

mod arena_run;
mod recon;
mod selftest;

mod calibration;
mod page_selftest;

pub use arena_run::{ArenaRunConfig, ArenaRunReport, run_arena};
pub use calibration::{CalibrationAttempt, CalibrationClick, CalibrationReport, calibrate_input};
pub use page_selftest::{SelftestPageOutcome, SelftestPagesReport, selftest_pages};
pub use recon::{RealSiteRecon, real_site_recon};
pub use selftest::selftest_boot;
