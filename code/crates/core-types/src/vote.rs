use core::fmt::Debug;

use crate::{Address, Height, NilOrVal, Round};

/// A type of vote.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "borsh",
    derive(::borsh::BorshSerialize, ::borsh::BorshDeserialize)
)]
pub enum VoteType {
    /// Votes for values which validators observe are valid for a given round.
    Prevote,

    /// Votes to commit to a particular value for a given round.
    Precommit,
}

/// Defines the requirements for a vote.
///
/// Votes are signed messages from validators for a particular value which
/// include information about the validator signing it.
///
/// # Equality and write-ahead log replay
///
/// `Eq` must distinguish votes only by content that consensus re-derives deterministically.
/// Replaying the write-ahead log re-derives every vote this node cast before a crash.
/// The re-derived vote and the recorded one are compared (`Eq`) and they must be equal.
/// Otherwise, it is considered a non-determinism bug.
///
/// The vote extension is exempt: the application supplies it and it need not be deterministic.
/// It is excluded from the comparison, and must also be excluded from the vote's signing preimage.
/// This way, a recorded signature stays valid for a vote that differs only in its extension.
pub trait Vote
where
    Self: Clone + Debug + Eq + Send + Sync + 'static,
{
    /// Block height.
    type Height: Height;
    /// Validator address.
    type Address: Address;
    /// Identifier of the voted value.
    type ValueId: Clone + Eq + Ord;
    /// Signed application extension carried by a precommit.
    type SignedExtension;

    /// The height for which the vote is for.
    fn height(&self) -> Self::Height;

    /// The round for which the vote is for.
    fn round(&self) -> Round;

    /// Get a reference to the value being voted for.
    fn value(&self) -> &NilOrVal<Self::ValueId>;

    /// Take ownership of the value being voted for.
    fn take_value(self) -> NilOrVal<Self::ValueId>;

    /// The type of vote.
    fn vote_type(&self) -> VoteType;

    /// Address of the validator who issued this vote
    fn validator_address(&self) -> &Self::Address;

    /// Votes extensions
    fn extension(&self) -> Option<&Self::SignedExtension>;

    /// Return an owned reference to this vote's extensions,
    /// removing them from the vote in the process.
    fn take_extension(&mut self) -> Option<Self::SignedExtension>;

    /// Extend this vote with an extension, overriding any existing extension.
    fn extend(self, extension: Self::SignedExtension) -> Self;
}
