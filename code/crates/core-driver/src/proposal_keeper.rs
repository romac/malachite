//! For storing proposals.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use core::fmt::Debug;

use derive_where::derive_where;

use malachitebft_core_types::{Proposal, Round, SignedMessage, Validity, Value};
pub use malachitebft_core_votekeeper::evidence::MAX_EVIDENCE_PER_VALIDATOR;
use tracing::{error, warn};

/// Two signed proposals retained as evidence of equivocation.
pub type ProposalPair<P, Sig> = (SignedMessage<P, Sig>, SignedMessage<P, Sig>);

/// Outcome of storing a proposal in a [`PerRound`] / [`ProposalKeeper`].
///
/// Equivocation is surfaced to the caller rather than recorded by the keeper, matching the
/// convention of the full-proposal keeper in the consensus layer. The caller records the pair
/// via [`ProposalKeeper::record_evidence`].
#[must_use]
#[derive_where(Clone, Debug)]
pub enum StoreProposalResult<P: Proposal, Sig: Clone + Debug + Eq> {
    /// The proposal was stored, or a duplicate proposal message was ignored.
    Stored,
    /// A different proposal from the same validator is already stored for this round, so the
    /// proposer has equivocated. Both proposals are returned so the caller can record evidence.
    Equivocation {
        /// The proposal already stored for this round.
        existing: SignedMessage<P, Sig>,
        /// The conflicting proposal, from the same validator.
        conflicting: SignedMessage<P, Sig>,
    },
}

/// The proposals received in a given round, if any.
#[derive_where(Clone, Debug, PartialEq, Eq, Default)]
pub struct PerRound<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    /// The proposals received in a given round (proposal.round) if any.
    proposals: Vec<(SignedMessage<P, Sig>, Validity)>,
}

impl<P, Sig> PerRound<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    /// Return the first proposal and its validity that matches the given value_id, if any.
    fn get_first_proposal_and_validity(
        &self,
        value_id: <P::Value as Value>::Id,
    ) -> Option<&(SignedMessage<P, Sig>, Validity)> {
        self.proposals
            .iter()
            .find(|(proposal, _)| proposal.value().id() == value_id)
    }

    // /// Return the first proposal, if any, without validity.
    fn get_first_proposal(&self) -> Option<&SignedMessage<P, Sig>> {
        self.proposals.first().map(|(p, _)| p)
    }

    /// Returns all proposals and their validities.
    pub fn get_proposals_and_validities(&self) -> &[(SignedMessage<P, Sig>, Validity)] {
        &self.proposals
    }

    /// Add a proposal to this round, checking for conflicts.
    ///
    /// All proposals must come from the same validator (proposer).
    /// If a proposal comes from a different validator than the first,
    /// this is considered a calling code bug and the function will panic.
    ///
    /// - Stores each unique proposal message once.
    /// - Returns [`StoreProposalResult::Equivocation`] if equivocation is detected from the
    ///   **same** validator.
    /// - Panics if proposals come from **different validators**.
    pub fn add(
        &mut self,
        proposal: SignedMessage<P, Sig>,
        validity: Validity,
    ) -> StoreProposalResult<P, Sig> {
        // Signatures are not part of proposal identity.
        if self.contains_message_with_validity(&proposal, validity) {
            return StoreProposalResult::Stored;
        }

        // Ensure all proposals come from the same validator
        self.verify_same_validator(&proposal);

        // Update existing proposal or add new one
        match self.proposal_validity_mut(&proposal) {
            Some(existing_validity) => {
                Self::update_validity(&proposal, existing_validity, validity);
            }
            None => {
                self.proposals.push((proposal.clone(), validity));
            }
        }

        // Check for equivocation (multiple distinct proposals)
        self.check_equivocation(proposal)
    }

    fn contains_message_with_validity(
        &self,
        proposal: &SignedMessage<P, Sig>,
        validity: Validity,
    ) -> bool {
        self.proposals
            .iter()
            .any(|(p, v)| p.message == proposal.message && *v == validity)
    }

    fn verify_same_validator(&self, proposal: &SignedMessage<P, Sig>) {
        if let Some(first) = self.get_first_proposal() {
            assert_eq!(
                first.validator_address(),
                proposal.validator_address(),
                "BUG: Received proposals from different validators in the same round.\n\
                Existing: {:?}, New: {:?}",
                first.validator_address(),
                proposal.validator_address()
            );
        }
    }

    fn proposal_validity_mut(&mut self, proposal: &SignedMessage<P, Sig>) -> Option<&mut Validity> {
        self.proposals
            .iter_mut()
            .find(|(p, _)| p.message == proposal.message)
            .map(|(_, v)| v)
    }

    fn update_validity(proposal: &SignedMessage<P, Sig>, current: &mut Validity, new: Validity) {
        use Validity::{Invalid, Valid};

        match (&current, &new) {
            (Invalid, Valid) => {
                warn!(
                    height = %proposal.message.height(),
                    round = %proposal.message.round(),
                    value_id = %proposal.message.value().id(),
                    "Application changed its mind on proposal's validity: Invalid --> Valid"
                );
                *current = new;
            }
            (Valid, Invalid) => {
                error!(
                    height = %proposal.message.height(),
                    round = %proposal.message.round(),
                    value_id = %proposal.message.value().id(),
                    "Application changed its mind on proposal's validity: Valid --> Invalid; \
                    this should not happen"
                );
            }
            _ => {
                // Same validity, no action needed
            }
        }
    }

    fn check_equivocation(&self, proposal: SignedMessage<P, Sig>) -> StoreProposalResult<P, Sig> {
        if self.proposals.len() > 1 {
            let existing = self
                .get_first_proposal()
                .expect("at least one proposal should exist")
                .clone();

            StoreProposalResult::Equivocation {
                existing,
                conflicting: proposal,
            }
        } else {
            StoreProposalResult::Stored
        }
    }
}

