//! The consensus state machine.

use malachitebft_core_types::{NilOrVal, Proposal, Round, TimeoutKind, Value};

use crate::debug_trace;
use crate::input::Input;
use crate::output::Output;
use crate::state::{State, Step};
use crate::transition::Transition;

/// The input's round and the local node's role in the current round.
pub struct Info {
    /// Round to which the input belongs.
    pub input_round: Round,
    /// Whether this node is the proposer for the current round.
    pub is_proposer: bool,
}

impl Info {
    /// Describe an input delivered to this round.
    pub fn new(input_round: Round, is_proposer: bool) -> Self {
        Self {
            input_round,
            is_proposer,
        }
    }
}

/// Check that a proposal has a valid Proof-Of-Lock round
fn is_valid_pol_round<P>(state: &State<P>, pol_round: Round) -> bool
where
    P: Proposal,
{
    pol_round.is_defined() && pol_round < state.round
}

/// Apply an input to the current state at the current round.
///
/// This function takes the current state and round, and an input,
/// and returns the next state and an optional message for the driver to act on.
///
/// Valid transitions result in at least a change to the state and/or an output.
///
/// Commented numbers refer to line numbers in the spec paper.
pub fn apply<P>(mut state: State<P>, info: &Info, input: Input<P>) -> Transition<P>
where
    P: Proposal,
{
    let this_round = state.round == info.input_round;

    match (state.step, input) {
        //
        // From NewRound.
        //

        // L11/L14
        (Step::Unstarted, Input::NewRound(round)) if info.is_proposer => {
            state.update_round(round);

            debug_trace!(state, Line::L11Proposer);

            // We are the proposer
            propose_valid_or_get_value(state)
        }

        // L11/L20
        (Step::Unstarted, Input::NewRound(round)) => {
            state.update_round(round);

            debug_trace!(state, Line::L11NonProposer);

            // We are not the proposer
            schedule_timeout_propose(state)
        }

        //
        // From Propose. Input must be for current round.
        //

        // L18
        (Step::Propose, Input::ProposeValue(value)) if this_round => {
            debug_assert!(info.is_proposer);

            propose(state, value)
        }

        // L22 with valid proposal
        (Step::Propose, Input::Proposal(proposal))
            if this_round && proposal.pol_round().is_nil() =>
        {
            debug_trace!(state, Line::L22);

            prevote(state, &proposal)
        }

        // L22 with invalid proposal
        (Step::Propose, Input::InvalidProposal) if this_round => prevote_nil(state),

        // L28 with valid proposal
        (Step::Propose, Input::ProposalAndPolkaPrevious(proposal))
            if this_round && is_valid_pol_round(&state, proposal.pol_round()) =>
        {
            debug_trace!(state, Line::L28ValidProposal);
            prevote_previous(state, &proposal)
        }

        // L28 with invalid proposal
        (Step::Propose, Input::InvalidProposalAndPolkaPrevious(proposal))
            if this_round && is_valid_pol_round(&state, proposal.pol_round()) =>
        {
            debug_trace!(state, Line::L28InvalidProposal);
            debug_trace!(state, Line::L32InvalidValue);

            prevote_nil(state)
        }

        // L57
        // We are the proposer.
        (Step::Propose, Input::TimeoutPropose) if this_round && info.is_proposer => {
            debug_trace!(state, Line::L59Proposer);

            prevote_nil(state)
        }

        // L57
        // We are not the proposer.
        (Step::Propose, Input::TimeoutPropose) if this_round => {
            debug_trace!(state, Line::L59NonProposer);

            prevote_nil(state)
        }

        //
        // From Prevote. Input must be for current round.
        //

        // L34
        (Step::Prevote, Input::PolkaAny) if this_round => {
            debug_trace!(state, Line::L34);
            debug_trace!(state, Line::L35);

            schedule_timeout_prevote(state)
        }

        // L45
        (Step::Prevote, Input::PolkaNil) if this_round => {
            debug_trace!(state, Line::L45);

            precommit_nil(state)
        }

        // L36/L37
        // NOTE: Only executed the first time, as the votekeeper will only emit this threshold once.
        (Step::Prevote, Input::ProposalAndPolkaCurrent(proposal)) if this_round => {
            debug_trace!(state, Line::L36ValidProposal);

            precommit(state, proposal)
        }

        // L61
        (Step::Prevote, Input::TimeoutPrevote) if this_round => {
            debug_trace!(state, Line::L61);

            precommit_nil(state)
        }

        //
        // From Precommit
        //

        // L36/L42
        // NOTE: Only executed the first time, as the votekeeper will only emit this threshold once.
        (Step::Precommit, Input::ProposalAndPolkaCurrent(proposal)) if this_round => {
            debug_trace!(state, Line::L36ValidProposal);
            set_valid_value(state, &proposal)
        }

        //
        // From Commit. No more state transitions.
        //
        (Step::Commit, _) => Transition::invalid(state),

        //
        // From all (except Commit). Various round guards.
        //

        // L47
        (_, Input::PrecommitAny) if this_round => {
            debug_trace!(state, Line::L48);
            schedule_timeout_precommit(state)
        }

        // L65
        (_, Input::TimeoutPrecommit) if this_round => {
            debug_trace!(state, Line::L67);
            round_skip(state, info.input_round.increment())
        }

        // L55
        (_, Input::SkipRound(round)) if state.round < round => {
            debug_trace!(state, Line::L55);
            round_skip(state, round)
        }

        // L49
        (_, Input::ProposalAndPrecommitValue(proposal)) => {
            let round = state.round;
            debug_trace!(state, Line::L49);
            commit(state, round, proposal)
        }

        // Invalid transition.
        _ => Transition::invalid(state),
    }
}

