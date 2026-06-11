//! Motor preflight and one-click-per-frame decision logic.

use saccade_core::{GameFrameReport, MotorAction, Ns, TargetId, TargetSource};

const NS_PER_MS: Ns = 1_000_000;

#[derive(Debug, Clone)]
pub struct MotorConfig {
    pub min_confidence: f32,
    pub stale_frame_max_ms: f32,
    pub min_target_age_ms: f32,
    pub max_target_age_frac: f32,
    pub min_inter_click_ms: f32,
    pub conservative_after_miss_ms: f32,
    pub lifetime_estimate_ms: f32,
}

impl Default for MotorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.70,
            stale_frame_max_ms: 20.0,
            min_target_age_ms: 0.0,
            max_target_age_frac: 0.6,
            min_inter_click_ms: 8.0,
            conservative_after_miss_ms: 250.0,
            lifetime_estimate_ms: 1500.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MotorController {
    cfg: MotorConfig,
    conservative_until_ns: Ns,
    last_click_ns: Option<Ns>,
    clicked_targets: Vec<TargetId>,
}

impl MotorController {
    pub fn new(cfg: MotorConfig) -> Self {
        Self {
            cfg,
            conservative_until_ns: 0,
            last_click_ns: None,
            clicked_targets: Vec::new(),
        }
    }

    pub fn on_frame(&mut self, report: &GameFrameReport, now: Ns) -> MotorAction {
        if now.saturating_sub(report.t_report_ns) > ms_to_ns(self.cfg.stale_frame_max_ms) {
            return noop("stale frame");
        }
        if self
            .last_click_ns
            .is_some_and(|last| now.saturating_sub(last) < ms_to_ns(self.cfg.min_inter_click_ms))
        {
            return noop("inter-click floor");
        }

        let conservative = now < self.conservative_until_ns;
        let min_confidence = if conservative {
            self.cfg.min_confidence.max(0.90)
        } else {
            self.cfg.min_confidence
        };
        let lifetime_ns = ms_to_ns(self.cfg.lifetime_estimate_ms);
        let min_age_ns = ms_to_ns(self.cfg.min_target_age_ms);
        let max_age_ns = (lifetime_ns as f32 * self.cfg.max_target_age_frac) as Ns;

        let target = report
            .targets
            .iter()
            .filter(|target| !target.clicked)
            .filter(|target| !self.clicked_targets.contains(&target.id))
            .filter(|target| target.confidence >= min_confidence)
            .filter(|target| {
                target.last_seen_ns == target.first_seen_ns || target.frame_id == report.frame_id
            })
            .filter(|target| target.bbox_css.contains(target.center_css))
            .filter(|target| report.game_area_css.contains(target.center_css))
            .filter(|target| {
                let age = now.saturating_sub(target.first_seen_ns);
                age >= min_age_ns && age <= max_age_ns
            })
            .filter(|target| !conservative || target.source != TargetSource::PixelDetector)
            .min_by_key(|target| target.first_seen_ns);

        let Some(target) = target else {
            return noop("no eligible target");
        };

        self.last_click_ns = Some(now);
        self.clicked_targets.push(target.id);
        MotorAction::Click {
            target_id: target.id,
            point_css: target.center_css,
            frame_id: report.frame_id,
        }
    }

    pub fn note_miss_confirmed(&mut self, now: Ns) {
        self.conservative_until_ns = now + ms_to_ns(self.cfg.conservative_after_miss_ms);
    }

    pub fn is_conservative(&self, now: Ns) -> bool {
        now < self.conservative_until_ns
    }
}

impl Default for MotorController {
    fn default() -> Self {
        Self::new(MotorConfig::default())
    }
}

fn noop(reason: &'static str) -> MotorAction {
    MotorAction::Noop {
        reason: reason.into(),
    }
}

fn ms_to_ns(ms: f32) -> Ns {
    (ms * NS_PER_MS as f32) as Ns
}
