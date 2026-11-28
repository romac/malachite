use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use malachitebft_core_state_machine::input::Input as RoundInput;
use malachitebft_core_state_machine::output::Output as RoundOutput;
use malachitebft_core_state_machine::state::{RoundValue, State as RoundState, Step};
use malachitebft_core_state_machine::state_machine::Info;
use malachitebft_core_types::{
    Context, EnterRoundCertificate, ExtendedCommitCertificate, NilOrVal, PolkaCertificate,
    Proposal, Round, SignedProposal, SignedVote, Threshold, Timeout, TimeoutKind, Validator,
    ValidatorSet, Validity, Value, ValueId, Vote, VoteType,
};
use malachitebft_core_votekeeper::keeper::Output as VKOutput;
use malachitebft_core_votekeeper::keeper::VoteKeeper;

use crate::input::Input;
use crate::output::Output;
use crate::proposal_keeper::{self, ProposalKeeper, StoreProposalResult};
use crate::rules::RoundEvent;
use crate::Error;
use crate::ThresholdParams;

/// Driver for the state machine of the Malachite consensus engine at a given height.
pub struct Driver<Ctx>
where
    Ctx: Context,
{
    /// The context of the consensus engine,
    /// for defining the concrete data types and signature scheme.
    pub(crate) ctx: Ctx,

    /// The address of the node.
    address: Ctx::Address,

    /// Quorum thresholds
    threshold_params: ThresholdParams,

    /// The validator set at the current height
    validator_set: Ctx::ValidatorSet,

    /// The proposer for the current round, None for round nil.
    proposer: Option<Ctx::Address>,

    /// The proposals to decide on.
    pub(crate) proposal_keeper: ProposalKeeper<Ctx>,

    /// The vote keeper.
    pub(crate) vote_keeper: VoteKeeper<Ctx>,

    /// The extended commit certificates, bundling per-validator precommit
    /// signatures with their vote extensions (iff required) for each decided value
    /// observed at this height.
    pub(crate) commit_certificates: Vec<ExtendedCommitCertificate<Ctx>>,

    /// The polka certificates
    pub(crate) polka_certificates: Vec<PolkaCertificate<Ctx>>,

    /// The state of the round state machine.
    pub(crate) round_state: RoundState<Ctx>,

    last_prevote: Option<Ctx::Vote>,
    last_precommit: Option<Ctx::Vote>,

    /// The certificate that justifies moving to the `enter_round` specified in the `EnterRoundCertificate.
    pub round_certificate: Option<EnterRoundCertificate<Ctx>>,
}

