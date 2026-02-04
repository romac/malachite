use std::collections::HashMap;
use std::time::Duration;

use malachitebft_peer::PeerId;

use super::{Score, ScoringStrategy, SyncResult};

/// Credit-based scoring strategy
///
/// Maintain an integer "credit" per peer.
/// - Fast success increases credit more than slow success.
/// - Failures and timeouts reduce credit.
///
/// Credits are clamped to [min_credit, max_credit].
/// Score is a normalized mapping of credit -> [0.0, 1.0].
#[derive(Clone, Debug)]
pub struct Credit {
    /// Threshold for what is considered "fast enough".
    pub slow_threshold: Duration,

    /// Credit deltas
    pub credit_fast_success: i32,
    pub credit_slow_success: i32,
    pub credit_failure: i32,
    pub credit_timeout: i32,

    /// Clamp bounds
    pub min_credit: i32,
    pub max_credit: i32,

    /// Per-peer credits
    credits: HashMap<PeerId, i32>,
}

impl Default for Credit {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(1), // slow_threshold
            2,                      // fast success
            1,                      // slow success
            -2,                     // failure
            -3,                     // timeout
            -20,                    // min_credit
            20,                     // max_credit
        )
    }
}

impl Credit {
    pub fn new(
        slow_threshold: Duration,
        credit_fast_success: i32,
        credit_slow_success: i32,
        credit_failure: i32,
        credit_timeout: i32,
        min_credit: i32,
        max_credit: i32,
    ) -> Self {
        assert!(
            slow_threshold.as_secs_f64() > 0.0,
            "slow_threshold must be > 0"
        );

        assert!(min_credit < max_credit, "min_credit must be < max_credit");

        Self {
            slow_threshold,
            credit_fast_success,
            credit_slow_success,
            credit_failure,
            credit_timeout,
            min_credit,
            max_credit,
            credits: HashMap::new(),
        }
    }

    fn clamp_credit(&self, c: i32) -> i32 {
        c.clamp(self.min_credit, self.max_credit)
    }

    /// Map credit in [min_credit, max_credit] to score in [0.0, 1.0].
    fn credit_to_score(&self, credit: i32) -> Score {
        let min = self.min_credit as f64;
        let max = self.max_credit as f64;
        let c = credit as f64;

        // Avoid division by zero if min and max are the same
        // (though this should be prevented by the constructor).
        if (max - min).abs() < f64::EPSILON {
            return 0.5;
        }

        ((c - min) / (max - min)).clamp(0.0, 1.0)
    }

    fn initial_credit(&self) -> i32 {
        // Neutral: midpoint of the clamp range.
        self.min_credit + (self.max_credit - self.min_credit) / 2
    }

    fn get_credit(&mut self, peer_id: PeerId) -> i32 {
        let init = self.initial_credit();
        *self.credits.entry(peer_id).or_insert(init)
    }

    fn set_credit(&mut self, peer_id: PeerId, credit: i32) -> i32 {
        let new_credit = self.clamp_credit(credit);
        self.credits.insert(peer_id, new_credit);
        new_credit
    }

    fn is_fast(&self, response_time: Duration) -> bool {
        response_time < self.slow_threshold
    }
}

impl ScoringStrategy for Credit {
    fn initial_score(&self, _peer_id: PeerId) -> Score {
        // Note: this returns a neutral score, but actual per-peer credit is created on first update.
        self.credit_to_score(self.initial_credit())
    }

    fn update_score(
        &mut self,
        peer_id: PeerId,
        _previous_score: Score,
        result: SyncResult,
    ) -> Score {
        let credit = self.get_credit(peer_id);

        let delta = match result {
            SyncResult::Success(rt) => {
                if self.is_fast(rt) {
                    self.credit_fast_success
                } else {
                    self.credit_slow_success
                }
            }
            SyncResult::Failure => self.credit_failure,
            SyncResult::Timeout => self.credit_timeout,
        };

        let new_credit = self.set_credit(peer_id, credit.saturating_add(delta));

        eprintln!(
            "result={result:?}, credit={credit}, delta={delta}, new={}, score={:.2}",
            new_credit,
            self.credit_to_score(new_credit)
        );

        self.credit_to_score(new_credit)
    }
}
