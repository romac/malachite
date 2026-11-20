//! Outputs of the round state machine.

use derive_where::derive_where;

use malachitebft_core_types::{NilOrVal, Proposal, Round, Timeout, TimeoutKind, Value, VoteType};

/// Fields of a vote requested by the round transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CastVote<H, I> {
    /// Height at which to vote.
    pub height: H,
    /// Round at which to vote.
    pub round: Round,
    /// Prevote or precommit.
    pub kind: VoteType,
    /// Value identifier, or nil.
    pub value: NilOrVal<I>,
}

/// Fields of a proposal requested by the round transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposeValue<H, V> {
    /// Height of the proposal.
    pub height: H,
    /// Round of the proposal.
    pub round: Round,
    /// Value to propose.
    pub value: V,
    /// Round of the proof of lock, or nil for a fresh proposal.
    pub pol_round: Round,
}

/// Output of the round state machine.
#[derive_where(Clone, Debug, PartialEq, Eq)]
pub enum Output<P>
where
    P: Proposal,
{
    /// Move to the new round.
    NewRound(Round),

    /// Broadcast the proposal.
    Proposal(ProposeValue<P::Height, P::Value>),

    /// Broadcast the vote.
    Vote(CastVote<P::Height, <P::Value as Value>::Id>),

    /// Schedule the timeout.
    ScheduleTimeout(Timeout),

    /// Ask for a value at the given height, round and to schedule a timeout.
    /// The timeout tells the proposal builder how long it has to build a value.
    GetValueAndScheduleTimeout(P::Height, Round, Timeout),

    /// Decide the value.
    Decision(Round, P),
}

impl<P: Proposal> Output<P> {
    /// Build a `Proposal` output.
    pub fn proposal(height: P::Height, round: Round, value: P::Value, pol_round: Round) -> Self {
        Output::Proposal(ProposeValue {
            height,
            round,
            value,
            pol_round,
        })
    }

    /// Build a `Vote` output for a prevote.
    pub fn prevote(
        height: P::Height,
        round: Round,
        value_id: NilOrVal<<P::Value as Value>::Id>,
    ) -> Self {
        Output::Vote(CastVote {
            height,
            round,
            value: value_id,
            kind: VoteType::Prevote,
        })
    }

    /// Build a `Vote` output for a precommit.
    pub fn precommit(
        height: P::Height,
        round: Round,
        value_id: NilOrVal<<P::Value as Value>::Id>,
    ) -> Self {
        Output::Vote(CastVote {
            height,
            round,
            value: value_id,
            kind: VoteType::Precommit,
        })
    }

    /// Build a `ScheduleTimeout` output.
    pub fn schedule_timeout(round: Round, step: TimeoutKind) -> Self {
        Output::ScheduleTimeout(Timeout { round, kind: step })
    }

    /// Build a `GetValue` output.
    pub fn get_value_and_schedule_timeout(
        height: P::Height,
        round: Round,
        step: TimeoutKind,
    ) -> Self {
        Output::GetValueAndScheduleTimeout(height, round, Timeout { round, kind: step })
    }

    /// Build a `Decision` output.
    pub fn decision(round: Round, proposal: P) -> Self {
        Output::Decision(round, proposal)
    }
}
