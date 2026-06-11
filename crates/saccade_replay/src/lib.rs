//! JSONL replay log writer/reader.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result, bail};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use saccade_core::{
    BenchmarkResult, ClickReceipt, GameFrameReport, InputSpace, ScoreState, TrackerEvent,
    VerificationResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const CHANNEL_CAPACITY: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReplayEvent {
    RunStarted {
        run_id: String,
        wall_clock_unix_ms: u64,
        config: Value,
        input_space: InputSpace,
    },
    FrameReport {
        report: GameFrameReport,
    },
    ClickDispatched {
        receipt: ClickReceipt,
    },
    TrackerEvent {
        event: TrackerEvent,
    },
    ScorePoll {
        score: ScoreState,
    },
    ClickVerified {
        result: VerificationResult,
    },
    RunFinished {
        result: BenchmarkResult,
    },
}

impl ReplayEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::FrameReport { .. } => "frame_report",
            Self::ClickDispatched { .. } => "click_dispatched",
            Self::TrackerEvent { .. } => "tracker_event",
            Self::ScorePoll { .. } => "score_poll",
            Self::ClickVerified { .. } => "click_verified",
            Self::RunFinished { .. } => "run_finished",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayDropCounts {
    pub frame_reports: u64,
    pub other_events: u64,
}

pub struct ReplayJsonlWriter<W: Write> {
    writer: W,
}

impl<W: Write> ReplayJsonlWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write_event(&mut self, event: &ReplayEvent) -> Result<()> {
        serde_json::to_writer(&mut self.writer, event)
            .context("failed to serialize replay event")?;
        self.writer
            .write_all(b"\n")
            .context("failed to terminate replay event line")
    }

    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush().context("failed to flush replay writer")
    }
}

pub fn read_events(path: impl AsRef<Path>) -> Result<Vec<ReplayEvent>> {
    let path = path.as_ref();
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("failed to read line {} from {}", index + 1, path.display())
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str(&line).with_context(|| {
            format!(
                "failed to parse replay event at {}:{}",
                path.display(),
                index + 1
            )
        })?;
        events.push(event);
    }

    Ok(events)
}

pub struct ReplayLogger {
    tx: Sender<ReplayEvent>,
    done_rx: Receiver<Result<ReplayDropCounts, String>>,
    handle: Option<JoinHandle<()>>,
    drops: ReplayDropCounts,
}

impl ReplayLogger {
    pub fn spawn(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let (tx, rx) = bounded(CHANNEL_CAPACITY);
        let (done_tx, done_rx) = bounded(1);
        let handle = thread::spawn(move || {
            let result = writer_thread(path, rx);
            let _ = done_tx.send(result);
        });

        Self {
            tx,
            done_rx,
            handle: Some(handle),
            drops: ReplayDropCounts::default(),
        }
    }

    /// Never blocks the caller. Frame reports are drop-preferred when the channel is full.
    pub fn try_log(&mut self, event: ReplayEvent) {
        match self.tx.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(ReplayEvent::FrameReport { .. })) => {
                self.drops.frame_reports += 1;
            }
            Err(TrySendError::Full(_)) => {
                self.drops.other_events += 1;
            }
            Err(TrySendError::Disconnected(_)) => {
                self.drops.other_events += 1;
            }
        }
    }

    pub fn finish(mut self) -> Result<ReplayDropCounts> {
        drop(self.tx);
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("replay writer thread panicked"))?;
        }

        let thread_drops = match self.done_rx.try_recv() {
            Ok(Ok(drops)) => drops,
            Ok(Err(error)) => bail!(error),
            Err(_) => ReplayDropCounts::default(),
        };

        Ok(ReplayDropCounts {
            frame_reports: self.drops.frame_reports + thread_drops.frame_reports,
            other_events: self.drops.other_events + thread_drops.other_events,
        })
    }
}

