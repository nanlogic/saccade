//! Servo-backed browser boundary. This is the only crate that may import `servo`.

mod arena_run;
mod browser_session;
mod browser_session_worker;
mod devmax_probe;
mod dogfood;
mod formmax_run;
mod native_input;
mod real_run;
mod recon;
mod rendering_profile;
mod selftest;
mod trusted_tabs;
mod user_flow;

mod calibration;
mod page_selftest;

pub use arena_run::{ArenaRunConfig, ArenaRunReport, run_arena};
pub use browser_session::{BrowserSessionProfile, selftest_browser_session};
pub use browser_session_worker::{
    BrowserSessionWorkerConfig, run_browser_session_worker, run_browser_session_worker_with_config,
};
pub use calibration::{CalibrationAttempt, CalibrationClick, CalibrationReport, calibrate_input};
pub use devmax_probe::{devmax_probe, devmax_probe_with_artifacts};
pub use dogfood::{DogfoodBrowserConfig, run_dogfood_browser};
pub use formmax_run::{
    FormmaxRunConfig, FormmaxRunReport, run_formmax_fixture, run_formmax_fixture_with_config,
};
pub use native_input::{
    NativeInputConfig, NativeInputProfile, selftest_native_input, selftest_native_input_with_config,
};
pub use page_selftest::{SelftestPageOutcome, SelftestPagesReport, selftest_pages};
pub use real_run::{RealRunConfig, RealRunReport, run_real};
pub use recon::{RealSiteRecon, real_site_recon};
pub use rendering_profile::{RenderingProfile, RenderingProfileSettings};
pub use selftest::selftest_boot;
pub use trusted_tabs::{
    TrustedTabsProfile, selftest_login_handoff, selftest_safety, selftest_trusted_tabs,
};
pub use user_flow::{UserFlowProfile, selftest_user_flow};