/// Keeps track of proposals.
#[derive_where(Clone, Debug, Default)]
pub struct ProposalKeeper<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    /// The proposal for each round.
    per_round: BTreeMap<Round, PerRound<P, Sig>>,

    /// Evidence of equivocation.
    evidence: EvidenceMap<P, Sig>,
}

impl<P, Sig> ProposalKeeper<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    /// Create a new `ProposalKeeper` instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the proposal and its validity for the round, matching the value_id, if any.
    pub fn get_proposal_and_validity_for_round_and_value(
        &self,
        round: Round,
        value_id: <P::Value as Value>::Id,
    ) -> Option<&(SignedMessage<P, Sig>, Validity)> {
        self.per_round
            .get(&round)
            .and_then(|round_info| round_info.get_first_proposal_and_validity(value_id))
    }

    /// Returns all proposals and their validities for the round, if any.
    pub fn get_proposals_and_validities_for_round(
        &self,
        round: Round,
    ) -> &[(SignedMessage<P, Sig>, Validity)] {
        self.per_round
            .get(&round)
            .map(PerRound::get_proposals_and_validities)
            .unwrap_or(&[])
    }

    /// Returns all proposals and their validities for all rounds.
    pub fn all_rounds(&self) -> &BTreeMap<Round, PerRound<P, Sig>> {
        &self.per_round
    }

    /// Return the evidence of equivocation.
    pub fn evidence(&self) -> &EvidenceMap<P, Sig> {
        &self.evidence
    }

    /// Remove and return all recorded evidence.
    pub fn take_evidence(&mut self) -> EvidenceMap<P, Sig> {
        core::mem::take(&mut self.evidence)
    }

    /// Store a proposal, returning whether it revealed an equivocation.
    ///
    /// On [`StoreProposalResult::Equivocation`] the caller records the surfaced pair via
    /// [`record_evidence`](Self::record_evidence).
    pub fn store_proposal(
        &mut self,
        proposal: SignedMessage<P, Sig>,
        validity: Validity,
    ) -> StoreProposalResult<P, Sig> {
        self.per_round
            .entry(proposal.round())
            .or_default()
            .add(proposal, validity)
    }

    /// Record a pair of equivocating proposals directly in the evidence map.
    ///
    /// Callers record evidence here after [`store_proposal`](Self::store_proposal) returns a
    /// [`StoreProposalResult::Equivocation`], or when an upstream layer detects equivocation for
    /// two proposals that share a value id but differ in another field (such as `pol_round`) and
    /// filters the conflicting one before it reaches the per-round store.
    /// [`EvidenceMap::add`] deduplicates and caps evidence per validator.
    ///
    /// Returns `true` when the pair was retained, or `false` when it was already present or the
    /// per-validator cap was reached.
    pub fn record_evidence(
        &mut self,
        existing: SignedMessage<P, Sig>,
        conflicting: SignedMessage<P, Sig>,
    ) -> bool {
        let height = conflicting.message.height();
        let round = conflicting.message.round();
        let proposer = conflicting.message.validator_address().clone();
        let value_id = conflicting.message.value().id();
        let retained = self.evidence.add(existing, conflicting);

        if retained {
            warn!(
                height = %height,
                round = %round,
                proposer = %proposer,
                value_id = %value_id,
                "Received equivocating proposal"
            );
        }

        retained
    }
}

