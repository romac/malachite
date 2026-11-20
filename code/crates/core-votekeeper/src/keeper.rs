//! For tallying votes and emitting messages when certain thresholds are reached.

use derive_where::derive_where;
use thiserror::Error;
use tracing::warn;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use malachitebft_core_types::{
    NilOrVal, Round, SignedMessage, Validator, ValidatorSet, Vote, VoteType,
};

use crate::evidence::{EvidenceMap, VotePair};
use crate::round_votes::RoundVotes;
use crate::round_weights::RoundWeights;
use crate::{Threshold, ThresholdParams, Weight};

/// Messages emitted by the vote keeper
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Output<Value> {
    /// We have a quorum of prevotes for some value or nil
    PolkaAny,

    /// We have a quorum of prevotes for nil
    PolkaNil,

    /// We have a quorum of prevotes for specific value
    PolkaValue(Value),

    /// We have a quorum of precommits for some value or nil
    PrecommitAny,

    /// We have a quorum of precommits for a specific value
    PrecommitValue(Value),

    /// We have f+1 honest votes for a value at a higher round
    SkipRound(Round),
}

/// Number of votes a validator casts per round: one prevote and one precommit.
const VOTES_PER_VALIDATOR_PER_ROUND: usize = 2;

/// Keeps track of votes and emitted outputs for a given round.
#[derive_where(Clone, Debug, PartialEq, Eq, Default)]
pub struct PerRound<V>
where
    V: Vote,
{
    /// The votes for this round.
    votes: RoundVotes<V>,

    /// The addresses and their weights for this round.
    addresses_weights: RoundWeights<V::Address>,

    /// All the votes received for this round.
    received_votes: Vec<SignedMessage<V, V::Signature>>,

    /// The emitted outputs for this round.
    emitted_outputs: BTreeSet<Output<V::ValueId>>,
}

/// Errors can that be yielded when recording a vote.
#[derive(Error)]
pub enum RecordVoteError<V>
where
    V: Vote,
{
    /// Attempted to record a conflicting vote.
    #[error("Conflicting vote: {existing} vs {conflicting}")]
    ConflictingVote {
        /// The vote already recorded.
        existing: SignedMessage<V, V::Signature>,
        /// The conflicting vote.
        conflicting: SignedMessage<V, V::Signature>,
    },
}

impl<V> PerRound<V>
where
    V: Vote,
{
    /// Create a new `PerRound` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `PerRound` instance with pre-allocated capacity for the expected number of votes.
    pub fn with_expected_number_of_votes(num_votes: usize) -> Self {
        Self {
            // pre-allocate capacity to avoid re-allocations during the addition of votes
            received_votes: Vec::with_capacity(num_votes),
            ..Self::default()
        }
    }

    /// Add a vote to the round, checking for conflicts.
    pub fn add(
        &mut self,
        vote: SignedMessage<V, V::Signature>,
        weight: Weight,
    ) -> Result<(), RecordVoteError<V>> {
        let existing_idx = self.received_votes.iter().position(|v| {
            v.vote_type() == vote.vote_type() && v.validator_address() == vote.validator_address()
        });

        if let Some(idx) = existing_idx {
            let existing = &self.received_votes[idx];
            if existing.value() != vote.value() {
                // This is an equivocating vote
                return Err(RecordVoteError::ConflictingVote {
                    existing: existing.clone(),
                    conflicting: vote,
                });
            }
            // Upgrade the stored vote if the incoming one carries an extension
            // while the stored vote lacks one. This can happen if the stored
            // vote came from a precommit round certificate whose votes do not
            // carry extensions, while individual votes are received after and
            // include extensions.
            if existing.extension().is_none() && vote.extension().is_some() {
                self.received_votes[idx] = vote;
            }
            return Ok(());
        }

        // Tally this vote
        self.votes.add_vote(&vote, weight);

        // Update the weight of the validator
        self.addresses_weights
            .set_once(vote.validator_address(), weight);

        // Add the vote to the received votes
        self.received_votes.push(vote);

        Ok(())
    }

    /// Return the vote of the given type received from the given validator.
    pub fn get_vote<'a>(
        &'a self,
        vote_type: VoteType,
        address: &'a V::Address,
    ) -> Option<&'a SignedMessage<V, V::Signature>> {
        self.received_votes
            .iter()
            .find(move |vote| vote.vote_type() == vote_type && vote.validator_address() == address)
    }

    /// Return the votes for this round.
    pub fn votes(&self) -> &RoundVotes<V> {
        &self.votes
    }

    /// Return the votes for this round.
    pub fn received_votes(&self) -> &Vec<SignedMessage<V, V::Signature>> {
        &self.received_votes
    }

    /// Return precommits for the given value in this round.
    pub fn precommits_for_value(
        &self,
        value_id: &V::ValueId,
    ) -> Vec<SignedMessage<V, V::Signature>> {
        self.received_votes
            .iter()
            .filter(|v| {
                v.vote_type() == VoteType::Precommit
                    && v.value() == &NilOrVal::Val(value_id.clone())
            })
            .cloned()
            .collect()
    }

    /// Return the addresses and their weights for this round.
    pub fn addresses_weights(&self) -> &RoundWeights<V::Address> {
        &self.addresses_weights
    }

    /// Return the emitted outputs for this round.
    pub fn emitted_outputs(&self) -> &BTreeSet<Output<V::ValueId>> {
        &self.emitted_outputs
    }
}