//---------------------------------------------------------------------
// Propose
//---------------------------------------------------------------------

/// We are the proposer. Propose the valid value if present, otherwise schedule timeout propose
/// and ask for a value.
///
/// Ref: L13-L16, L19
///
/// Transitions from `Step::Unstarted` to `Step::Propose`, therefore called just once per round.
pub fn propose_valid_or_get_value<P>(mut state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    debug_trace!(state, Line::L14);

    match &state.valid {
        Some(round_value) => {
            // L16
            let pol_round = round_value.round;
            let proposal = Output::proposal(
                state.height,
                state.round,
                round_value.value.clone(),
                pol_round,
            );
            debug_trace!(state, Line::L16);
            debug_trace!(state, Line::L19);

            Transition::to(state.with_step(Step::Propose)).with_output(proposal)
        }
        None => {
            // L18
            let output = Output::get_value_and_schedule_timeout(
                state.height,
                state.round,
                TimeoutKind::Propose,
            );
            debug_trace!(state, Line::L18);

            debug_assert!(state.check_timeout(TimeoutKind::Propose));
            Transition::to(state.with_step(Step::Propose)).with_output(output)
        }
    }
}

/// We are the proposer; propose the valid value if it exists,
/// otherwise propose the given value.
///
/// Ref: L13, L17-18
pub fn propose<P>(mut state: State<P>, value: P::Value) -> Transition<P>
where
    P: Proposal,
{
    let proposal = Output::proposal(state.height, state.round, value, Round::Nil);

    debug_trace!(state, Line::L19);
    Transition::to(state.with_step(Step::Propose)).with_output(proposal)
}

//---------------------------------------------------------------------
// Prevote
//---------------------------------------------------------------------

/// Received a complete proposal; prevote the value,
/// unless we are locked on something else.
///
/// Ref: L22 with valid proposal
pub fn prevote<P>(mut state: State<P>, proposal: &P) -> Transition<P>
where
    P: Proposal,
{
    let vr = proposal.pol_round();
    assert_eq!(vr, Round::Nil);
    let proposed = proposal.value().id();
    let value = match &state.locked {
        Some(locked) if locked.value.id() == proposed => {
            // already locked on value
            debug_trace!(state, Line::L24ValidAndLockedValue);
            NilOrVal::Val(proposed)
        }
        Some(_) => {
            // locked on a different value
            debug_trace!(state, Line::L26ValidAndLockedValue);
            NilOrVal::Nil
        }
        None => {
            // not locked, prevote the value
            debug_trace!(state, Line::L24ValidNoLockedRound);
            NilOrVal::Val(proposed)
        }
    };

    let output = Output::prevote(state.height, state.round, value);
    Transition::to(state.with_step(Step::Prevote)).with_output(output)
}

/// Received a proposal for a previously seen value and a polka from a previous round; prevote the value,
/// unless we are locked on a different value at a higher round.
///
/// Ref: L28
pub fn prevote_previous<P>(mut state: State<P>, proposal: &P) -> Transition<P>
where
    P: Proposal,
{
    let vr = proposal.pol_round();
    assert!(vr >= Round::Some(0));
    assert!(vr < proposal.round());

    let proposed = proposal.value().id();
    let value = match &state.locked {
        Some(locked) if locked.round <= vr => {
            // locked on lower or equal round, maybe on different value
            debug_trace!(state, Line::L30ValidLockedRound);
            NilOrVal::Val(proposed)
        }
        Some(locked) if locked.value.id() == proposed => {
            // already locked same value
            debug_trace!(state, Line::L30ValidLockedValue);
            NilOrVal::Val(proposed)
        }
        Some(_) => {
            // we're locked on a different value in a higher round, prevote nil
            debug_trace!(state, Line::L32InvalidValue);
            NilOrVal::Nil
        }
        None => {
            // not locked, prevote the value
            debug_trace!(state, Line::L30ValidNoLockedRound);
            NilOrVal::Val(proposed)
        }
    };

    let output = Output::prevote(state.height, state.round, value);
    Transition::to(state.with_step(Step::Prevote)).with_output(output)
}