/// Keeps track of evidence of equivocation.
#[derive_where(Clone, Debug, Default)]
pub struct EvidenceMap<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    map: BTreeMap<P::Address, Vec<ProposalPair<P, Sig>>>,
}

impl<P, Sig> EvidenceMap<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    /// Create a new `EvidenceMap` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return whether or not there is any evidence of equivocation.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Return the evidence of equivocation for a given address, if any.
    pub fn get(&self, address: &P::Address) -> Option<&Vec<ProposalPair<P, Sig>>> {
        self.map.get(address)
    }

    /// Add evidence of equivocating proposals, ie. two proposals submitted by the same validator
    /// for the same height and round that differ in any field — a different value, or the same
    /// value with a different `pol_round`, for example.
    /// If evidence for the same pair of proposal messages already exists, it will not be added
    /// again. Once a validator has [`MAX_EVIDENCE_PER_VALIDATOR`] recorded pairs, further pairs
    /// from that validator are dropped.
    ///
    /// Returns `true` when the pair was retained, or `false` when it was already present or the
    /// per-validator cap was reached.
    ///
    /// # Precondition
    /// - Both proposals must be from the same validator (debug-asserted).
    pub fn add(
        &mut self,
        existing: SignedMessage<P, Sig>,
        conflicting: SignedMessage<P, Sig>,
    ) -> bool {
        debug_assert_eq!(
            existing.validator_address(),
            conflicting.validator_address()
        );

        if let Some(evidence) = self.map.get_mut(conflicting.validator_address()) {
            // Check if this proposal-message pair already exists in either order.
            let already_exists = evidence.iter().any(|(e, c)| {
                (e.message == existing.message && c.message == conflicting.message)
                    || (e.message == conflicting.message && c.message == existing.message)
            });

            if already_exists || evidence.len() >= MAX_EVIDENCE_PER_VALIDATOR {
                return false;
            }

            evidence.push((existing, conflicting));
        } else {
            self.map.insert(
                conflicting.validator_address().clone(),
                vec![(existing, conflicting)],
            );
        }

        true
    }

    /// Return the number of addresses with recorded proposal equivocations.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Iterate over all addresses with recorded proposal equivocations.
    pub fn iter(
        &self,
    ) -> alloc::collections::btree_map::Iter<'_, P::Address, Vec<ProposalPair<P, Sig>>> {
        self.map.iter()
    }
}

impl<'a, P, Sig> IntoIterator for &'a EvidenceMap<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    type Item = (&'a P::Address, &'a Vec<ProposalPair<P, Sig>>);
    type IntoIter = alloc::collections::btree_map::Iter<'a, P::Address, Vec<ProposalPair<P, Sig>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<P, Sig> IntoIterator for EvidenceMap<P, Sig>
where
    P: Proposal,
    Sig: Clone + Debug + Eq,
{
    type Item = (P::Address, Vec<ProposalPair<P, Sig>>);
    type IntoIter = alloc::collections::btree_map::IntoIter<P::Address, Vec<ProposalPair<P, Sig>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}