fn writer_thread(
    path: PathBuf,
    rx: Receiver<ReplayEvent>,
) -> std::result::Result<ReplayDropCounts, String> {
    let file = File::create(&path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    let mut writer = ReplayJsonlWriter::new(BufWriter::new(file));

    for event in rx {
        writer
            .write_event(&event)
            .map_err(|error| format!("failed to write replay event: {error}"))?;
    }
    writer
        .flush()
        .map_err(|error| format!("failed to flush replay log: {error}"))?;

    Ok(ReplayDropCounts::default())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaySummary {
    pub run_started: u32,
    pub frame_reports: u32,
    pub click_dispatched: u32,
    pub tracker_events: u32,
    pub score_polls: u32,
    pub click_verified: u32,
    pub run_finished: u32,
}

impl ReplaySummary {
    pub fn from_events(events: &[ReplayEvent]) -> Self {
        let mut summary = Self::default();
        for event in events {
            match event {
                ReplayEvent::RunStarted { .. } => summary.run_started += 1,
                ReplayEvent::FrameReport { .. } => summary.frame_reports += 1,
                ReplayEvent::ClickDispatched { .. } => summary.click_dispatched += 1,
                ReplayEvent::TrackerEvent { .. } => summary.tracker_events += 1,
                ReplayEvent::ScorePoll { .. } => summary.score_polls += 1,
                ReplayEvent::ClickVerified { .. } => summary.click_verified += 1,
                ReplayEvent::RunFinished { .. } => summary.run_finished += 1,
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use saccade_core::{
        AccuracySummary, ClickOutcome, CssPoint, CssRect, DetectorUsage, DifficultyConfig,
        InputBackendKind, LatencyPair, LatencySummary, PixelRegion, RenderedTarget, RunCounters,
        TargetId, TargetSource, VerificationResult, ViewportInfo,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn replay_event_json_round_trip_preserves_kind() {
        let event = ReplayEvent::RunStarted {
            run_id: "run_test".into(),
            wall_clock_unix_ms: 42,
            config: json!({"site": "arena"}),
            input_space: InputSpace::CssLogical,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"kind\":\"run_started\""));
        let decoded: ReplayEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.kind(), "run_started");
    }

    #[test]
    fn jsonl_writer_and_reader_round_trip_events() {
        let mut bytes = Vec::new();
        {
            let mut writer = ReplayJsonlWriter::new(&mut bytes);
            writer.write_event(&sample_run_started()).unwrap();
            writer.write_event(&sample_frame_report()).unwrap();
            writer.write_event(&sample_click_dispatched()).unwrap();
            writer.write_event(&sample_click_verified()).unwrap();
            writer.write_event(&sample_run_finished()).unwrap();
            writer.flush().unwrap();
        }

        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.lines().count(), 5);

        for line in text.lines() {
            let event: ReplayEvent = serde_json::from_str(line).unwrap();
            assert!(!event.kind().is_empty());
        }
    }

    #[test]
    fn logger_writes_file_and_summary_counts_events() {
        let path = temp_replay_path("logger_writes_file_and_summary_counts_events");
        let mut logger = ReplayLogger::spawn(path.clone());
        logger.try_log(sample_run_started());
        logger.try_log(sample_frame_report());
        logger.try_log(sample_click_dispatched());
        logger.try_log(sample_run_finished());
        let drops = logger.finish().unwrap();

        assert_eq!(drops, ReplayDropCounts::default());
        let events = read_events(&path).unwrap();
        let summary = ReplaySummary::from_events(&events);
        assert_eq!(summary.run_started, 1);
        assert_eq!(summary.frame_reports, 1);
        assert_eq!(summary.click_dispatched, 1);
        assert_eq!(summary.run_finished, 1);

        let _ = fs::remove_file(path);
    }

    fn sample_run_started() -> ReplayEvent {
        ReplayEvent::RunStarted {
            run_id: "run_test".into(),
            wall_clock_unix_ms: 42,
            config: json!({"site": "arena", "seed": 42}),
            input_space: InputSpace::CssLogical,
        }
    }

    fn sample_frame_report() -> ReplayEvent {
        ReplayEvent::FrameReport {
            report: GameFrameReport {
                frame_id: 7,
                t_report_ns: 20_000_000,
                game_area_css: CssRect {
                    x: 0.0,
                    y: 0.0,
                    w: 1280.0,
                    h: 600.0,
                },
                targets: vec![RenderedTarget {
                    id: TargetId(1),
                    frame_id: 7,
                    first_seen_ns: 10_000_000,
                    last_seen_ns: 20_000_000,
                    center_css: CssPoint { x: 42.0, y: 84.0 },
                    bbox_css: CssRect {
                        x: 35.0,
                        y: 77.0,
                        w: 14.0,
                        h: 14.0,
                    },
                    radius_css: 7.0,
                    confidence: 0.95,
                    source: TargetSource::DomRect,
                    clicked: false,
                }],
                detector_ms: 0.4,
            },
        }
    }

    fn sample_click_dispatched() -> ReplayEvent {
        ReplayEvent::ClickDispatched {
            receipt: ClickReceipt {
                click_id: 1,
                target_id: TargetId(1),
                point_css: CssPoint { x: 42.0, y: 84.0 },
                frame_id: 7,
                t_target_first_seen_ns: 10_000_000,
                t_decided_ns: 20_200_000,
                t_move_sent_ns: 20_300_000,
                t_down_sent_ns: 20_350_000,
                t_up_sent_ns: 20_400_000,
                backend: InputBackendKind::ServoInternal,
            },
        }
    }

    fn sample_click_verified() -> ReplayEvent {
        ReplayEvent::ClickVerified {
            result: VerificationResult {
                click_id: 1,
                target_id: TargetId(1),
                outcome: ClickOutcome::Hit,
                t_verified_ns: 36_000_000,
                reason: "target disappeared".into(),
            },
        }
    }

    fn sample_run_finished() -> ReplayEvent {
        ReplayEvent::RunFinished {
            result: BenchmarkResult {
                run_id: "run_test".into(),
                site: "arena".into(),
                url: "http://127.0.0.1/arena".into(),
                difficulty: DifficultyConfig {
                    spawn_speed: "Epic".into(),
                    target_size: "Tiny".into(),
                },
                duration_s: 15,
                verdict: "PASS".into(),
                result: RunCounters {
                    hits: 1,
                    misses: 0,
                    targets_seen: 1,
                    clicks_sent: 1,
                    unknown_verifications: 0,
                    false_positive_clicks: 0,
                    stale_clicks: 0,
                    expired_unclicked: 0,
                },
                latency_ms: LatencySummary {
                    detect_to_dispatch: LatencyPair { p50: 0.3, p95: 0.3 },
                    first_visible_to_dispatch: LatencyPair {
                        p50: 10.3,
                        p95: 10.3,
                    },
                    capture: LatencyPair { p50: 0.5, p95: 0.5 },
                    detect: LatencyPair { p50: 0.4, p95: 0.4 },
                },
                accuracy: AccuracySummary {
                    median_click_error_css_px: 0.0,
                    max_click_error_css_px: 0.0,
                },
                detectors_used: DetectorUsage {
                    pixel_detector: 0,
                    dom_rect: 1,
                    canvas_observe: 0,
                    fused: 0,
                },
                instrumentation: "observe_only".into(),
                input_space: InputSpace::CssLogical,
                llm_frame_calls: 0,
                calibration_max_err_css_px: 0.0,
                replay_file: "runs/run_test/replay.jsonl".into(),
            },
        }
    }

    #[allow(dead_code)]
    fn sample_pixel_region() -> PixelRegion {
        PixelRegion {
            w: 1,
            h: 1,
            rgba: Arc::new(vec![0, 0, 0, 255]),
        }
    }

    #[allow(dead_code)]
    fn sample_viewport() -> ViewportInfo {
        ViewportInfo {
            width_css: 1280.0,
            height_css: 800.0,
            device_scale_factor: 1.0,
            page_zoom: 1.0,
        }
    }

    fn temp_replay_path(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("saccade_{test_name}_{nanos}.jsonl"))
    }
}