impl<Ctx> Driver<Ctx>
where
    Ctx: Context,
{
    /// Create a new `Driver` instance for the given height.
    ///
    /// Called when consensus is started and initialized with the first height.
    /// Re-initialization for subsequent heights is done using `move_to_height()`.
    pub fn new(
        ctx: Ctx,
        height: Ctx::Height,
        validator_set: Ctx::ValidatorSet,
        address: Ctx::Address,
        threshold_params: ThresholdParams,
    ) -> Self {
        let proposal_keeper = ProposalKeeper::new();
        let vote_keeper = VoteKeeper::new(validator_set.clone(), threshold_params);
        let round_state = RoundState::new(height, Round::Nil);

        Self {
            ctx,
            address,
            threshold_params,
            validator_set,
            proposal_keeper,
            vote_keeper,
            round_state,
            proposer: None,
            commit_certificates: vec![],
            polka_certificates: vec![],
            last_prevote: None,
            last_precommit: None,
            round_certificate: None,
        }
    }

    /// Reset votes and round state and move to the new height with the given validator set.
    pub fn move_to_height(&mut self, height: Ctx::Height, validator_set: Ctx::ValidatorSet) {
        // Update the validator set
        self.validator_set = validator_set.clone();
        self.proposer = None;

        // Reset the vote keeper
        let vote_keeper = VoteKeeper::new(validator_set, self.threshold_params);
        self.vote_keeper = vote_keeper;

        // Reset the round state
        let round_state = RoundState::new(height, Round::Nil);
        self.round_state = round_state;
        self.round_certificate = None;

        // Reset the proposal keeper
        let proposal_keeper = ProposalKeeper::new();
        self.proposal_keeper = proposal_keeper;

        // Reset certificates
        self.commit_certificates = vec![];
        self.polka_certificates = vec![];

        // Reset additional internal state
        self.last_prevote = None;
        self.last_precommit = None;
    }

    /// Return the height of the consensus.
    pub fn height(&self) -> Ctx::Height {
        self.round_state.height
    }

    /// Return the current round we are at.
    pub fn round(&self) -> Round {
        self.round_state.round
    }

    /// Return the current step within the round we are at.
    pub fn step(&self) -> Step {
        self.round_state.step
    }

    /// Returns true if the current step is propose.
    pub fn step_is_propose(&self) -> bool {
        self.round_state.step == Step::Propose
    }

    /// Returns true if the current step is prevote.
    pub fn step_is_prevote(&self) -> bool {
        self.round_state.step == Step::Prevote
    }

    /// Returns true if the current step is precommit.
    pub fn step_is_precommit(&self) -> bool {
        self.round_state.step == Step::Precommit
    }

    /// Returns true if the current step is commit.
    pub fn step_is_commit(&self) -> bool {
        self.round_state.step == Step::Commit
    }

    /// Return the valid value (the value for which we saw a polka) for the current round, if any.
    pub fn valid_value(&self) -> Option<&RoundValue<Ctx::Value>> {
        self.round_state.valid.as_ref()
    }

    /// Return a reference to the votekeper
    pub fn votes(&self) -> &VoteKeeper<Ctx> {
        &self.vote_keeper
    }

    /// Return a mutable reference to the votekeper
    pub fn votes_mut(&mut self) -> &mut VoteKeeper<Ctx> {
        &mut self.vote_keeper
    }

    /// Return precommits for the given round and value from the vote keeper
    pub fn restore_precommits(
        &self,
        round: Round,
        value_id: &ValueId<Ctx>,
    ) -> Vec<SignedVote<Ctx>> {
        self.vote_keeper
            .per_round(round)
            .map(|per_round| per_round.precommits_for_value(value_id))
            .unwrap_or_default()
    }

    /// Return a reference to the proposal keeper
    pub fn proposals(&self) -> &ProposalKeeper<Ctx> {
        &self.proposal_keeper
    }

    /// Return the state for the current round.
    pub fn round_state(&self) -> &RoundState<Ctx> {
        &self.round_state
    }

    /// Return the round and value of the decided proposal
    pub fn decided_value(&self) -> Option<(Round, Ctx::Value)> {
        self.round_state
            .decision
            .as_ref()
            .map(|decision| (decision.round, decision.value.clone()))
    }

    /// Return the address of the node.
    pub fn address(&self) -> &Ctx::Address {
        &self.address
    }

    /// Return the validator set for this height.
    pub fn validator_set(&self) -> &Ctx::ValidatorSet {
        &self.validator_set
    }

    /// Return the proposer address for the current round, if any.
    pub fn proposer_address(&self) -> Option<&Ctx::Address> {
        self.proposer.as_ref()
    }

    /// Remove and return recorded evidence of proposal equivocation.
    pub fn take_proposal_evidence(&mut self) -> proposal_keeper::EvidenceMap<Ctx> {
        self.proposal_keeper.take_evidence()
    }

    /// Record a pair of equivocating proposals as evidence in the proposal keeper.
    ///
    /// Used by upstream layers that detect equivocation but filter the conflicting proposal
    /// before it reaches the normal proposal-handling path.
    /// Returns whether the evidence pair was newly retained.
    pub fn record_proposal_evidence(
        &mut self,
        existing: SignedProposal<Ctx>,
        conflicting: SignedProposal<Ctx>,
    ) -> bool {
        self.proposal_keeper.record_evidence(existing, conflicting)
    }

    /// Remove and return recorded evidence of vote equivocation.
    pub fn take_vote_evidence(&mut self) -> malachitebft_core_votekeeper::EvidenceMap<Ctx> {
        self.vote_keeper.take_evidence()
    }

    /// Return the proposer for the current round.
    pub fn get_proposer(&self) -> Result<&Ctx::Validator, Error<Ctx>> {
        if let Some(proposer) = &self.proposer {
            let proposer = self
                .validator_set
                .get_by_address(proposer)
                .ok_or_else(|| Error::ProposerNotFound(proposer.clone()))?;

            Ok(proposer)
        } else {
            Err(Error::NoProposer(self.height(), self.round()))
        }
    }

    /// Get the extended commit certificate for the given round and value id.
    pub fn commit_certificate(
        &self,
        round: Round,
        value_id: &ValueId<Ctx>,
    ) -> Option<&ExtendedCommitCertificate<Ctx>> {
        self.commit_certificates
            .iter()
            .find(|c| &c.value_id == value_id && c.round == round && c.height == self.height())
    }

    /// Return the extended commit certificates, if any.
    pub fn commit_certificates(&self) -> &[ExtendedCommitCertificate<Ctx>] {
        self.commit_certificates.as_ref()
    }

    /// Get the polka certificates at the current height for the specified round and value, if it exists
    pub fn polka_certificate(
        &self,
        round: Round,
        value_id: &ValueId<Ctx>,
    ) -> Option<&PolkaCertificate<Ctx>> {
        self.polka_certificates
            .iter()
            .find(|c| &c.value_id == value_id && c.round == round && c.height == self.height())
    }

    /// Return the polka certificates, if any.
    pub fn polka_certificates(&self) -> &[PolkaCertificate<Ctx>] {
        self.polka_certificates.as_ref()
    }

    /// Rule evaluation completes synchronously in `process`, so the driver has no queued inputs.
    pub fn pending_inputs(&self) -> &[(Round, RoundInput<Ctx>)] {
        &[]
    }

    /// Return the last issued prevote, if any.
    pub fn last_prevote(&self) -> Option<&Ctx::Vote> {
        self.last_prevote.as_ref()
    }

    /// Return the last issued prevote, if any.
    pub fn last_precommit(&self) -> Option<&Ctx::Vote> {
        self.last_precommit.as_ref()
    }

    /// Get the round certificate for the current round.
    pub fn round_certificate(&self) -> Option<&EnterRoundCertificate<Ctx>> {
        self.round_certificate.as_ref()
    }

    /// Returns the proposal and its validity for the given round and value_id, if any.
    pub fn proposal_and_validity_for_round_and_value(
        &self,
        round: Round,
        value_id: ValueId<Ctx>,
    ) -> Option<&(SignedProposal<Ctx>, Validity)> {
        self.proposal_keeper
            .get_proposal_and_validity_for_round_and_value(round, value_id)
    }

    /// Returns a valid proposal for the given round and value_id, if any.
    pub fn valid_proposal_for_round_and_value(
        &self,
        round: Round,
        value_id: ValueId<Ctx>,
    ) -> Option<&SignedProposal<Ctx>> {
        if let Some((proposal, validity)) =
            self.proposal_and_validity_for_round_and_value(round, value_id)
        {
            if validity.is_valid() {
                return Some(proposal);
            }
        }
        None
    }

    /// Returns the proposals and their validities for the given round, if any.
    pub fn proposals_and_validities_for_round(
        &self,
        round: Round,
    ) -> &[(SignedProposal<Ctx>, Validity)] {
        self.proposal_keeper
            .get_proposals_and_validities_for_round(round)
    }

    /// Store the last vote that we have cast
    fn set_last_vote_cast(&mut self, vote: &Ctx::Vote) {
        assert_eq!(vote.height(), self.height());

        if vote.round() == self.round() {
            match vote.vote_type() {
                VoteType::Prevote => self.last_prevote = Some(vote.clone()),
                VoteType::Precommit => self.last_precommit = Some(vote.clone()),
            }
        }
    }

    /// Process the given input, returning the outputs to be broadcast to the network.
    pub fn process(&mut self, msg: Input<Ctx>) -> Result<Vec<Output<Ctx>>, Error<Ctx>> {
        let mut outputs = Vec::new();
        let mut previous = (self.height(), self.step());
        if let Some(output) = self.apply(msg)? {
            self.lift_output(output, &mut outputs);
        }

        // Finish the rules for one step entry before starting the next. This
        // preserves the ordering of already-enabled timers relative to votes.
        // Each pass visits a finite set of proposals once, then both aggregate
        // quorums. Another pass requires a step advance, even for silent rules.
        while previous != (self.height(), self.step()) {
            let entered = self.step();
            if matches!(entered, Step::Unstarted | Step::Commit) {
                break;
            }
            // L36 is already eligible in Prevote. A later proposal or polka can
            // still update the valid value in Precommit without a step change.
            if matches!(entered, Step::Propose | Step::Prevote) {
                let count = self.proposals_and_validities_for_round(self.round()).len();
                for index in 0..count {
                    let rule = self.proposal_on_step_entry(entered, index);
                    self.emit_rule(rule, &mut outputs)?;
                }
            }
            self.emit_rule(self.prevote_quorum_rule(), &mut outputs)?;
            self.emit_rule(self.precommit_quorum_rule(), &mut outputs)?;
            previous = (self.height(), entered);
        }
        Ok(outputs)
    }

    fn emit_rule(
        &mut self,
        rule: Option<RoundEvent<Ctx>>,
        outputs: &mut Vec<Output<Ctx>>,
    ) -> Result<(), Error<Ctx>> {
        if let Some(output) = self.apply_rule(rule)? {
            self.lift_output(output, outputs);
        }
        Ok(())
    }

    fn apply_rule(
        &mut self,
        rule: Option<RoundEvent<Ctx>>,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        match rule {
            Some(event) => self.apply_input(event.input_round, event.input),
            None => Ok(None),
        }
    }

    /// Convert the output of the round state machine to the output type of the driver.
    fn lift_output(&mut self, round_output: RoundOutput<Ctx>, outputs: &mut Vec<Output<Ctx>>) {
        match round_output {
            RoundOutput::NewRound(round) => outputs.push(Output::NewRound(self.height(), round)),

            RoundOutput::Proposal(proposal) => outputs.push(Output::Propose(proposal)),

            RoundOutput::Vote(vote) => self.lift_vote_output(vote, outputs),

            RoundOutput::ScheduleTimeout(timeout) => outputs.push(Output::ScheduleTimeout(timeout)),

            RoundOutput::GetValueAndScheduleTimeout(height, round, timeout) => {
                outputs.push(Output::ScheduleTimeout(timeout));
                outputs.push(Output::GetValue(height, round, timeout));
            }

            RoundOutput::Decision(round, proposal) => outputs.push(Output::Decide(round, proposal)),
        }
    }

    fn lift_vote_output(&mut self, vote: Ctx::Vote, outputs: &mut Vec<Output<Ctx>>) {
        if vote.validator_address() != self.address() {
            return;
        }

        // Only cast a vote if any of the following is true:
        // - We have not voted yet
        // - That vote is for a higher height than our last vote
        // - That vote is for a higher round than our last vote
        // - That vote is the same as our last vote
        // Precommits have the additional constraint that the value must match the valid value
        let can_vote = match vote.vote_type() {
            VoteType::Prevote => self.last_prevote.as_ref().is_none_or(|prev| {
                prev.height() < vote.height() || prev.round() < vote.round() || prev == &vote
            }),
            VoteType::Precommit => {
                let good_precommit = self.last_precommit.as_ref().is_none_or(|prev| {
                    prev.height() < vote.height() || prev.round() < vote.round() || prev == &vote
                });
                let match_valid = self.round_state.valid.as_ref().is_none_or(|valid| {
                    if let NilOrVal::Val(value_id) = vote.value() {
                        &valid.value.id() == value_id
                    } else {
                        true
                    }
                });
                good_precommit && match_valid
            }
        };

        if can_vote {
            self.set_last_vote_cast(&vote);
            outputs.push(Output::Vote(vote));
        }
    }

    /// Apply the given input to the state machine, returning the output, if any.
    fn apply(&mut self, input: Input<Ctx>) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        match input {
            Input::CommitCertificate(certificate) => self.apply_commit_certificate(certificate),
            Input::PolkaCertificate(certificate) => self.apply_polka_certificate(certificate),
            Input::NewRound(height, round, proposer) => {
                self.apply_new_round(height, round, proposer)
            }
            Input::ProposeValue(round, value) => self.apply_propose_value(round, value),
            Input::Proposal(proposal, validity) => self.apply_proposal(proposal, validity),
            Input::Vote(vote) => self.apply_vote(vote),
            Input::TimeoutElapsed(timeout) => self.apply_timeout(timeout),
            Input::SyncDecision(proposal) => self.apply_decide_on_sync(proposal),
        }
    }

    fn apply_commit_certificate(
        &mut self,
        certificate: ExtendedCommitCertificate<Ctx>,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        if self.height() != certificate.height {
            return Err(Error::InvalidCertificateHeight {
                certificate_height: certificate.height,
                consensus_height: self.height(),
            });
        }

        let round = certificate.round;

        // A commit certificate for a future round contains 2f+1 precommits,
        // which is more than enough to justify entering that round (f+1 suffices).
        // Store it as a round certificate before applying the rule,
        // so that a SkipRound transition has a proper justification.
        if round > self.round() {
            self.store_round_certificate_from_commit_certificate(&certificate);
        }

        let value = certificate.value_id.clone();
        self.record_commit_certificate(certificate);
        self.apply_rule(self.stored_commit_rule(round, value))
    }

    fn apply_polka_certificate(
        &mut self,
        certificate: PolkaCertificate<Ctx>,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        if self.height() != certificate.height {
            return Err(Error::InvalidCertificateHeight {
                certificate_height: certificate.height,
                consensus_height: self.height(),
            });
        }

        let round = certificate.round;
        // Retain the proof before a future-round certificate can trigger a skip.
        if round > self.round() {
            self.store_round_certificate_from_polka_certificate(&certificate);
        }

        let value = certificate.value_id.clone();
        self.record_polka_certificate(certificate);
        self.apply_rule(self.stored_polka_rule(round, value))
    }

    fn apply_new_round(
        &mut self,
        height: Ctx::Height,
        round: Round,
        proposer: Ctx::Address,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        if self.height() == height {
            // If it's a new round for same height, just reset the round, keep the valid and locked values
            self.round_state.round = round;
        } else {
            self.round_state = RoundState::new(height, round);
        }

        // Update the proposer for the new round
        self.proposer = Some(proposer);

        self.apply_input(round, RoundInput::NewRound(round))
    }

    fn apply_propose_value(
        &mut self,
        round: Round,
        value: Ctx::Value,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        self.apply_input(round, RoundInput::ProposeValue(value))
    }

    fn apply_proposal(
        &mut self,
        proposal: SignedProposal<Ctx>,
        validity: Validity,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        if self.height() != proposal.height() {
            return Err(Error::InvalidProposalHeight {
                proposal_height: proposal.height(),
                consensus_height: self.height(),
            });
        }

        let message = proposal.message.clone();
        let (outcome, retained) = self.proposal_keeper.store_with_validity(proposal, validity);
        if let StoreProposalResult::Equivocation {
            existing,
            conflicting,
        } = outcome
        {
            self.proposal_keeper.record_evidence(existing, conflicting);
        }
        self.apply_rule(self.proposal_rule(&message, retained, self.step()))
    }

    fn apply_vote(
        &mut self,
        vote: SignedVote<Ctx>,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        if self.height() != vote.height() {
            return Err(Error::InvalidVoteHeight {
                vote_height: vote.height(),
                consensus_height: self.height(),
            });
        }

        if self
            .validator_set
            .get_by_address(vote.validator_address())
            .is_none()
        {
            return Err(Error::ValidatorNotFound(vote.validator_address().clone()));
        }

        let vote_round = vote.round();
        let vote_value = vote.value().clone();
        let this_round = self.round();

        let Some(output) = self.vote_keeper.apply_vote(vote, this_round) else {
            return Ok(None);
        };

        match &output {
            VKOutput::PolkaValue(val) => self.build_polka_certificate(vote_round, val),
            // Only store PrecommitAny certificates for the current round:
            // - Lower round PrecommitAny is ignored
            // - Higher round PrecommitAny cannot occur because receiving 2f+1
            //   Precommit votes for a higher round would first generate a SkipRound certificate,
            //   advancing the node to that round before the PrecommitAny is processed
            VKOutput::PrecommitAny if this_round == vote_round => {
                self.store_precommit_any_round_certificate(vote_round)
            }
            VKOutput::SkipRound(round) => {
                self.store_skip_round_certificate(*round);
                // For future rounds, SkipRound may have taken priority over PolkaValue.
                // In this case, store the associated polka certificate so it is available
                // for the liveness protocol in the case a Precommit message is published.
                if let NilOrVal::Val(value_id) = &vote_value {
                    if self.vote_keeper.is_threshold_met(
                        &vote_round,
                        VoteType::Prevote,
                        Threshold::Value(value_id.clone()),
                    ) {
                        self.build_polka_certificate(vote_round, value_id);
                    }
                }
            }
            // For future rounds, PrecommitValue may have taken priority over SkipRound
            // in the vote keeper. Store the skip round certificate here so it's available
            // if the quorum rule falls back to SkipRound (e.g. no valid proposal).
            VKOutput::PrecommitValue(_) if vote_round > this_round => {
                self.store_skip_round_certificate(vote_round)
            }
            _ => (),
        }

        self.apply_rule(self.threshold_rule(vote_round, output))
    }

    fn apply_timeout(&mut self, timeout: Timeout) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        let input = match timeout.kind {
            TimeoutKind::Propose => RoundInput::TimeoutPropose,
            TimeoutKind::Prevote => RoundInput::TimeoutPrevote,
            TimeoutKind::Precommit => RoundInput::TimeoutPrecommit,

            // The driver never receives these events, so we can just ignore them.
            TimeoutKind::Rebroadcast => return Ok(None),
            TimeoutKind::FinalizeHeight(_) => return Ok(None),
        };

        let output = self.apply_input(timeout.round, input)?;

        // If a precommit timeout caused the state machine to advance to a new
        // round, ensure a precommit round certificate justifying that round is
        // stored.
        if matches!(timeout.kind, TimeoutKind::Precommit) {
            if let Some(RoundOutput::NewRound(new_round)) = output.as_ref() {
                let needs_refresh = self
                    .round_certificate
                    .as_ref()
                    .is_none_or(|cert| cert.enter_round != *new_round);
                if needs_refresh {
                    self.ensure_precommit_round_certificate(timeout.round);
                }
            }
        }

        Ok(output)
    }

    /// Apply a sync decision using the provided unsigned proposal.
    ///
    /// This is used when we receive a value and commit certificate from sync.
    /// The certificate must already be stored in the driver (from earlier processing).
    /// We use the normal state machine decision path with `ProposalAndPrecommitValue`.
    fn apply_decide_on_sync(
        &mut self,
        proposal: Ctx::Proposal,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        let round = proposal.round();
        let value_id = proposal.value().id();

        // The certificate should already be stored - it was looked up by the caller
        // in on_proposed_value before calling SyncDecision
        let certificate = self.commit_certificate(round, &value_id).ok_or_else(|| {
            Error::CertificateNotFound {
                round,
                value_id: value_id.clone(),
            }
        })?;

        // Sanity check: certificate height should match
        debug_assert_eq!(
            certificate.height,
            self.height(),
            "Certificate height mismatch"
        );

        // Go through the state machine with ProposalAndPrecommitValue
        // This will trigger L49 and produce a Decision output
        self.apply_rule(self.commit_rule(round, Some(&proposal)))
    }

    /// Apply the input, update the state.
    fn apply_input(
        &mut self,
        input_round: Round,
        input: RoundInput<Ctx>,
    ) -> Result<Option<RoundOutput<Ctx>>, Error<Ctx>> {
        // All fallible work MUST happen before `with_round_state` — that
        // helper's closure cannot `?` out, so nothing between the take and
        // the replace can leave `round_state` at its default.
        let proposer_address = self.get_proposer()?.address().clone();

        Ok(self.with_round_state(|ctx, address, round_state| {
            let info = Info::new(input_round, address, &proposer_address);
            let transition = round_state.apply(ctx, &info, input);
            (transition.next_state, transition.output)
        }))
    }

    /// Atomically transition `round_state` via the given infallible closure.
    ///
    /// The closure's signature forces the caller to return a replacement
    /// `RoundState`; there is no way to `?` out of it, so `round_state` can
    /// never be left at its `Default::default()` value, which can be inconsistent.
    /// Any fallible work
    /// must therefore be done before calling this helper, where an early
    /// return is safe.
    fn with_round_state<T>(
        &mut self,
        f: impl FnOnce(&Ctx, &Ctx::Address, RoundState<Ctx>) -> (RoundState<Ctx>, T),
    ) -> T {
        let taken = core::mem::take(&mut self.round_state);
        let (new_state, result) = f(&self.ctx, &self.address, taken);
        self.round_state = new_state;
        result
    }

    /// Return the traces logged during execution.
    #[cfg(feature = "debug")]
    pub fn get_traces(&self) -> &[malachitebft_core_state_machine::traces::Trace<Ctx>] {
        self.round_state.get_traces()
    }
}

impl<Ctx> fmt::Debug for Driver<Ctx>
where
    Ctx: Context,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Driver")
            .field("address", &self.address)
            .field("validator_set", &self.validator_set)
            .field("proposal", &self.proposal_keeper)
            .field("votes", &self.vote_keeper)
            .field("round_state", &self.round_state)
            .finish()
    }
}
