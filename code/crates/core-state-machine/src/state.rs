//! The state maintained by the round state machine

use derivative::Derivative;

use crate::input::Input;
use crate::state_machine::Info;
use crate::transition::Transition;

#[cfg(feature = "debug")]
use crate::traces::*;

use malachitebft_core_types::{Context, Height, Round, TimeoutKind};

/// A value and its associated round
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundValue<Value> {
    /// The value
    pub value: Value,
    /// The round
    pub round: Round,
}

impl<Value> RoundValue<Value> {
    /// Create a new `RoundValue` instance.
    pub fn new(value: Value, round: Round) -> Self {
        Self { value, round }
    }
}

/// The step of consensus in this round
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// The round has not started yet
    Unstarted,

    /// Propose step.
    /// Either we are the proposer or we are waiting for a proposal.
    Propose,

    /// We are at the prevote step.
    Prevote,

    /// We are at the precommit step.
    Precommit,

    /// We have committed and decided on a value
    Commit,
}

/// The state of the consensus state machine
#[derive(Derivative)]
#[derivative(Clone, Debug, PartialEq, Eq)]
pub struct State<Ctx>
where
    Ctx: Context,
{
    /// The height of the consensus
    pub height: Ctx::Height,

    /// The round we are at within a height
    pub round: Round,

    /// The step we are at within a round
    pub step: Step,

    /// The value we are locked on, ie. we have received a polka for before we precommitted
    pub locked: Option<RoundValue<Ctx::Value>>,

    /// The value for which we saw a polka
    pub valid: Option<RoundValue<Ctx::Value>>,

    /// The value we have decided on, None if no decision has been made yet.
    /// The decision round is the round of the proposal that we decided on.
    /// It may be different, lower or higher, than the state machine round.
    pub decision: Option<RoundValue<Ctx::Value>>,

    /// Timeouts already scheduled for the current round.
    /// Intended to avoid scheduling the same timeout multiple times.
    #[derivative(PartialEq = "ignore")]
    pub scheduled_timeouts: [bool; 3],

    /// Buffer with traces of tendermint algorithm lines,
    #[cfg(feature = "debug")]
    #[derive_where(skip)]
    pub traces: alloc::vec::Vec<Trace<Ctx>>,
}

impl<Ctx> State<Ctx>
where
    Ctx: Context,
{
    /// Create a new `State` instance at the given height and round.
    pub fn new(height: Ctx::Height, round: Round) -> Self {
        Self {
            height,
            round,
            step: Step::Unstarted,
            locked: None,
            valid: None,
            decision: None,
            scheduled_timeouts: [false; 3],
            #[cfg(feature = "debug")]
            traces: alloc::vec::Vec::default(),
        }
    }

    /// Set the round.
    pub fn with_round(self, round: Round) -> Self {
        Self { round, ..self }
    }

    /// Set the step.
    pub fn with_step(self, step: Step) -> Self {
        Self { step, ..self }
    }

    /// Check it a timeout was already scheduled.
    ///
    /// If so, returns `true` and no change is made to the state.
    /// Otherwise, returns `false` and set scheduled_timeouts[timeout] to `true`.
    pub fn check_timeout(&mut self, timeout: TimeoutKind) -> bool {
        let index = timeout.index();
        if self.scheduled_timeouts[index] {
            true
        } else {
            self.scheduled_timeouts[index] = true;
            false
        }
    }

    /// Set the locked value.
    pub fn set_locked(self, value: Ctx::Value) -> Self {
        Self {
            locked: Some(RoundValue::new(value, self.round)),
            ..self
        }
    }

    /// Set the valid value.
    pub fn set_valid(self, value: Ctx::Value) -> Self {
        Self {
            valid: Some(RoundValue::new(value, self.round)),
            ..self
        }
    }

    /// Set the value we have decided on.
    pub fn set_decision(self, proposal_round: Round, value: Ctx::Value) -> Self {
        Self {
            decision: Some(RoundValue::new(value, proposal_round)),
            ..self
        }
    }

    /// Apply the given input to the current state, triggering a transition.
    pub fn apply(self, ctx: &Ctx, data: &Info<Ctx>, input: Input<Ctx>) -> Transition<Ctx> {
        crate::state_machine::apply(ctx, self, data, input)
    }

    /// Return the traces logged during execution.
    #[cfg(feature = "debug")]
    pub fn add_trace(&mut self, line: Line) {
        self.traces.push(Trace::new(self.height, self.round, line));
    }

    /// Return the traces logged during execution.
    #[cfg(feature = "debug")]
    pub fn get_traces(&self) -> &[Trace<Ctx>] {
        &self.traces
    }
}

impl<Ctx> Default for State<Ctx>
where
    Ctx: Context,
{
    fn default() -> Self {
        Self::new(Ctx::Height::ZERO, Round::Nil)
    }
}
