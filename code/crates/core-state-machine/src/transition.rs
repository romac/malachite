//! A transition taken by the state machine after processing an input.

use malachitebft_core_types::Proposal;

use crate::output::Output;
use crate::state::State;

/// A transition taken by the state machine after processing an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition<P>
where
    P: Proposal,
{
    /// The next state to transition to.
    pub next_state: State<P>,
    /// The output to emit.
    pub output: Option<Output<P>>,
    /// Whether the transition is valid or not.
    pub valid: bool,
}

impl<P> Transition<P>
where
    P: Proposal,
{
    /// Build a new valid transition to the given next state.
    pub fn to(next_state: State<P>) -> Self {
        Self {
            next_state,
            output: None,
            valid: true,
        }
    }

    /// Build a new invalid transition to the given next state.
    pub fn invalid(next_state: State<P>) -> Self {
        Self {
            next_state,
            output: None,
            valid: false,
        }
    }

    /// Set the output of the transition.
    pub fn with_output(mut self, output: Output<P>) -> Self {
        self.output = Some(output);
        self
    }
}
