//! Click verification from tracker events and score polls.

use saccade_core::{
    ClickOutcome, ClickReceipt, Ns, ScoreState, TargetId, TrackerEvent, VerificationResult,
};

const DEFAULT_DEADLINE_NS: Ns = 50_000_000;

#[derive(Debug, Clone)]
pub struct PendingClick {
    pub receipt: ClickReceipt,
    pub deadline_ns: Ns,
}

#[derive(Debug, Clone, Default)]
pub struct RunVerdict {
    pub hits: u32,
    pub misses: u32,
    pub unknown: u32,
    pub stale: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Verifier {
    pending: Vec<PendingClick>,
    last_score: Option<ScoreState>,
}

impl Verifier {
    pub fn add_click(&mut self, receipt: ClickReceipt) {
        self.pending.push(PendingClick {
            deadline_ns: receipt.t_up_sent_ns + DEFAULT_DEADLINE_NS,
            receipt,
        });
    }

    pub fn on_frame(
        &mut self,
        events: &[TrackerEvent],
        score: Option<&ScoreState>,
        now: Ns,
    ) -> Vec<VerificationResult> {
        let mut results = Vec::new();

        if let Some(score) = score {
            if let Some(last) = &self.last_score {
                if score.misses > last.misses {
                    results.extend(self.drain_pending_as(
                        ClickOutcome::Miss,
                        score.t_obs_ns,
                        "score miss counter increased",
                    ));
                } else if score.hits > last.hits {
                    results.extend(self.drain_pending_as(
                        ClickOutcome::Hit,
                        score.t_obs_ns,
                        "score hit counter increased",
                    ));
                }
            }
            self.last_score = Some(score.clone());
        }

        for event in events {
            if let TrackerEvent::Disappeared {
                target_id,
                t_obs_ns,
            } = event
            {
                if let Some(index) = self
                    .pending
                    .iter()
                    .position(|pending| pending.receipt.target_id == *target_id)
                {
                    let pending = self.pending.remove(index);
                    results.push(verification(
                        &pending.receipt,
                        ClickOutcome::Hit,
                        *t_obs_ns,
                        "target disappeared",
                    ));
                }
            }
        }

        let mut index = 0;
        while index < self.pending.len() {
            if now > self.pending[index].deadline_ns {
                let pending = self.pending.remove(index);
                results.push(verification(
                    &pending.receipt,
                    ClickOutcome::Unknown,
                    now,
                    "verification deadline expired",
                ));
            } else {
                index += 1;
            }
        }

        results
    }

    pub fn finalize(&mut self, final_score: ScoreState) -> RunVerdict {
        let unknown = self.pending.len() as u32;
        self.pending.clear();
        RunVerdict {
            hits: final_score.hits,
            misses: final_score.misses,
            unknown,
            stale: 0,
        }
    }

    fn drain_pending_as(
        &mut self,
        outcome: ClickOutcome,
        t_verified_ns: Ns,
        reason: &'static str,
    ) -> Vec<VerificationResult> {
        self.pending
            .drain(..)
            .map(|pending| verification(&pending.receipt, outcome, t_verified_ns, reason))
            .collect()
    }
}

fn verification(
    receipt: &ClickReceipt,
    outcome: ClickOutcome,
    t_verified_ns: Ns,
    reason: &'static str,
) -> VerificationResult {
    VerificationResult {
        click_id: receipt.click_id,
        target_id: receipt.target_id,
        outcome,
        t_verified_ns,
        reason: reason.into(),
    }
}

pub fn target_disappeared(target_id: TargetId, t_obs_ns: Ns) -> TrackerEvent {
    TrackerEvent::Disappeared {
        target_id,
        t_obs_ns,
    }
}
