use std::collections::{BTreeMap, BTreeSet, HashMap};

use malachitebft_core_types::{Round, SignedVote};
use malachitebft_core_votekeeper::keeper::{Output, VoteKeeper};
use malachitebft_core_votekeeper::ThresholdParams;
use malachitebft_test::{
    Address, Height, PrivateKey, Signature, TestContext, Validator, ValidatorSet, ValueId, Vote,
};

use rand::rngs::StdRng;

use crate::types::VoteType;
use crate::votekeeper::VoteKeeperOutput::*;
use crate::votekeeper::{State, VoteKeeperOutput, WeightedVote};

use super::utils::{check_votes, value_from_model, value_id};

pub(super) struct VoteKeeperRunner {
    addresses: HashMap<String, Address>,
}

impl VoteKeeperRunner {
    pub(super) fn replay(trace: &itf::Trace<State>, mut rng: StdRng) -> Vec<String> {
        let first = &trace.states[0].value;
        let by_name: BTreeMap<_, _> = first.bookkeeper.validator_set.iter().collect();
        let mut addresses = HashMap::new();
        let validators = ValidatorSet::new(by_name.into_iter().map(|(name, weight)| {
            let key = PrivateKey::generate(&mut rng);
            let validator = Validator::new(key.public_key(), u64::try_from(*weight).unwrap());
            addresses.insert(name.clone(), validator.address);
            validator
        }));
        let fixture = Self { addresses };
        let mut keeper = VoteKeeper::new(validators, ThresholdParams::default());
        let mut differences = Vec::new();
        fixture.check_state(&keeper, first, 0, &mut differences);
        let mut delivery = 0;
        for pair in trace.states.windows(2) {
            let state = &pair[1].value;
            if state == &pair[0].value {
                continue;
            }
            let WeightedVote::Vote(input_vote, weight, current_round) = &state.weighted_vote else {
                panic!("expected a vote delivery");
            };
            delivery += 1;
            let vote = fixture.vote(input_vote);
            let validator = keeper
                .validator_set()
                .get_by_address(&vote.validator_address)
                .unwrap();
            assert_eq!(u64::try_from(*weight).unwrap(), validator.voting_power);
            let actual = keeper.apply_vote(
                SignedVote::new(vote, Signature::test()),
                Round::from(*current_round),
            );
            let expected = model_output(&state.last_emitted, input_vote.round);
            if actual != expected {
                differences.push(format!(
                    "delivery {delivery} result: Quint {expected:?} / Rust {actual:?}"
                ));
            }
            fixture.check_state(&keeper, state, delivery, &mut differences);
        }
        assert!(delivery > 0, "trace contained no vote deliveries");
        differences
    }

    fn vote(&self, v: &crate::types::Vote) -> Vote {
        let height = Height::new(u64::try_from(v.height).unwrap());
        let round = Round::from(v.round);
        let value = value_from_model(&v.value_id);
        let address = self.addresses[&v.src_address];
        match v.vote_type {
            VoteType::Prevote => Vote::new_prevote(height, round, value, address),
            VoteType::Precommit => Vote::new_precommit(height, round, value, address),
        }
    }

    fn check_state(
        &self,
        actual_state: &VoteKeeper<TestContext>,
        expected: &State,
        delivery: usize,
        differences: &mut Vec<String>,
    ) {
        let expected_state = &expected.bookkeeper;

        assert_eq!(
            actual_state.total_weight(),
            expected_state.total_weight() as u64,
            "total_weight for the current height"
        );

        assert_eq!(actual_state.rounds(), expected_state.rounds.len());

        // Compare in round order so diagnostics are stable across runs.
        let rounds: BTreeMap<_, _> = expected_state.rounds.iter().collect();
        for (&round, expected_round) in rounds {
            assert_eq!(expected_round.height, expected_state.height);
            assert_eq!(expected_round.round, round);
            assert_eq!(
                expected_round.prevotes.total_weight,
                expected_state.total_weight()
            );
            assert_eq!(
                expected_round.precommits.total_weight,
                expected_state.total_weight()
            );

            let actual_round = actual_state.per_round(Round::from(round)).unwrap();

            let stored: BTreeSet<_> = actual_round
                .received_votes()
                .iter()
                .map(|v| v.message.clone())
                .collect();
            assert_eq!(
                stored.len(),
                actual_round.received_votes().len(),
                "duplicate stored vote"
            );
            let expected_votes: BTreeSet<_> = expected_round
                .prevotes
                .received_votes
                .iter()
                .chain(&expected_round.precommits.received_votes)
                .map(|v| self.vote(v))
                .collect();
            assert_eq!(stored, expected_votes, "stored votes at round {round}");

            let expected_outputs: BTreeSet<_> = expected_round
                .emitted_outputs
                .iter()
                .map(|out| model_output(out, round).expect("empty stored output"))
                .collect();
            let emitted = actual_round.emitted_outputs();
            if *emitted != expected_outputs {
                differences.push(format!("delivery {delivery} round {round} stored outputs: Quint {expected_outputs:?} / Rust {emitted:?}"));
            }

            let expected_addresses_weights = &expected_round.votes_addresses_weights;
            let actual_addresses_weights = &actual_round.addresses_weights().get_inner();
            assert_eq!(
                actual_addresses_weights.len(),
                expected_addresses_weights.len()
            );
            for (address, expected_weight) in expected_addresses_weights {
                assert_eq!(
                    actual_addresses_weights.get(self.addresses.get(address).unwrap()),
                    Some(&(*expected_weight as u64)),
                    "weight for address {address:?}"
                );
            }

            let actual_votes = &actual_round.votes();

            let expected_prevotes = &expected_round.prevotes;
            let actual_prevotes = actual_votes.prevotes();
            check_votes(expected_prevotes, actual_prevotes, &self.addresses);

            let expected_precommits = &expected_round.precommits;
            let actual_precommits = actual_votes.precommits();
            check_votes(expected_precommits, actual_precommits, &self.addresses);
        }
    }
}

/// The vote keeper associates an output with its containing round. Quint carries
/// that round in the output as well.
fn model_output(output: &VoteKeeperOutput, round: i64) -> Option<Output<ValueId>> {
    let (r, output) = match output {
        NoOutput => return None,
        PolkaAny(r) => (r, Output::PolkaAny),
        PolkaNil(r) => (r, Output::PolkaNil),
        PolkaValue(r, v) => (r, Output::PolkaValue(value_id(v))),
        PrecommitAny(r) => (r, Output::PrecommitAny),
        PrecommitValue(r, v) => (r, Output::PrecommitValue(value_id(v))),
        Skip(r) => (r, Output::SkipRound(Round::from(*r))),
    };
    assert_eq!(*r, round);
    Some(output)
}