/// Received a complete proposal for an empty or invalid value, or timed out; prevote nil.
///
/// Ref: L22/L25, L28/L31, L57
pub fn prevote_nil<P>(state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    let output = Output::prevote(state.height, state.round, NilOrVal::Nil);

    Transition::to(state.with_step(Step::Prevote)).with_output(output)
}

// ---------------------------------------------------------------------
// Precommit
// ---------------------------------------------------------------------

/// Received a polka for a value; precommit the value.
///
/// Ref: L36
///
/// NOTE: Only one of this and set_valid_value should be called once in a round
///       How do we enforce this?
pub fn precommit<P>(state: State<P>, proposal: P) -> Transition<P>
where
    P: Proposal,
{
    if state.step != Step::Prevote {
        return Transition::to(state);
    }

    let value = proposal.value();
    let output = Output::precommit(state.height, state.round, NilOrVal::Val(value.id()));

    let next = state
        .set_locked(value.clone())
        .set_valid(value.clone())
        .with_step(Step::Precommit);

    Transition::to(next).with_output(output)
}

/// Received a polka for nil or timed out of prevote; precommit nil.
///
/// Ref: L44, L61
pub fn precommit_nil<P>(state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    let output = Output::precommit(state.height, state.round, NilOrVal::Nil);

    Transition::to(state.with_step(Step::Precommit)).with_output(output)
}

// ---------------------------------------------------------------------
// Schedule timeouts
// ---------------------------------------------------------------------

/// We're not the proposer; schedule timeout propose.
///
/// Ref: L11, L20
///
/// Transitions from `Step::Unstarted` to `Step::Propose`, therefore called just once per round.
pub fn schedule_timeout_propose<P>(mut state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    debug_trace!(state, Line::L21ProposeTimeoutScheduled);

    debug_assert!(state.check_timeout(TimeoutKind::Propose));
    let timeout = Output::schedule_timeout(state.round, TimeoutKind::Propose);
    Transition::to(state.with_step(Step::Propose)).with_output(timeout)
}

/// We received a polka for any; schedule timeout prevote.
///
/// Ref: L34
///
/// This should only be called once in a round, per the spec.
pub fn schedule_timeout_prevote<P>(mut state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    if state.check_timeout(TimeoutKind::Prevote) {
        let output = Output::schedule_timeout(state.round, TimeoutKind::Prevote);
        Transition::to(state).with_output(output)
    } else {
        Transition::to(state)
    }
}

/// We received +2/3 precommits for any; schedule timeout precommit.
///
/// Ref: L47
///
/// This should only be called once in a round, per the spec.
pub fn schedule_timeout_precommit<P>(mut state: State<P>) -> Transition<P>
where
    P: Proposal,
{
    if state.check_timeout(TimeoutKind::Precommit) {
        let output = Output::schedule_timeout(state.round, TimeoutKind::Precommit);
        Transition::to(state).with_output(output)
    } else {
        Transition::to(state)
    }
}

//---------------------------------------------------------------------
// Set the valid value.
//---------------------------------------------------------------------

/// We received a polka for a value after we already precommitted.
/// Set the valid value and current round.
///
/// Ref: L36/L42
///
/// NOTE: only one of this and precommit should be called once in a round
pub fn set_valid_value<P>(state: State<P>, proposal: &P) -> Transition<P>
where
    P: Proposal,
{
    Transition::to(state.set_valid(proposal.value().clone()))
}

//---------------------------------------------------------------------
// New round or height
//---------------------------------------------------------------------

/// We finished a round (timeout precommit) or received +1/3 votes
/// from a higher round. Move to the higher round.
///
/// Ref: L65
pub fn round_skip<P>(state: State<P>, round: Round) -> Transition<P>
where
    P: Proposal,
{
    let new_state = state.with_round(round).with_step(Step::Unstarted);
    Transition::to(new_state).with_output(Output::NewRound(round))
}

/// We received +2/3 precommits for a value - commit and decide that value!
///
/// Ref: L49
pub fn commit<P>(state: State<P>, round: Round, proposal: P) -> Transition<P>
where
    P: Proposal,
{
    match &state.decision {
        // We already decided the same value
        Some(decision) if decision.value.id() == proposal.value().id() => Transition::to(state),
        // We decided a different value
        Some(_) => Transition::invalid(state),
        // First time we see a decision
        None => {
            let new_state = state
                .set_decision(proposal.round(), proposal.value().clone())
                .with_step(Step::Commit);
            let output = Output::decision(round, proposal.clone());
            Transition::to(new_state).with_output(output)
        }
    }
}
