use core::fmt::Debug;

/// Defines the requirements for a proposal part type.
pub trait ProposalPart
where
    Self: Clone + Debug + Eq + Send + Sync + 'static,
{
    /// Is this the first proposal part?
    fn is_first(&self) -> bool;

    /// Is this the last proposal part?
    fn is_last(&self) -> bool;
}
