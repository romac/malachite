//! For tallying all the votes for a single round

use derive_where::derive_where;

use malachitebft_core_types::{NilOrVal, Vote, VoteType};

use crate::count::VoteCount;
use crate::{Threshold, ThresholdParam, Weight};

/// Tracks all the votes for a single round
#[derive_where(Clone, Debug, PartialEq, Eq)]
pub struct RoundVotes<V: Vote> {
    /// The prevotes for this round.
    prevotes: VoteCount<V::Address, V::ValueId>,
    /// The precommits for this round.
    precommits: VoteCount<V::Address, V::ValueId>,
}

impl<V: Vote> RoundVotes<V> {
    /// Create a new `RoundVotes` instance.
    pub fn new() -> Self {
        RoundVotes {
            prevotes: VoteCount::new(),
            precommits: VoteCount::new(),
        }
    }

    /// Return the prevotes for this round.
    pub fn prevotes(&self) -> &VoteCount<V::Address, V::ValueId> {
        &self.prevotes
    }

    /// Return the precommits for this round.
    pub fn precommits(&self) -> &VoteCount<V::Address, V::ValueId> {
        &self.precommits
    }

    /// Add a vote to the round, of the given type, from the given address,
    /// with the given value and weight.
    pub fn add_vote(&mut self, vote: &V, weight: Weight) -> Weight {
        match vote.vote_type() {
            VoteType::Prevote => self
                .prevotes
                .add(vote.validator_address(), vote.value(), weight),
            VoteType::Precommit => {
                self.precommits
                    .add(vote.validator_address(), vote.value(), weight)
            }
        }
    }

    /// Get the weight of the vote of the given type for the given value.
    ///
    /// If there is no vote for that value, return 0.
    pub fn get_weight(&self, vote_type: VoteType, value: &NilOrVal<V::ValueId>) -> Weight {
        match vote_type {
            VoteType::Prevote => self.prevotes.get(value),
            VoteType::Precommit => self.precommits.get(value),
        }
    }

    /// Get the sum of the weights of the votes of the given type.
    pub fn weight_sum(&self, vote_type: VoteType) -> Weight {
        match vote_type {
            VoteType::Prevote => self.prevotes.sum(),
            VoteType::Precommit => self.precommits.sum(),
        }
    }

    /// Get the sum of the weights of all votes, regardless of type, for the given value.
    pub fn combined_weight(&self, value: &NilOrVal<V::ValueId>) -> Weight {
        self.prevotes.get(value) + self.precommits.get(value)
    }

    /// Return whether or not the threshold is met, ie. if we have a quorum for that threshold.
    pub fn is_threshold_met(
        &self,
        vote_type: VoteType,
        threshold: Threshold<V::ValueId>,
        param: ThresholdParam,
        total_weight: Weight,
    ) -> bool {
        match vote_type {
            VoteType::Prevote => self
                .prevotes
                .is_threshold_met(threshold, param, total_weight),

            VoteType::Precommit => self
                .precommits
                .is_threshold_met(threshold, param, total_weight),
        }
    }
}

impl<V: Vote> Default for RoundVotes<V> {
    fn default() -> Self {
        Self::new()
    }
}