/// Keeps track of votes and emits messages when thresholds are reached.
#[derive_where(Clone, Debug)]
pub struct VoteKeeper<V, Set>
where
    V: Vote,
    Set: ValidatorSet<Validator: Validator<Address = V::Address>>,
{
    /// The validator set for this height.
    validator_set: Set,

    /// The threshold parameters.
    threshold_params: ThresholdParams,

    /// The votes and emitted outputs for each round.
    per_round: BTreeMap<Round, PerRound<V>>,

    /// Evidence of equivocation.
    evidence: EvidenceMap<V>,
}

impl<V, Set> VoteKeeper<V, Set>
where
    V: Vote,
    Set: ValidatorSet<Validator: Validator<Address = V::Address>>,
{
    /// Create a new `VoteKeeper` instance, for the given
    /// total network weight (ie. voting power) and threshold parameters.
    pub fn new(validator_set: Set, threshold_params: ThresholdParams) -> Self {
        Self {
            validator_set,
            threshold_params,
            per_round: BTreeMap::new(),
            evidence: EvidenceMap::new(),
        }
    }

    /// Return the current validator set
    pub fn validator_set(&self) -> &Set {
        &self.validator_set
    }

    /// Return the total weight (ie. voting power) of the network.
    pub fn total_weight(&self) -> Weight {
        self.validator_set.total_voting_power()
    }

    /// Return the votes for the given round.
    pub fn per_round(&self, round: Round) -> Option<&PerRound<V>> {
        self.per_round.get(&round)
    }

    /// Return votes for all rounds we have seen so far.
    pub fn all_rounds(&self) -> &BTreeMap<Round, PerRound<V>> {
        &self.per_round
    }

    /// Return how many rounds we have seen votes for so far.
    pub fn rounds(&self) -> usize {
        self.per_round.len()
    }

    /// Return the highest round we have seen votes for so far.
    pub fn max_round(&self) -> Round {
        self.per_round.keys().max().copied().unwrap_or(Round::Nil)
    }

    /// Return the evidence of equivocation.
    pub fn evidence(&self) -> &EvidenceMap<V> {
        &self.evidence
    }

    /// Remove and return all recorded evidence.
    pub fn take_evidence(&mut self) -> EvidenceMap<V> {
        core::mem::take(&mut self.evidence)
    }

    /// Check if we have already seen a vote.
    ///
    /// Matches `PerRound::add`'s duplicate semantic: a stored vote from the
    /// same validator with the same type and value counts as "seen", unless
    /// the incoming vote carries an extension the stored one lacks — in
    /// which case the stored vote can still be upgraded.
    pub fn has_vote(&self, vote: &SignedMessage<V, V::Signature>) -> bool {
        self.per_round
            .get(&vote.round())
            .and_then(|per_round| per_round.get_vote(vote.vote_type(), vote.validator_address()))
            .is_some_and(|existing| {
                existing.value() == vote.value()
                    && !(existing.extension().is_none() && vote.extension().is_some())
            })
    }

    /// A conflicting vote from this validator that would not add evidence
    /// because the per-validator cap is already full.
    ///
    /// The first few conflicts still have to pass the already-seen gate so
    /// they can reach [`EvidenceMap::add`]. Once the map is full, further
    /// distinct values teach the node nothing and must not be verified or
    /// WAL-appended.
    pub fn is_saturated_conflict(&self, vote: &SignedMessage<V, V::Signature>) -> bool {
        let Some(existing) = self
            .per_round
            .get(&vote.round())
            .and_then(|per_round| per_round.get_vote(vote.vote_type(), vote.validator_address()))
        else {
            return false;
        };
        existing.value() != vote.value() && self.evidence.is_full(vote.validator_address())
    }

    /// Apply a vote with a given weight, potentially triggering an output.
    pub fn apply_vote(
        &mut self,
        vote: SignedMessage<V, V::Signature>,
        round: Round,
    ) -> Option<Output<V::ValueId>> {
        let total_weight = self.total_weight();
        let per_round =
            self.per_round
                .entry(vote.round())
                .or_insert(PerRound::with_expected_number_of_votes(
                    self.validator_set.count() * VOTES_PER_VALIDATOR_PER_ROUND,
                ));

        let Some(validator) = self.validator_set.get_by_address(vote.validator_address()) else {
            // Vote from unknown validator, let's discard it.
            return None;
        };

        match per_round.add(vote.clone(), validator.voting_power()) {
            Ok(()) => (),
            Err(RecordVoteError::ConflictingVote {
                existing,
                conflicting,
            }) => {
                // This is an equivocating vote
                warn!(
                    "Received equivocating vote {:?}, existing {:?}",
                    conflicting, existing
                );
                self.evidence.add(existing, conflicting);
                return None;
            }
        }

        let threshold = compute_threshold(
            vote.vote_type(),
            per_round,
            vote.value(),
            self.threshold_params,
            total_weight,
        );

        let skip_round = if vote.round() > round
            && self
                .threshold_params
                .honest
                .is_met(per_round.addresses_weights.sum(), total_weight)
        {
            Some(vote.round())
        } else {
            None
        };

        let output = threshold_to_output(vote.vote_type(), threshold, skip_round);

        match output {
            // Ensure we do not output the same message twice
            Some(output) if !per_round.emitted_outputs.contains(&output) => {
                per_round.emitted_outputs.insert(output.clone());
                Some(output)
            }
            _ => None,
        }
    }

    /// Check if a threshold is met, ie. if we have a quorum for that threshold.
    pub fn is_threshold_met(
        &self,
        round: &Round,
        vote_type: VoteType,
        threshold: Threshold<V::ValueId>,
    ) -> bool {
        self.per_round.get(round).is_some_and(|per_round| {
            per_round.votes.is_threshold_met(
                vote_type,
                threshold,
                self.threshold_params.quorum,
                self.total_weight(),
            )
        })
    }

    /// Prunes all stored votes from rounds less than `min_round`.
    pub fn prune_votes(&mut self, min_round: Round) {
        self.per_round.retain(|round, _| *round >= min_round);
    }

    /// Return the stored vote that conflicts with `vote` — same validator, same vote type,
    /// different value — without mutating the keeper. Returns `None` if no conflict exists.
    pub fn conflicting_vote<'a>(
        &'a self,
        vote: &'a SignedMessage<V, V::Signature>,
    ) -> Option<&'a SignedMessage<V, V::Signature>> {
        let existing = self
            .per_round
            .get(&vote.round())?
            .get_vote(vote.vote_type(), vote.validator_address())?;

        if existing.value() != vote.value() {
            Some(existing)
        } else {
            None
        }
    }

    /// Return whether `vote` has already been recorded as part of an equivocation pair.
    ///
    /// Lets callers skip re-verifying the signature of a conflicting vote whose evidence
    /// is already stored.
    pub fn has_equivocation_evidence(&self, vote: &SignedMessage<V, V::Signature>) -> bool {
        self.evidence
            .get(vote.validator_address())
            .is_some_and(|pairs| pairs.iter().any(|(e, c)| e == vote || c == vote))
    }

    /// Return whether [`Self::detect_equivocation`] would record a new pair for `vote`.
    ///
    /// False when `vote` raises no conflict, when its pair is already stored, and when the
    /// validator has reached the per-validator evidence cap — past the cap no further pair is
    /// retained, so a conflicting vote would otherwise never satisfy
    /// [`Self::has_equivocation_evidence`] and would be reconsidered on every delivery.
    ///
    /// This is the guard [`Self::detect_equivocation`] itself applies, so callers can use it to
    /// gate the cost of verifying `vote`'s signature on there being something to gain from it.
    pub fn can_record_equivocation(&self, vote: &SignedMessage<V, V::Signature>) -> bool {
        self.conflicting_vote(vote).is_some()
            && !self.has_equivocation_evidence(vote)
            && !self.evidence.is_full(vote.validator_address())
    }

    /// Record `vote` as equivocation evidence when it conflicts with a stored vote from the
    /// same validator, returning the conflicting pair only when it is newly recorded.
    ///
    /// Repeated deliveries of the same conflicting vote, and deliveries once the validator has
    /// reached the per-validator cap, retain nothing and return `None`.
    pub fn detect_equivocation(
        &mut self,
        vote: SignedMessage<V, V::Signature>,
    ) -> Option<VotePair<V>> {
        if !self.can_record_equivocation(&vote) {
            return None;
        }

        let existing = self.conflicting_vote(&vote)?.clone();
        self.evidence.add(existing.clone(), vote.clone());

        Some((existing, vote))
    }
}

