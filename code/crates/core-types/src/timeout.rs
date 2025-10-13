use core::fmt;

use derive_where::derive_where;

use crate::{Context, Round};

/// The timeout type. There may be multiple timeouts running in a given step.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TimeoutKind {
    /// Timeout for the propose step.
    Propose,

    /// Timeout for the prevote step.
    Prevote,

    /// Timeout for the precommit step.
    Precommit,

    /// Timeout to rebroadcast the round synchronization messages
    Rebroadcast,
}

/// A timeout for a round step.
#[derive_where(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timeout<Ctx: Context> {
    /// The timeout kind.
    pub kind: TimeoutKind,

    /// The height for which the timeout is for.
    pub height: Ctx::Height,

    /// The round for which the timeout is for.
    pub round: Round,
}

impl<Ctx: Context> Timeout<Ctx> {
    /// Create a new timeout for the given round and step.
    pub const fn new(height: Ctx::Height, round: Round, kind: TimeoutKind) -> Self {
        Self {
            height,
            round,
            kind,
        }
    }

    /// Create a new timeout for the propose step of the given round.
    pub const fn propose(height: Ctx::Height, round: Round) -> Self {
        Self::new(height, round, TimeoutKind::Propose)
    }

    /// Create a new timeout for the prevote step of the given round.
    pub const fn prevote(height: Ctx::Height, round: Round) -> Self {
        Self::new(height, round, TimeoutKind::Prevote)
    }

    /// Create a new timeout for the precommit step of the given round.
    pub const fn precommit(height: Ctx::Height, round: Round) -> Self {
        Self::new(height, round, TimeoutKind::Precommit)
    }

    /// Create a new timeout for rebroadcasting the round synchronization messages.
    pub const fn rebroadcast(height: Ctx::Height, round: Round) -> Self {
        Self::new(height, round, TimeoutKind::Rebroadcast)
    }
}

impl<Ctx: Context> fmt::Display for Timeout<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}Timeout(height: {}, round: {})",
            self.kind, self.height, self.round
        )
    }
}
