use arc_malachitebft_core_driver::{Driver, Input, Step};
use malachitebft_core_types::{Round, Validity};
use malachitebft_test::utils::validators::make_validators;
use malachitebft_test::{Address, Height, TestContext, ValidatorSet, Value};

use crate::utils::*;

fn fixture() -> (Driver<TestContext>, [Address; 4]) {
    let validators = make_validators([1, 1, 1, 1]).map(|(validator, _)| validator);
    let addresses = validators.each_ref().map(|validator| validator.address);
    let driver = Driver::new(
        TestContext::new(),
        Height::new(1),
        ValidatorSet::new(validators.to_vec()),
        addresses[3],
        Default::default(),
    );
    (driver, addresses)
}

#[test]
fn step_entry_preserves_order_of_votes_and_independent_timers() {
    let (mut driver, voters) = fixture();
    let round = Round::new(1);
    let value = Value::new(17);
    driver
        .process(new_round_input(Round::new(0), voters[0]))
        .unwrap();
    driver
        .process(proposal_input(
            round,
            value.clone(),
            Round::Nil,
            Validity::Valid,
            voters[0],
        ))
        .unwrap();
    let skip = driver
        .process(polka_certificate_input_at(
            round,
            value.clone(),
            &voters[..3],
        ))
        .unwrap();
    assert_eq!(skip, [new_round_output(round)]);
    assert_eq!(driver.round_certificate().unwrap().enter_round, round);

    for voter in &voters[..3] {
        driver.process(precommit_nil_input(round, voter)).unwrap();
    }
    // Starting the round re-enables its timers. The proposal enables L22,
    // both quorum timers are already eligible, and the new Prevote step enables L36.
    assert_eq!(
        driver.process(new_round_input(round, voters[0])).unwrap(),
        [
            start_propose_timer_output(round),
            prevote_output(round, value.clone(), &voters[3]),
            start_prevote_timer_output(round),
            start_precommit_timer_output(round),
            precommit_output(round, value.clone(), &voters[3]),
        ]
    );
    assert_eq!(driver.round_state().locked.as_ref().unwrap().value, value);
    assert_eq!(driver.step(), Step::Precommit);
}

#[test]
fn proposal_notification_uses_the_retained_verdict() {
    let (mut driver, voters) = fixture();
    let round = Round::new(0);
    let value = Value::new(17);
    driver.process(new_round_input(round, voters[0])).unwrap();
    let proposal = |validity| proposal_input(round, value.clone(), Round::Nil, validity, voters[0]);
    driver.process(proposal(Validity::Valid)).unwrap();

    // Preload the signed evidence without dispatching notifications. The next
    // proposal input is the event that asks the driver to examine this quorum.
    for voter in &voters[..3] {
        let Input::Vote(vote) = prevote_input_at(round, value.clone(), voter) else {
            unreachable!()
        };
        driver.votes_mut().apply_vote(vote, round);
    }
    assert_eq!(
        driver.process(proposal(Validity::Invalid)).unwrap(),
        [precommit_output(round, value.clone(), &voters[3])]
    );
    let (_, validity) = driver
        .proposal_and_validity_for_round_and_value(round, value.id())
        .unwrap();
    assert_eq!(*validity, Validity::Valid);
    assert_eq!(driver.round_state().locked.as_ref().unwrap().value, value);
}

#[test]
fn late_polka_updates_valid_without_relocking_or_voting_again() {
    for certificate_first in [false, true] {
        let (mut driver, voters) = fixture();
        let round = Round::new(0);
        let value = Value::new(17);
        driver.process(new_round_input(round, voters[0])).unwrap();
        driver.process(timeout_propose_input(round)).unwrap();
        let nil_precommit = driver.process(timeout_prevote_input(round)).unwrap();
        assert_eq!(nil_precommit, [precommit_nil_output(round, &voters[3])]);

        let proposal = proposal_input(round, value.clone(), Round::Nil, Validity::Valid, voters[0]);
        let certificate = polka_certificate_input_at(round, value.clone(), &voters[..3]);
        let mut inputs = [proposal, certificate.clone()];
        if certificate_first {
            inputs.reverse();
        }
        for input in inputs {
            assert!(driver.process(input).unwrap().is_empty());
        }
        assert!(driver.process(certificate).unwrap().is_empty());
        assert_eq!(driver.step(), Step::Precommit);
        assert!(driver.round_state().locked.is_none());
        let valid = driver.valid_value().unwrap();
        assert_eq!((valid.round, &valid.value), (round, &value));
        assert_eq!(driver.polka_certificates().len(), 1);
        assert_eq!(
            driver
                .votes()
                .per_round(round)
                .unwrap()
                .received_votes()
                .len(),
            3
        );
    }
}

