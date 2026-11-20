//! For tallying votes of the same type.

use alloc::collections::BTreeSet;

use malachitebft_core_types::NilOrVal;

use crate::value_weights::ValuesWeights;
use crate::{Threshold, ThresholdParam, Weight};

/// VoteCount tallys votes of the same type.
///
/// Votes are for nil or for some value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoteCount<Address, Id> {
    /// Weight of votes for the values, including nil
    pub values_weights: ValuesWeights<NilOrVal<Id>>,

    /// Addresses of validators who voted for the values
    pub validator_addresses: BTreeSet<Address>,
}

impl<Address: Ord + Clone, Id: Ord + Clone> VoteCount<Address, Id> {
    /// Create a new `VoteCount`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add vote for a value (or nil) to internal counters, but only if we haven't seen
    /// a vote from that particular validator yet.
    pub fn add(&mut self, address: &Address, value: &NilOrVal<Id>, weight: Weight) -> Weight {
        if self.validator_addresses.contains(address) {
            // Validator has already voted, ignore this vote
            self.values_weights.get(value)
        } else {
            self.validator_addresses.insert(address.clone());
            self.values_weights.add(value.clone(), weight)
        }
    }

    /// Return the weight of votes for the given value (or nil).
    pub fn get(&self, value: &NilOrVal<Id>) -> Weight {
        self.values_weights.get(value)
    }

    /// Return the sum of weights of votes for all values.
    pub fn sum(&self) -> Weight {
        self.values_weights.sum()
    }

    /// Return whether or not the threshold is met, ie. if we have a quorum for that threshold.
    pub fn is_threshold_met(
        &self,
        threshold: Threshold<Id>,
        param: ThresholdParam,
        total_weight: Weight,
    ) -> bool {
        match threshold {
            Threshold::Value(value) => {
                let weight = self.values_weights.get(&NilOrVal::Val(value));
                param.is_met(weight, total_weight)
            }

            Threshold::Nil => {
                let weight = self.values_weights.get(&NilOrVal::Nil);
                param.is_met(weight, total_weight)
            }

            Threshold::Any => {
                let sum_weight = self.values_weights.sum();
                param.is_met(sum_weight, total_weight)
            }

            Threshold::Unreached => false,
        }
    }
}

impl<Address, Id> Default for VoteCount<Address, Id> {
    fn default() -> Self {
        Self {
            values_weights: ValuesWeights::new(),
            validator_addresses: BTreeSet::new(),
        }
    }
}
