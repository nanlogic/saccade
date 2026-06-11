use serde::{Deserialize, Serialize};

use crate::{
    AccuracySummary, DetectorUsage, DifficultyConfig, InputSpace, Ns, RunCounters, TargetSource,
};

const BUCKET_UPPER_NS: [Ns; 29] = [
    100_000,
    125_000,
    160_000,
    200_000,
    250_000,
    315_000,
    400_000,
    500_000,
    630_000,
    800_000,
    1_000_000,
    1_250_000,
    1_600_000,
    2_000_000,
    2_500_000,
    3_150_000,
    4_000_000,
    5_000_000,
    6_300_000,
    8_000_000,
    10_000_000,
    16_000_000,
    25_000_000,
    40_000_000,
    63_000_000,
    100_000_000,
    250_000_000,
    500_000_000,
    1_000_000_000,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    buckets: Vec<u64>,
    overflow: u64,
    count: u64,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            buckets: vec![0; BUCKET_UPPER_NS.len()],
            overflow: 0,
            count: 0,
        }
    }

    pub fn record_ns(&mut self, ns: Ns) {
        self.count += 1;
        match BUCKET_UPPER_NS.iter().position(|upper| ns <= *upper) {
            Some(index) => self.buckets[index] += 1,
            None => self.overflow += 1,
        }
    }

    pub fn p50_ms(&self) -> f32 {
        self.percentile_ms(0.50)
    }

    pub fn p95_ms(&self) -> f32 {
        self.percentile_ms(0.95)
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    fn percentile_ms(&self, percentile: f32) -> f32 {
        if self.count == 0 {
            return 0.0;
        }

        let rank = ((self.count as f32 * percentile).ceil() as u64).max(1);
        let mut seen = 0;
        for (index, count) in self.buckets.iter().enumerate() {
            seen += count;
            if seen >= rank {
                return BUCKET_UPPER_NS[index] as f32 / 1_000_000.0;
            }
        }

        1_000.0
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LatencyPair {
    pub p50: f32,
    pub p95: f32,
}

impl From<&Histogram> for LatencyPair {
    fn from(histogram: &Histogram) -> Self {
        Self {
            p50: histogram.p50_ms(),
            p95: histogram.p95_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySummary {
    pub detect_to_dispatch: LatencyPair,
    pub first_visible_to_dispatch: LatencyPair,
    pub capture: LatencyPair,
    pub detect: LatencyPair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub run_id: String,
    pub site: String,
    pub url: String,
    pub difficulty: DifficultyConfig,
    pub duration_s: u32,
    pub verdict: String,
    pub result: RunCounters,
    pub latency_ms: LatencySummary,
    pub accuracy: AccuracySummary,
    pub detectors_used: DetectorUsage,
    pub instrumentation: String,
    pub input_space: InputSpace,
    pub llm_frame_calls: u32,
    pub calibration_max_err_css_px: f32,
    pub replay_file: String,
}

impl DetectorUsage {
    pub fn record(&mut self, source: TargetSource) {
        match source {
            TargetSource::PixelDetector => self.pixel_detector += 1,
            TargetSource::DomRect => self.dom_rect += 1,
            TargetSource::CanvasObserve => self.canvas_observe += 1,
            TargetSource::Fused => self.fused += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_reports_fixed_bucket_percentiles() {
        let mut histogram = Histogram::new();
        histogram.record_ns(100_000);
        histogram.record_ns(1_100_000);
        histogram.record_ns(4_900_000);
        histogram.record_ns(900_000_000);

        assert_eq!(histogram.count(), 4);
        assert_eq!(histogram.p50_ms(), 1.25);
        assert_eq!(histogram.p95_ms(), 1000.0);
    }
}
