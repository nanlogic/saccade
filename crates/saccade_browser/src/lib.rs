//! Servo-backed browser boundary. This is the only crate that may import `servo`.

mod arena_run;
mod devmax_probe;
mod formmax_run;
mod real_run;
mod recon;
mod selftest;
mod trusted_tabs;

mod calibration;
mod page_selftest;

pub use arena_run::{ArenaRunConfig, ArenaRunReport, run_arena};
pub use calibration::{CalibrationAttempt, CalibrationClick, CalibrationReport, calibrate_input};
pub use devmax_probe::devmax_probe;
pub use formmax_run::{FormmaxRunReport, run_formmax_fixture};
pub use page_selftest::{SelftestPageOutcome, SelftestPagesReport, selftest_pages};
pub use real_run::{RealRunConfig, RealRunReport, run_real};
pub use recon::{RealSiteRecon, real_site_recon};
pub use selftest::selftest_boot;
pub use trusted_tabs::{
    TrustedTabsProfile, selftest_login_handoff, selftest_safety, selftest_trusted_tabs,
};
