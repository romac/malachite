use malachitebft_core_consensus::{
    process, ConsensusMsg, Effect, Error, Input, Params, ProposedValue, Resumable, Resume,
    SignedConsensusMsg, State,
};
use malachitebft_core_types::{
    NilOrVal, Round, SignedProposal, SignedVote, Validity, ValueOrigin, ValuePayload,
};
use malachitebft_metrics::Metrics;
use malachitebft_test::utils::validators::make_validators;
use malachitebft_test::{Height, PrivateKey, Proposal, TestContext, ValidatorSet, Value, Vote};
use serde::Deserialize;

use super::{check_differences, replay_cases};
use crate::consensus::ConsensusState;
use crate::tests::consensus::utils::value_id_from_model;
use crate::types::Vote as ModelVote;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag")]
enum Event {
    Start,
    Envelope,
    HostAccepted,
}

#[derive(PartialEq, Eq, Deserialize)]
struct Frame {
    event: Event,
    payload: String,
    state: ConsensusState,
    votes: Vec<ModelVote>,
}

#[derive(Default)]
struct VoteEffects {
    signed: Vec<SignedVote<TestContext>>,
    published: Vec<SignedVote<TestContext>>,
}

impl VoteEffects {
    fn handle(
        &mut self,
        effect: Effect<TestContext>,
        key: &PrivateKey,
    ) -> Result<Resume<TestContext>, ()> {
        Ok(match effect {
            Effect::VerifySignature(msg, pk, r) => {
                let bytes = match &msg.message {
                    ConsensusMsg::Proposal(p) => p.to_sign_bytes(),
                    ConsensusMsg::Vote(v) => v.to_sign_bytes(),
                };
                r.resume_with(pk.verify(&bytes, &msg.signature).is_ok())
            }
            Effect::SignVote(v, r) => {
                let signature = key.sign(&v.to_sign_bytes());
                let vote = SignedVote::new(v, signature);
                self.signed.push(vote.clone());
                r.resume_with(vote)
            }
            Effect::PublishConsensusMsg(SignedConsensusMsg::Vote(v), r) => {
                self.published.push(v);
                r.resume_with(())
            }
            Effect::ExtendVote(_, _, _, _, r) => r.resume_with(None),
            _ => Resume::Continue,
        })
    }
}

#[test]
fn proposal_and_host_verdict_order() {
    let mut differences = Vec::new();
    replay_cases(
        "consensus/quint/tests/consensus/hostValidityReplayTest.qnt",
        |case, trace: &itf::Trace<Frame>| {
            let keys = make_validators([1, 1, 1, 1]);
            let validators = ValidatorSet::new(keys.iter().map(|(v, _)| v.clone()));
            let ctx = TestContext::new();
            let h = Height::new(1);
            let r = Round::new(0);
            let proposer = ctx.select_proposer(&validators, h, r).address;
            let (_, proposer_key) = keys.iter().find(|(v, _)| v.address == proposer).unwrap();
            let (local, local_key) = keys.iter().find(|(v, _)| v.address != proposer).unwrap();
            let value = Value::new(1);
            let proposal = Proposal::new(h, r, value.clone(), Round::Nil, proposer);
            let signature = proposer_key.sign(&proposal.to_sign_bytes());
            let signed = SignedProposal::new(proposal, signature);
            let payload = match trace.states[0].value.payload.as_str() {
                "ProposalOnly" => ValuePayload::ProposalOnly,
                "ProposalAndParts" => ValuePayload::ProposalAndParts,
                mode => panic!("unknown payload mode {mode}"),
            };
            let params = Params {
                address: local.address,
                threshold_params: Default::default(),
                value_payload: payload,
                enabled: true,
            };
            let mut core = State::new(ctx, h, validators.clone(), params, 1000, 1000);
            let metrics = Metrics::new();
            for (index, frame) in trace.states.iter().map(|s| &s.value).enumerate() {
                if index > 0 && frame == &trace.states[index - 1].value {
                    continue;
                }
                let input = match frame.event {
                    Event::Start => {
                        Input::StartHeight(h, validators.clone(), false, None, Default::default())
                    }
                    Event::Envelope => Input::Proposal(signed.clone()),
                    Event::HostAccepted => Input::ProposedValue(
                        ProposedValue {
                            height: h,
                            round: r,
                            valid_round: Round::Nil,
                            proposer,
                            value: value.clone(),
                            validity: Validity::Valid,
                        },
                        ValueOrigin::Consensus,
                    ),
                };
                let mut effects = VoteEffects::default();
                let result: Result<(), Error<TestContext>> = process!(input: input, state: &mut core, metrics: &metrics, with: e => effects.handle(e, local_key));
                result.unwrap();
                assert_eq!(
                    effects.signed, effects.published,
                    "{case} {:?}: signed and published votes",
                    frame.event
                );
                assert_eq!(core.height().as_u64() as i64, frame.state.height);
                assert_eq!(core.round().as_i64(), frame.state.round);
                let expected: Vec<_> = frame
                    .votes
                    .iter()
                    .map(|v| {
                        assert_eq!(v.src_address, "local");
                        Vote {
                            typ: v.vote_type.to_common(),
                            height: Height::new(v.height as u64),
                            round: Round::from(v.round),
                            value: value_id_from_model(&v.value_id)
                                .map_or(NilOrVal::Nil, NilOrVal::Val),
                            validator_address: local.address,
                            extension: None,
                        }
                    })
                    .collect();
                let actual: Vec<_> = effects.published.into_iter().map(|v| v.message).collect();
                let expected_step = frame.state.step.to_round_step().unwrap();
                let actual_step = core.driver.step();
                if expected != actual || expected_step != actual_step {
                    // Check the full unexpected message, including sender, value ID, height and round.
                    assert_eq!(
                        actual,
                        [Vote::new_prevote(
                            h,
                            r,
                            NilOrVal::Val(value.id()),
                            local.address
                        )]
                    );
                    differences.push(format!("{case} {:?}: Quint {expected_step:?}, {} vote(s) / Rust {actual_step:?}, {} vote(s)", frame.event, expected.len(), actual.len()));
                }
            }
        },
    );
    check_differences(
        "proposalOnlyPendingHostTest Envelope: Quint Propose, 0 vote(s) / Rust Prevote, 1 vote(s)",
        &differences,
    );
}
