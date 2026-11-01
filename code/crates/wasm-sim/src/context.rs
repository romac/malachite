//! A minimal [`Context`] for simulation purposes.
//!
//! Validators are identified by their index in the validator set, and
//! signatures are simulated: a signature is the index of the signer, and
//! verifying it checks that it matches the expected public key (also the index).
//! This keeps the simulation free of cryptography and randomness while still
//! exercising the consensus logic.

use core::fmt;

use malachitebft_core_types::{
    self as types, LinearTimeouts, NilOrVal, Round, SignedExtension, VoteType, VotingPower,
};
use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct SimContext;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Height(pub u64);

impl fmt::Display for Height {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl types::Height for Height {
    const ZERO: Self = Self(0);
    const INITIAL: Self = Self(1);

    fn increment_by(&self, n: u64) -> Self {
        Self(self.0 + n)
    }

    fn decrement_by(&self, n: u64) -> Option<Self> {
        self.0.checked_sub(n).map(Self)
    }

    fn as_u64(&self) -> u64 {
        self.0
    }
}

/// A validator address: its index in the validator set.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Address(pub u32);

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl types::Address for Address {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Value(pub u64);

impl types::Value for Value {
    type Id = u64;

    fn id(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub height: Height,
    pub round: Round,
    pub value: Value,
    pub pol_round: Round,
    pub proposer: Address,
}

impl types::Proposal<SimContext> for Proposal {
    fn height(&self) -> Height {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    fn value(&self) -> &Value {
        &self.value
    }

    fn take_value(self) -> Value {
        self.value
    }

    fn pol_round(&self) -> Round {
        self.pol_round
    }

    fn validator_address(&self) -> &Address {
        &self.proposer
    }
}

/// Unused: the simulation runs in `ProposalOnly` mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalPart;

impl types::ProposalPart<SimContext> for ProposalPart {
    fn is_first(&self) -> bool {
        true
    }

    fn is_last(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    pub typ: VoteType,
    pub height: Height,
    pub round: Round,
    pub value: NilOrVal<u64>,
    pub voter: Address,
    pub extension: Option<SignedExtension<SimContext>>,
}

impl types::Vote<SimContext> for Vote {
    fn height(&self) -> Height {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    fn value(&self) -> &NilOrVal<u64> {
        &self.value
    }

    fn take_value(self) -> NilOrVal<u64> {
        self.value
    }

    fn vote_type(&self) -> VoteType {
        self.typ
    }

    fn validator_address(&self) -> &Address {
        &self.voter
    }

    fn extension(&self) -> Option<&SignedExtension<SimContext>> {
        self.extension.as_ref()
    }

    fn take_extension(&mut self) -> Option<SignedExtension<SimContext>> {
        self.extension.take()
    }

    fn extend(self, extension: SignedExtension<SimContext>) -> Self {
        Self {
            extension: Some(extension),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    pub address: Address,
    pub voting_power: VotingPower,
}

impl types::Validator<SimContext> for Validator {
    fn address(&self) -> &Address {
        &self.address
    }

    fn public_key(&self) -> &Address {
        &self.address
    }

    fn voting_power(&self) -> VotingPower {
        self.voting_power
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorSet {
    pub validators: Vec<Validator>,
}

impl ValidatorSet {
    /// `n` validators with equal voting power.
    pub fn uniform(n: u32) -> Self {
        Self {
            validators: (0..n)
                .map(|i| Validator {
                    address: Address(i),
                    voting_power: 1,
                })
                .collect(),
        }
    }
}

impl types::ValidatorSet<SimContext> for ValidatorSet {
    fn count(&self) -> usize {
        self.validators.len()
    }

    fn total_voting_power(&self) -> VotingPower {
        self.validators.iter().map(|v| v.voting_power).sum()
    }

    fn get_by_address(&self, address: &Address) -> Option<&Validator> {
        self.validators.iter().find(|v| &v.address == address)
    }

    fn get_by_index(&self, index: usize) -> Option<&Validator> {
        self.validators.get(index)
    }
}

/// Simulated signing: signatures, public and private keys are all the validator address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimSigning;

#[derive(Debug)]
pub struct DecodingError;

impl fmt::Display for DecodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid encoding")
    }
}

fn decode_address(bytes: &[u8]) -> Result<Address, DecodingError> {
    let bytes = <[u8; 4]>::try_from(bytes).map_err(|_| DecodingError)?;
    Ok(Address(u32::from_be_bytes(bytes)))
}

impl types::SigningScheme for SimSigning {
    type DecodingError = DecodingError;
    type Signature = Address;
    type PublicKey = Address;
    type PrivateKey = Address;

    fn decode_signature(bytes: &[u8]) -> Result<Address, DecodingError> {
        decode_address(bytes)
    }

    fn encode_signature(signature: &Address) -> Vec<u8> {
        signature.0.to_be_bytes().to_vec()
    }

    fn decode_public_key(bytes: &[u8]) -> Result<Address, DecodingError> {
        decode_address(bytes)
    }

    fn encode_public_key(public_key: &Address) -> Vec<u8> {
        public_key.0.to_be_bytes().to_vec()
    }
}

impl types::Context for SimContext {
    type Address = Address;
    type Height = Height;
    type ProposalPart = ProposalPart;
    type Proposal = Proposal;
    type Validator = Validator;
    type ValidatorSet = ValidatorSet;
    type Timeouts = LinearTimeouts;
    type Value = Value;
    type Vote = Vote;
    type Extension = ();
    type SigningScheme = SimSigning;

    /// Round-robin proposer selection.
    fn select_proposer<'a>(
        &self,
        validator_set: &'a ValidatorSet,
        height: Height,
        round: Round,
    ) -> &'a Validator {
        let round = round.as_u32().expect("round must be defined") as u64;
        let index = (height.0.saturating_sub(1) + round) % validator_set.validators.len() as u64;
        &validator_set.validators[index as usize]
    }

    fn new_proposal(
        &self,
        height: Height,
        round: Round,
        value: Value,
        pol_round: Round,
        address: Address,
    ) -> Proposal {
        Proposal {
            height,
            round,
            value,
            pol_round,
            proposer: address,
        }
    }

    fn new_prevote(
        &self,
        height: Height,
        round: Round,
        value_id: NilOrVal<u64>,
        address: Address,
    ) -> Vote {
        Vote {
            typ: VoteType::Prevote,
            height,
            round,
            value: value_id,
            voter: address,
            extension: None,
        }
    }

    fn new_precommit(
        &self,
        height: Height,
        round: Round,
        value_id: NilOrVal<u64>,
        address: Address,
    ) -> Vote {
        Vote {
            typ: VoteType::Precommit,
            height,
            round,
            value: value_id,
            voter: address,
            extension: None,
        }
    }
}
