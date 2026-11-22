use std::collections::BTreeMap;

use itf::de::{As, Integer, Same};
use malachitebft_core_driver::{Driver, Input, Output, ThresholdParams};
use malachitebft_core_types::{
    ExtendedCommitCertificate, NilOrVal, PolkaCertificate, Round, SignedProposal, SignedVote,
    Timeout, Validity,
};
use malachitebft_test::{
    Address, Height, PrivateKey, Proposal, Signature, TestContext, Validator, ValidatorSet, Vote,
};
use rand::{rngs::StdRng, SeedableRng};
use serde::Deserialize;

use crate::consensus::{ConsensusState, Output as ModelOutput};
use crate::tests::consensus::utils::{value_from_string, value_id_from_model};
use crate::types::{
    Proposal as ModelProposal, Timeout as ModelTimeout, Vote as ModelVote, VoteType,
};

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub(super) enum Event {
    #[serde(rename = "NoDInput")]
    Initial,
    #[serde(rename = "StartDInput")]
    Start,
    #[serde(rename = "ProposeValueDInput")]
    ProposeValue(String),
    #[serde(rename = "ProposalDInput")]
    Proposal(ModelProposal),
    #[serde(rename = "VoteDInput")]
    Vote(ModelVote),
    #[serde(rename = "CertificateDInput")]
    Certificate(Vec<ModelVote>),
    #[serde(rename = "TimeoutDInput")]
    Timeout(ModelTimeout),
    #[serde(rename = "PendingDInput")]
    Pending(crate::consensus::Input),
    #[serde(rename = "StepChangeDInput")]
    StepChange,
}

