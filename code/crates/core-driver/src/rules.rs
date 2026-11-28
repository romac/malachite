//! Read-only Tendermint rules over the evidence retained at this height.
//!
//! Proposals and threshold notifications identify which round and value to examine.
//! A verified certificate is also a quorum proof in its own right, even if the vote
//! keeper retained an earlier, conflicting vote from one of its signers.
//!
//! | Rule | Evidence | Input round |
//! |---|---|---|
//! | L22 | Fresh proposal in Propose | Current round |
//! | L28 | Current proposal cites a matching earlier value polka | Proposal round |
//! | L36 | Valid current proposal and a current value polka | Current round |
//! | L34 / L44 | Any-value / nil prevote quorum in Prevote | Current round |
//! | L47 | Any-value precommit quorum | Current round |
//! | L49 | Valid candidate and a matching value precommit quorum | Certified round |
//! | L55 | Enough voting power to enter a higher round | Future round |

use malachitebft_core_state_machine::input::Input as RoundInput;
use malachitebft_core_state_machine::state::Step;
use malachitebft_core_types::{
    Context, Proposal, Round, TimeoutKind, Validity, Value, ValueId, VoteType,
};
use malachitebft_core_votekeeper::keeper::Output as VKOutput;
use malachitebft_core_votekeeper::Threshold;

use crate::Driver;

/// The round in which the machine must interpret an input. For L28 this is
/// the proposal's round even when the triggering prevote is from a past round.
pub(super) struct RoundEvent<Ctx: Context> {
    pub(super) input_round: Round,
    pub(super) input: RoundInput<Ctx>,
}

impl<Ctx: Context> RoundEvent<Ctx> {
    fn new(input_round: Round, input: RoundInput<Ctx>) -> Self {
        Self { input_round, input }
    }
}

impl<Ctx: Context> Driver<Ctx> {
    fn has_polka(&self, round: Round, value: &ValueId<Ctx>) -> bool {
        self.polka_certificate(round, value).is_some()
            || self.vote_keeper.is_threshold_met(
                &round,
                VoteType::Prevote,
                Threshold::Value(value.clone()),
            )
    }

    fn has_commit_quorum(&self, round: Round, value: &ValueId<Ctx>) -> bool {
        self.commit_certificate(round, value).is_some()
            || self.vote_keeper.is_threshold_met(
                &round,
                VoteType::Precommit,
                Threshold::Value(value.clone()),
            )
    }

    /// A value precommit quorum can support L49 in *any* round of this height.
    /// With no valid candidate the same proof supports L55 or L47.
    pub(super) fn commit_rule(
        &self,
        proof_round: Round,
        candidate: Option<&Ctx::Proposal>,
    ) -> Option<RoundEvent<Ctx>> {
        let input = match candidate {
            Some(proposal) => RoundInput::ProposalAndPrecommitValue(proposal.clone()),
            None if proof_round > self.round() => RoundInput::SkipRound(proof_round),
            None => return self.precommit_timer_rule(proof_round),
        };
        Some(RoundEvent::new(proof_round, input))
    }

    pub(super) fn stored_commit_rule(
        &self,
        proof_round: Round,
        value: ValueId<Ctx>,
    ) -> Option<RoundEvent<Ctx>> {
        let proposal = self.valid_proposal_for_round_and_value(proof_round, value);
        self.commit_rule(proof_round, proposal.map(|signed| &signed.message))
    }

    /// Join a proposal and a prevote quorum on their value and the round the
    /// proposal needs. Only the caller's affected proof round is considered.
    fn polka_rule(
        &self,
        proposal: &Ctx::Proposal,
        validity: Validity,
        proof_round: Round,
    ) -> Option<RoundEvent<Ctx>> {
        let round = proposal.round();
        if round != self.round() || !proof_round.is_defined() {
            return None;
        }
        let value = proposal.value().id();
        match self.step() {
            // L28: evidence is from proposal.pol_round(), the prevote is in proposal.round().
            Step::Propose
                if proof_round == proposal.pol_round()
                    && proof_round < round
                    && self.has_polka(proof_round, &value) =>
            {
                let input = match validity {
                    Validity::Valid => RoundInput::ProposalAndPolkaPrevious(proposal.clone()),
                    Validity::Invalid => {
                        RoundInput::InvalidProposalAndPolkaPrevious(proposal.clone())
                    }
                };
                Some(RoundEvent::new(round, input))
            }
            // L36: in Prevote this locks and precommits. In Precommit only the
            // valid value changes, and applying the same update again has no effect.
            Step::Prevote | Step::Precommit
                if proof_round == round && validity.is_valid() && self.has_polka(round, &value) =>
            {
                if self.step() == Step::Precommit
                    && self
                        .valid_value()
                        .is_some_and(|valid| valid.round == round && valid.value.id() == value)
                {
                    return None;
                }
                Some(RoundEvent::new(
                    round,
                    RoundInput::ProposalAndPolkaCurrent(proposal.clone()),
                ))
            }
            _ => None,
        }
    }

