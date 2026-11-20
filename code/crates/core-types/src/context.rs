use crate::{
    Address, Extension, Height, NilOrVal, Proposal, ProposalPart, Round, SigningScheme, Timeouts,
    Validator, ValidatorSet, Value, ValueId, Vote,
};

/// The application types used together by this consensus instance.
pub trait ConsensusTypes
where
    Self: Sized + Clone + Send + Sync + 'static,
{
    /// The type of address of a validator.
    type Address: Address;

    /// The type of the height of a block.
    type Height: Height;

    /// The type of proposal part
    type ProposalPart: ProposalPart;

    /// The interface provided by the proposal type.
    type Proposal: Proposal<Height = Self::Height, Value = Self::Value, Address = Self::Address>;

    /// The interface provided by the validator type.
    type Validator: Validator<Address = Self::Address, PublicKey = crate::PublicKey<Self>>;

    /// The interface provided by the validator set type.
    type ValidatorSet: ValidatorSet<Validator = Self::Validator>;

    /// The interface provided by the timeouts type.
    type Timeouts: Timeouts;

    /// The `Value` type denotes the value `v` carried by the `Proposal`
    /// consensus message that is gossiped to other nodes by the proposer.
    type Value: Value;

    /// The type of votes that can be cast.
    type Vote: Vote<
        Height = Self::Height,
        Address = Self::Address,
        ValueId = ValueId<Self>,
        SignedExtension = crate::SignedExtension<Self>,
    >;

    /// The type of vote extensions.
    type Extension: Extension;

    /// The signing scheme used to sign consensus messages.
    type SigningScheme: SigningScheme;
}

/// Selects the proposer deterministically from the validators at a height.
pub trait ProposerSelector<S: ConsensusTypes> {
    /// Select a proposer in the validator set for the given height and round.
    fn select_proposer<'a>(
        &self,
        validator_set: &'a S::ValidatorSet,
        height: S::Height,
        round: Round,
    ) -> &'a S::Validator;
}

/// Constructs application messages from the fields chosen by consensus.
pub trait MessageBuilder<S: ConsensusTypes> {
    /// Build a new proposal for the given value at the given height, round and POL round.
    fn new_proposal(
        &self,
        height: S::Height,
        round: Round,
        value: S::Value,
        pol_round: Round,
        address: S::Address,
    ) -> S::Proposal;

    /// Build a new prevote vote by the validator with the given address,
    /// for the value identified by the given value id, at the given round.
    fn new_prevote(
        &self,
        height: S::Height,
        round: Round,
        value_id: NilOrVal<ValueId<S>>,
        address: S::Address,
    ) -> S::Vote;

    /// Build a new precommit vote by the validator with the given address,
    /// for the value identified by the given value id, at the given round.
    fn new_precommit(
        &self,
        height: S::Height,
        round: Round,
        value_id: NilOrVal<ValueId<S>>,
        address: S::Address,
    ) -> S::Vote;
}

/// Convenience bound for an application that provides both consensus operations.
pub trait Context: ConsensusTypes + MessageBuilder<Self> + ProposerSelector<Self> {}

impl<S> Context for S where S: ConsensusTypes + MessageBuilder<S> + ProposerSelector<S> {}
