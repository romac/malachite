//! Common data types and abstractions for the consensus engine.

#![no_std]
#![forbid(unsafe_code)]
#![deny(trivial_casts, trivial_numeric_casts)]
#![warn(
    missing_docs,
    rustdoc::broken_intra_doc_links,
    rustdoc::private_intra_doc_links,
    variant_size_differences
)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::panic))]

extern crate alloc;

mod certificate;
mod context;
mod error;
mod height;
mod height_params;
mod proposal;
mod proposal_part;
mod round;
mod ser;
mod signed_message;
mod signing;
mod threshold;
mod timeout;
mod timeouts;
mod validator_proof;
mod validator_set;
mod value;
mod vote;
mod vote_extension;

/// Utility functions and types.
pub mod utils;

/// Type alias to make it easier to refer the `ValueId` type.
pub type ValueId<Ctx> = <<Ctx as ConsensusTypes>::Value as Value>::Id;

/// Type alias to make it easier to refer the `PublicKey` type.
pub type PublicKey<Ctx> = <<Ctx as ConsensusTypes>::SigningScheme as SigningScheme>::PublicKey;

/// Type alias to make it easier to refer the `PrivateKey` type.
pub type PrivateKey<Ctx> = <<Ctx as ConsensusTypes>::SigningScheme as SigningScheme>::PrivateKey;

/// Type alias to make it easier to refer the `Signature` type.
pub type Signature<Ctx> = <<Ctx as ConsensusTypes>::SigningScheme as SigningScheme>::Signature;

/// A signed vote.
pub type SignedVote<Ctx> = SignedMessage<<Ctx as ConsensusTypes>::Vote, Signature<Ctx>>;

/// A double signed vote (e.g., conflicting votes).
pub type DoubleVote<Ctx> = (SignedVote<Ctx>, SignedVote<Ctx>);

/// A signed proposal
pub type SignedProposal<Ctx> = SignedMessage<<Ctx as ConsensusTypes>::Proposal, Signature<Ctx>>;

/// Double signed proposal (e.g., conflicting proposals).
pub type DoubleProposal<Ctx> = (SignedProposal<Ctx>, SignedProposal<Ctx>);

/// A signed vote extension
pub type SignedExtension<Ctx> = SignedMessage<<Ctx as ConsensusTypes>::Extension, Signature<Ctx>>;

pub use certificate::{
    CertificateError, CommitCertificate, CommitSignature, EnterRoundCertificate,
    ExtendedCommitCertificate, ExtendedCommitSignature, PolkaCertificate, PolkaSignature,
    RoundCertificate, RoundCertificateType, RoundSignature, ValueResponse,
};
pub use context::{ConsensusTypes, Context, MessageBuilder, ProposerSelector};
pub use error::BoxError;
pub use height::Height;
pub use height_params::HeightParams;
pub use proposal::{Proposal, Validity};
pub use proposal_part::ProposalPart;
pub use round::Round;
pub use signed_message::SignedMessage;
pub use signing::SigningScheme;
pub use threshold::{Threshold, ThresholdParam, ThresholdParams};
pub use timeout::{Timeout, TimeoutKind};
pub use timeouts::{LinearTimeouts, Timeouts};
pub use validator_proof::ValidatorProof;
pub use validator_set::{Address, Validator, ValidatorSet, VotingPower};
pub use value::{NilOrVal, Value, ValueOrigin, ValuePayload};
pub use vote::{Vote, VoteType};
pub use vote_extension::{Extension, VoteExtensionPolicy, VoteExtensionScope, VoteExtensions};