    pub(super) fn proposal_rule(
        &self,
        proposal: &Ctx::Proposal,
        validity: Validity,
        entry_step: Step,
    ) -> Option<RoundEvent<Ctx>> {
        if self.round().is_nil() {
            return None;
        }
        let round = proposal.round();
        if validity.is_valid()
            && self.round_state.decision.is_none()
            && self.has_commit_quorum(round, &proposal.value().id())
        {
            return self.commit_rule(round, Some(proposal));
        }
        if round != self.round() || entry_step != self.step() {
            return None;
        }
        match entry_step {
            // L22: a fresh proposal does not cite a previous polka.
            Step::Propose if proposal.pol_round().is_nil() => {
                let input = match validity {
                    Validity::Valid => RoundInput::Proposal(proposal.clone()),
                    Validity::Invalid => RoundInput::InvalidProposal,
                };
                Some(RoundEvent::new(round, input))
            }
            Step::Propose => self.polka_rule(proposal, validity, proposal.pol_round()),
            Step::Prevote | Step::Precommit => self.polka_rule(proposal, validity, round),
            _ => None,
        }
    }

    pub(super) fn stored_polka_rule(
        &self,
        proof_round: Round,
        value: ValueId<Ctx>,
    ) -> Option<RoundEvent<Ctx>> {
        if proof_round > self.round() {
            return Some(RoundEvent::new(
                proof_round,
                RoundInput::SkipRound(proof_round),
            ));
        }
        // Distinct proposal messages can have the same value and cite different
        // POL rounds. Select the first one for which this proof enables a rule.
        self.proposals_and_validities_for_round(self.round())
            .iter()
            .filter(|(proposal, _)| proposal.value().id() == value)
            .find_map(|(proposal, validity)| {
                self.polka_rule(&proposal.message, *validity, proof_round)
            })
            // A value polka still proves the any-value prevote quorum (L34).
            .or_else(|| self.prevote_timer_rule(proof_round))
    }

    fn prevote_timer_rule(&self, round: Round) -> Option<RoundEvent<Ctx>> {
        (round == self.round()
            && self.step() == Step::Prevote
            && !self
                .round_state
                .scheduled_timeouts
                .is_scheduled(TimeoutKind::Prevote))
        .then(|| RoundEvent::new(round, RoundInput::PolkaAny))
    }

    fn precommit_timer_rule(&self, round: Round) -> Option<RoundEvent<Ctx>> {
        (round == self.round()
            && self.step() != Step::Commit
            && !self
                .round_state
                .scheduled_timeouts
                .is_scheduled(TimeoutKind::Precommit))
        .then(|| RoundEvent::new(round, RoundInput::PrecommitAny))
    }

    pub(super) fn threshold_rule(
        &self,
        round: Round,
        threshold: VKOutput<ValueId<Ctx>>,
    ) -> Option<RoundEvent<Ctx>> {
        let input = match threshold {
            VKOutput::PolkaValue(v) => return self.stored_polka_rule(round, v),
            VKOutput::PrecommitValue(v) => return self.stored_commit_rule(round, v),
            VKOutput::PolkaAny => return self.prevote_timer_rule(round),
            VKOutput::PolkaNil => RoundInput::PolkaNil,
            VKOutput::PrecommitAny => return self.precommit_timer_rule(round),
            VKOutput::SkipRound(target) => RoundInput::SkipRound(target),
        };
        Some(RoundEvent::new(round, input))
    }

    pub(super) fn proposal_on_step_entry(
        &self,
        entry_step: Step,
        index: usize,
    ) -> Option<RoundEvent<Ctx>> {
        let (proposal, validity) = &self.proposals_and_validities_for_round(self.round())[index];
        match entry_step {
            Step::Propose => self.proposal_rule(&proposal.message, *validity, entry_step),
            Step::Prevote if self.step() == entry_step => {
                self.polka_rule(&proposal.message, *validity, self.round())
            }
            _ => None,
        }
    }

    /// L44 takes precedence over L34. A nil quorum leads directly to Precommit.
    pub(super) fn prevote_quorum_rule(&self) -> Option<RoundEvent<Ctx>> {
        let round = self.round();
        if self.step() != Step::Prevote {
            return None;
        }
        if self
            .vote_keeper
            .is_threshold_met(&round, VoteType::Prevote, Threshold::Nil)
        {
            return Some(RoundEvent::new(round, RoundInput::PolkaNil));
        }
        if self
            .vote_keeper
            .is_threshold_met(&round, VoteType::Prevote, Threshold::Any)
            || self
                .polka_certificates
                .iter()
                .any(|c| c.height == self.height() && c.round == round)
        {
            return self.prevote_timer_rule(round);
        }
        None
    }

    /// L47 is independent of all the prevote rules above.
    pub(super) fn precommit_quorum_rule(&self) -> Option<RoundEvent<Ctx>> {
        let round = self.round();
        if self
            .vote_keeper
            .is_threshold_met(&round, VoteType::Precommit, Threshold::Any)
            || self
                .commit_certificates
                .iter()
                .any(|c| c.height == self.height() && c.round == round)
        {
            return self.precommit_timer_rule(round);
        }
        None
    }
}
