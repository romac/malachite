use core::fmt::{Debug, Display};

/// Voting power held by a validator.
///
/// TODO: Introduce newtype
pub type VotingPower = u64;

/// Defines the requirements for an address.
pub trait Address
where
    Self: Clone + Debug + Display + Eq + Ord + Send + Sync,
{
}

/// Defines the requirements for a validator.
pub trait Validator
where
    Self: Clone + Debug + Eq + Send + Sync,
{
    /// Validator address.
    type Address: Address;
    /// Public verification key.
    type PublicKey;

    /// The address of the validator, typically derived from its public key.
    fn address(&self) -> &Self::Address;

    /// The public key of the validator, used to verify signatures.
    fn public_key(&self) -> &Self::PublicKey;

    /// The voting power held by the validaror.
    fn voting_power(&self) -> VotingPower;
}

/// Defines the requirements for a validator set.
///
/// A validator set is a collection of validators.
///
/// # Important
/// The validators must be unique and sorted in a deterministic order.
///
/// Such an ordering can be defined as in CometBFT:
/// - first by validator power (descending)
/// - then lexicographically by address (ascending)
pub trait ValidatorSet
where
    Self: Clone + Debug + Eq + Send + Sync,
{
    /// Element type in deterministic order.
    type Validator: Validator;

    /// The number of validators in the set.
    fn count(&self) -> usize;

    /// Return an iterator over the validators in the set.
    fn iter(&self) -> impl Iterator<Item = &Self::Validator> {
        (0..self.count()).filter_map(|i| self.get_by_index(i))
    }

    /// The total voting power of the validator set.
    fn total_voting_power(&self) -> VotingPower;

    /// Get the validator with the given address.
    fn get_by_address(
        &self,
        address: &<Self::Validator as Validator>::Address,
    ) -> Option<&Self::Validator>;

    /// Get the validator at the given index.
    fn get_by_index(&self, index: usize) -> Option<&Self::Validator>;
}
