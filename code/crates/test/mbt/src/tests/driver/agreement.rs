use std::collections::{BTreeMap, BTreeSet};

use malachitebft_core_driver::{Input, Output};
use serde::Deserialize;

use super::model::{Event, Fixture, Model};
use super::{model_observation, process, rust_observation};
use crate::consensus::Output as ModelOutput;
use crate::tests::{check_differences, replay_cases};

#[derive(Deserialize)]
struct Node {
    es: Model,
}

#[derive(Debug, PartialEq, Deserialize)]
struct History {
    validator: String,
    input: Event,
    output: ModelOutput,
}

#[derive(Deserialize)]
struct NetworkFrame {
    #[serde(rename = "agreementReplayTest::TendermintDSL::system")]
    system: BTreeMap<String, Node>,
    #[serde(rename = "agreementReplayTest::TendermintDSL::_hist")]
    history: History,
    // Only initialization and the completed scenario need an observation.
    #[serde(rename = "roundZeroProposer", default)]
    proposer: Option<String>,
    #[serde(default)]
    agreement: Option<bool>,
}

#[test]
fn honest_quorums() {
    replay_cases(
        "consensus/quint/tests/driver/agreementReplayTest.qnt",
        |name, trace: &itf::Trace<NetworkFrame>| {
            let first = &trace.states[0].value;
            let leader = first.proposer.as_ref().expect("fixture has no proposer");
            let mut nodes: BTreeMap<_, _> = first
                .system
                .iter()
                .map(|(id, n)| (id, Fixture::new(&n.es, std::slice::from_ref(leader))))
                .collect();
            assert_eq!(nodes.len(), 4);
            let mut proposals = Vec::new();
            let mut votes = Vec::new();
            let mut decisions = BTreeMap::new();
            let mut external = 0;
            for pair in trace.states.windows(2) {
                let h = &pair[1].value.history;
                // Delivery-to-inbox steps retain the previous driver history. Pending and
                // step-change work is already drained by the real Driver::process call.
                if h == &pair[0].value.history
                    || matches!(
                        h.input,
                        Event::Initial | Event::Pending(_) | Event::StepChange
                    )
                {
                    continue;
                }
                let (fixture, driver) = nodes.get_mut(&h.validator).unwrap();
                let input = fixture.input(&h.input, driver);
                match &input {
                    Input::Proposal(p, _) => assert!(
                        proposals.contains(&p.message),
                        "proposal was never emitted by a Rust node"
                    ),
                    Input::Vote(v) => assert!(
                        votes.contains(&v.message),
                        "vote was never emitted by a Rust node"
                    ),
                    _ => {}
                }
                external += 1;
                for output in process(fixture, driver, input) {
                    match output {
                        Output::Propose(p) => proposals.push(p),
                        Output::Vote(v) => votes.push(v),
                        Output::Decide(_, p) => assert!(decisions
                            .insert(h.validator.clone(), p.value.id())
                            .is_none()),
                        _ => {}
                    }
                }
            }

            // Four starts, one value from the proposer, four delivered proposals,
            // twelve delivered prevotes and two distinct three-vote precommit quorums.
            assert_eq!(external, 27);
            assert_eq!(decisions.len(), 2);
            let last = &trace.states.last().unwrap().value;
            for (id, (fixture, driver)) in nodes {
                let model = &last.system[id].es;
                assert_eq!(
                    model_observation(&fixture, model, vec![]),
                    rust_observation(&driver, vec![]),
                    "node {id}"
                );
                for entry in &model.chain {
                    let proposal = fixture.proposal(&entry.decision);
                    assert_eq!(decisions[id], proposal.value.id());
                    let mut actual: Vec<_> = driver
                        .restore_precommits(proposal.round, &proposal.value.id())
                        .into_iter()
                        .map(|v| v.message)
                        .collect();
                    let mut expected: Vec<_> =
                        entry.commit.iter().map(|v| fixture.vote(v)).collect();
                    actual.sort_by_key(|v| v.validator_address);
                    expected.sort_by_key(|v| v.validator_address);
                    assert_eq!(actual.len(), 3);
                    assert_eq!(actual, expected, "commit evidence for {id}");
                }
            }
            let rust_agreement = decisions.values().collect::<BTreeSet<_>>().len() == 1;
            assert!(rust_agreement, "honest Rust nodes decided different values");
            let model_agreement = last.agreement.expect("missing final AgreementInv");
            let mut differences = Vec::new();
            if rust_agreement != model_agreement {
                differences.push(format!(
                    "{name} AgreementInv: Quint {model_agreement} / Rust {rust_agreement}"
                ));
            }
            check_differences(
                "honestQuorumsTest AgreementInv: Quint false / Rust true",
                &differences,
            );
        },
    );
}