#[derive(Debug, PartialEq, Deserialize)]
pub(super) struct Entry {
    pub decision: ModelProposal,
    pub commit: Vec<ModelVote>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub(super) struct Model {
    pub cs: ConsensusState,
    pub chain: Vec<Entry>,
    #[serde(with = "As::<BTreeMap<Same, Integer>>")]
    valset: BTreeMap<String, u64>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub(super) struct Frame {
    #[serde(with = "As::<Integer>")]
    pub sequence: usize,
    pub event: Event,
    pub model: Model,
    pub effects: Vec<ModelOutput>,
    pub leaders: Vec<String>,
}

pub(super) struct Fixture {
    validators: BTreeMap<String, Validator>,
    leaders: Vec<Address>,
}

impl Fixture {
    pub fn new(model: &Model, schedule: &[String]) -> (Self, Driver<TestContext>) {
        let mut rng = StdRng::seed_from_u64(0x42);
        let validators: BTreeMap<_, _> = model
            .valset
            .iter()
            .map(|(name, weight)| {
                (
                    name.clone(),
                    Validator::new(PrivateKey::generate(&mut rng).public_key(), *weight),
                )
            })
            .collect();
        let leaders = schedule
            .iter()
            .map(|name| validators[name].address)
            .collect();
        let driver = Driver::new(
            TestContext::new(),
            Height::new(model.cs.height.try_into().unwrap()),
            ValidatorSet::new(validators.values().cloned()),
            validators[&model.cs.process].address,
            ThresholdParams::default(),
        );
        (
            Self {
                validators,
                leaders,
            },
            driver,
        )
    }

    pub fn display(&self, value: &impl std::fmt::Debug) -> String {
        let mut text = format!("{value:?}");
        // The role is part of the scenario. The chosen validator can differ
        // between Quint evaluators because the model iterates a set of addresses.
        // Compare concrete addresses first, and label them only when reporting.
        for (name, validator) in &self.validators {
            let label = self
                .leaders
                .iter()
                .position(|a| *a == validator.address)
                .map_or_else(|| name.clone(), |round| format!("proposer[r{round}]"));
            text = text.replace(&format!("{:?}", validator.address), &label);
        }
        text
    }

    pub fn leader(&self, round: Round) -> Address {
        self.leaders[usize::try_from(round.as_i64()).unwrap()]
    }

    pub fn proposal(&self, p: &ModelProposal) -> Proposal {
        Proposal::new(
            Height::new(p.height.try_into().unwrap()),
            Round::from(p.round),
            value_from_string(&p.proposal).unwrap(),
            Round::from(p.valid_round),
            self.validators[&p.src_address].address,
        )
    }

    pub fn vote(&self, v: &ModelVote) -> Vote {
        let height = Height::new(v.height.try_into().unwrap());
        let round = Round::from(v.round);
        let value = value_id_from_model(&v.value_id).map_or(NilOrVal::Nil, NilOrVal::Val);
        let address = self.validators[&v.src_address].address;
        match v.vote_type {
            VoteType::Prevote => Vote::new_prevote(height, round, value, address),
            VoteType::Precommit => Vote::new_precommit(height, round, value, address),
        }
    }

    pub fn input(&self, event: &Event, driver: &Driver<TestContext>) -> Input<TestContext> {
        match event {
            Event::Initial | Event::Pending(_) | Event::StepChange => {
                panic!("internal event at the external input boundary")
            }
            Event::Start => {
                Input::NewRound(driver.height(), Round::new(0), self.leader(Round::new(0)))
            }
            Event::ProposeValue(v) => {
                Input::ProposeValue(driver.round(), value_from_string(v).unwrap())
            }
            Event::Proposal(p) => {
                let validity = if p.proposal == "invalid" {
                    Validity::Invalid
                } else {
                    Validity::Valid
                };
                Input::Proposal(
                    SignedProposal::new(self.proposal(p), Signature::test()),
                    validity,
                )
            }
            Event::Vote(v) => Input::Vote(SignedVote::new(self.vote(v), Signature::test())),
            Event::Timeout(t) => Input::TimeoutElapsed(Timeout::new(driver.round(), t.to_common())),
            Event::Certificate(votes) => {
                let first = votes.first().expect("empty model certificate");
                let height = Height::new(first.height.try_into().unwrap());
                let round = Round::from(first.round);
                let value = value_id_from_model(&first.value_id).expect("nil certificate");
                let signed = votes
                    .iter()
                    .map(|v| SignedVote::new(self.vote(v), Signature::test()))
                    .collect();
                match first.vote_type {
                    VoteType::Prevote => {
                        Input::PolkaCertificate(PolkaCertificate::new(height, round, value, signed))
                    }
                    VoteType::Precommit => Input::CommitCertificate(
                        ExtendedCommitCertificate::from_votes(height, round, value, signed),
                    ),
                }
            }
        }
    }

    /// Expand the model's compound proposer effect into the driver's two ordered outputs.
    pub fn outputs(&self, frame: &Frame) -> Vec<Output<TestContext>> {
        let mut result = Vec::new();
        for effect in &frame.effects {
            match effect {
                ModelOutput::GetValueAndScheduleTimeout(h, r, t) => {
                    let round = Round::from(*r);
                    let timeout = Timeout::new(round, t.to_common());
                    result.push(Output::ScheduleTimeout(timeout));
                    result.push(Output::GetValue(
                        Height::new((*h).try_into().unwrap()),
                        round,
                        timeout,
                    ));
                }
                ModelOutput::Vote(v) => result.push(Output::Vote(self.vote(v))),
                ModelOutput::Proposal(p) => result.push(Output::Propose(self.proposal(p))),
                ModelOutput::Timeout(r, t) => result.push(Output::ScheduleTimeout(Timeout::new(
                    Round::from(*r),
                    t.to_common(),
                ))),
                ModelOutput::Decided(proposal_round, v) => {
                    let decision = &frame
                        .model
                        .chain
                        .last()
                        .expect("model decision missing from chain")
                        .decision;
                    assert_eq!((&decision.round, &decision.proposal), (proposal_round, v));
                    result.push(Output::Decide(
                        Round::from(frame.model.cs.round),
                        self.proposal(decision),
                    ));
                }
                other => panic!("unexpected external driver effect {other:?}"),
            }
        }
        result
    }
}