/// Compute whether or not we have reached a threshold for the given value,
/// and return that threshold.
fn compute_threshold<V>(
    vote_type: VoteType,
    round: &PerRound<V>,
    value: &NilOrVal<V::ValueId>,
    thresholds: ThresholdParams,
    total_weight: Weight,
) -> Threshold<V::ValueId>
where
    V: Vote,
{
    let weight = round.votes.get_weight(vote_type, value);

    match value {
        NilOrVal::Val(value) if thresholds.quorum.is_met(weight, total_weight) => {
            Threshold::Value(value.clone())
        }

        NilOrVal::Nil if thresholds.quorum.is_met(weight, total_weight) => Threshold::Nil,

        _ => {
            let weight_sum = round.votes.weight_sum(vote_type);

            if thresholds.quorum.is_met(weight_sum, total_weight) {
                Threshold::Any
            } else {
                Threshold::Unreached
            }
        }
    }
}

/// Map a vote type and a threshold to a state machine output.
/// Also considers an optional honest threshold for a future round.
fn threshold_to_output<Value>(
    typ: VoteType,
    threshold: Threshold<Value>,
    future_round: Option<Round>,
) -> Option<Output<Value>> {
    if let Some(future_round) = future_round {
        // Only PrecommitValue(v) has larger priority than SkipRound(r)
        match (typ, threshold) {
            (VoteType::Precommit, Threshold::Value(v)) => Some(Output::PrecommitValue(v)),

            (_, _) => Some(Output::SkipRound(future_round)),
        }
    } else {
        // Thresholds for the current round
        match (typ, threshold) {
            (_, Threshold::Unreached) => None,

            (VoteType::Prevote, Threshold::Any) => Some(Output::PolkaAny),
            (VoteType::Prevote, Threshold::Nil) => Some(Output::PolkaNil),
            (VoteType::Prevote, Threshold::Value(v)) => Some(Output::PolkaValue(v)),

            (VoteType::Precommit, Threshold::Any) => Some(Output::PrecommitAny),
            (VoteType::Precommit, Threshold::Nil) => Some(Output::PrecommitAny),
            (VoteType::Precommit, Threshold::Value(v)) => Some(Output::PrecommitValue(v)),
        }
    }
}
