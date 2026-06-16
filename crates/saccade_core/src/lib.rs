//! Core cross-crate types. Servo-facing code must translate into these units.

mod geometry;
mod metrics;
mod model;
mod site_policy;
mod time;

pub use geometry::{
    CoordinateMapper, CssPoint, CssPx, CssRect, DevicePoint, DevicePx, InputSpace, ViewportInfo,
};
pub use metrics::{BenchmarkResult, Histogram, LatencyPair, LatencySummary};
pub use model::{
    AccuracySummary, ClickOutcome, ClickReceipt, DetectorUsage, DifficultyConfig, DomRectObs,
    FrameObservation, GameFrameReport, InputBackendKind, MotorAction, PixelRegion, ReadGrant,
    RenderedTarget, RunCounters, ScoreState, TabId, TabInfo, TabOwner, TabVisualMarker,
    TargetCandidate, TargetEvidence, TargetId, TargetSource, TrackerEvent, VerificationResult,
};
pub use site_policy::{SitePolicy, SiteRiskLevel, classify_site_url, site_action_requires_user};
pub use time::{Clock, Ns};
