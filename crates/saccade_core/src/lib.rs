//! Core cross-crate types. Servo-facing code must translate into these units.

mod geometry;
mod metrics;
mod model;
mod time;

pub use geometry::{
    CoordinateMapper, CssPoint, CssPx, CssRect, DevicePoint, DevicePx, InputSpace, ViewportInfo,
};
pub use metrics::{BenchmarkResult, Histogram, LatencyPair, LatencySummary};
pub use model::{
    AccuracySummary, ClickOutcome, ClickReceipt, DetectorUsage, DifficultyConfig, DomRectObs,
    FrameObservation, GameFrameReport, InputBackendKind, MotorAction, PixelRegion, RenderedTarget,
    RunCounters, ScoreState, TargetCandidate, TargetEvidence, TargetId, TargetSource, TrackerEvent,
    VerificationResult,
};
pub use time::{Clock, Ns};
