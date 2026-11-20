//! Evidence of equivocation.

use alloc::collections::btree_map::BTreeMap;
use alloc::{vec, vec::Vec};

use derive_where::derive_where;

use malachitebft_core_types::{SignedMessage, Vote};

/// Maximum number of distinct equivocation pairs retained per validator.
///
/// A few pairs suffice to prove equivocation; further pairs from the same
/// validator are dropped so per-validator storage remains bounded.
pub const MAX_EVIDENCE_PER_VALIDATOR: usize = 3;

/// Two signed votes retained as evidence of equivocation.
pub type VotePair<V> = (
    SignedMessage<V, <V as Vote>::Signature>,
    SignedMessage<V, <V as Vote>::Signature>,
);

/// Keeps track of evidence of equivocation.
#[derive_where(Clone, Debug, Default)]
pub struct EvidenceMap<V>
where
    V: Vote,
{
    map: BTreeMap<V::Address, Vec<VotePair<V>>>,
}

impl<V> EvidenceMap<V>
where
    V: Vote,
{
    /// Create a new `EvidenceMap` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return whether or not there is any evidence of equivocation.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Return the evidence of equivocation for a given address, if any.
    pub fn get(&self, address: &V::Address) -> Option<&Vec<VotePair<V>>> {
        self.map.get(address)
    }

    /// Whether this validator already has [`MAX_EVIDENCE_PER_VALIDATOR`] pairs.
    /// A further conflicting vote cannot add evidence and should not be
    /// verified or written to the WAL.
    pub fn is_full(&self, address: &V::Address) -> bool {
        self.get(address)
            .is_some_and(|pairs| pairs.len() >= MAX_EVIDENCE_PER_VALIDATOR)
    }

    /// Add evidence of equivocating votes, ie. two votes submitted by the same validator,
    /// but with different values but for the same height and round.
    /// If evidence for the same pair of votes already exists, it will not be added again.
    /// Once a validator has [`MAX_EVIDENCE_PER_VALIDATOR`] recorded pairs, further pairs
    /// from that validator are dropped.
    ///
    /// # Precondition
    /// - Both votes must be from the same validator (debug-asserted).
    pub fn add(
        &mut self,
        existing: SignedMessage<V, V::Signature>,
        conflicting: SignedMessage<V, V::Signature>,
    ) {
        debug_assert_eq!(
            existing.validator_address(),
            conflicting.validator_address()
        );

        if let Some(evidence) = self.map.get_mut(conflicting.validator_address()) {
            // Check if this evidence already exists (in either order)
            let already_exists = evidence.iter().any(|(e, c)| {
                (e == &existing && c == &conflicting) || (e == &conflicting && c == &existing)
            });
            if !already_exists && evidence.len() < MAX_EVIDENCE_PER_VALIDATOR {
                evidence.push((existing, conflicting));
            }
        } else {
            self.map.insert(
                conflicting.validator_address().clone(),
                vec![(existing, conflicting)],
            );
        }
    }

    /// Return the number of addresses with recorded vote equivocations.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Iterate over all addresses with recorded vote equivocations.
    pub fn iter(&self) -> alloc::collections::btree_map::Iter<'_, V::Address, Vec<VotePair<V>>> {
        self.map.iter()
    }
}

impl<'a, V> IntoIterator for &'a EvidenceMap<V>
where
    V: Vote,
{
    type Item = (&'a V::Address, &'a Vec<VotePair<V>>);
    type IntoIter = alloc::collections::btree_map::Iter<'a, V::Address, Vec<VotePair<V>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

impl<V> IntoIterator for EvidenceMap<V>
where
    V: Vote,
{
    type Item = (V::Address, Vec<VotePair<V>>);
    type IntoIter = alloc::collections::btree_map::IntoIter<V::Address, Vec<VotePair<V>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter()
    }
}