#[test]
fn previous_polka_selects_the_proposal_that_cites_its_round() {
    let round = Round::new(2);
    let proof_round = Round::new(1);
    let value = Value::new(17);
    for certificate in [false, true] {
        for proof_first in [false, true] {
            let (mut driver, voters) = fixture();
            driver.process(new_round_input(round, voters[0])).unwrap();
            let proposal = |pol_round| {
                proposal_input(round, value.clone(), pol_round, Validity::Valid, voters[0])
            };

            // Both messages propose the same value, but only the second cites round 1.
            assert!(driver.process(proposal(Round::new(0))).unwrap().is_empty());
            let mut proof: Vec<_> = if certificate {
                vec![polka_certificate_input_at(
                    proof_round,
                    value.clone(),
                    &voters[..3],
                )]
            } else {
                voters[..3]
                    .iter()
                    .map(|voter| prevote_input_at(proof_round, value.clone(), voter))
                    .collect()
            };
            if proof_first {
                proof.push(proposal(proof_round));
            } else {
                proof.insert(0, proposal(proof_round));
            }
            let mut outputs = Vec::new();
            for input in proof {
                outputs.extend(driver.process(input).unwrap());
            }
            assert_eq!(outputs, [prevote_output(round, value.clone(), &voters[3])]);
            assert!(driver.polka_certificate(proof_round, &value.id()).is_some());
            assert_eq!(
                driver
                    .take_proposal_evidence()
                    .get(&voters[0])
                    .unwrap()
                    .len(),
                1
            );
        }
    }
}

#[test]
fn commit_quorum_selects_a_valid_matching_proposal() {
    let (mut driver, voters) = fixture();
    let round = Round::new(2);
    let value = Value::new(17);
    driver.process(new_round_input(round, voters[0])).unwrap();
    for (pol_round, validity) in [
        (Round::new(0), Validity::Invalid),
        (Round::new(1), Validity::Valid),
    ] {
        assert!(driver
            .process(proposal_input(
                round,
                value.clone(),
                pol_round,
                validity,
                voters[0]
            ))
            .unwrap()
            .is_empty());
    }
    let candidate = driver
        .valid_proposal_for_round_and_value(round, value.id())
        .unwrap();
    assert_eq!(candidate.message.pol_round, Round::new(1));
    let expected = decide_output(round, candidate.message.clone());
    assert_eq!(
        driver
            .process(commit_certificate_input_at(
                round,
                value.clone(),
                &voters[..3]
            ))
            .unwrap(),
        [expected]
    );
    assert_eq!(driver.decided_value(), Some((round, value)));
    assert_eq!(driver.commit_certificates().len(), 1);
}

#[test]
fn new_height_with_the_same_round_number_evaluates_only_its_own_evidence() {
    let (mut driver, voters) = fixture();
    let round = Round::new(0);
    let value = Value::new(17);
    driver.process(new_round_input(round, voters[0])).unwrap();
    driver
        .process(polka_certificate_input_at(
            round,
            value.clone(),
            &voters[..3],
        ))
        .unwrap();
    driver
        .process(proposal_input(
            round,
            value,
            Round::Nil,
            Validity::Valid,
            voters[0],
        ))
        .unwrap();
    assert_eq!(driver.step(), Step::Precommit);

    let height = Height::new(2);
    let outputs = driver
        .process(Input::NewRound(height, round, voters[0]))
        .unwrap();
    assert_eq!(outputs, [start_propose_timer_output(round)]);
    assert_eq!(driver.height(), height);
    assert_eq!(driver.step(), Step::Propose);
    assert!(driver.proposals_and_validities_for_round(round).is_empty());
    assert!(driver.votes().per_round(round).is_none());
    assert!(driver.polka_certificates().is_empty());
    assert!(driver.round_state().locked.is_none());
}
