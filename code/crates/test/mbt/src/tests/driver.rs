mod agreement;
mod model;

use malachitebft_core_driver::{Driver, Input, Output, Step};
use malachitebft_core_state_machine::state::RoundValue;
use malachitebft_core_types::Round;
use malachitebft_test::{Height, TestContext, Value};

use crate::tests::consensus::utils::value_from_model;
use crate::tests::{check_differences, replay_cases};
use model::{Fixture, Frame, Model};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    height: Height,
    round: Round,
    step: Step,
    locked: Option<RoundValue<Value>>,
    valid: Option<RoundValue<Value>>,
    decided: Option<RoundValue<Value>>,
    outputs: Vec<Output<TestContext>>,
}

fn model_observation(
    fixture: &Fixture,
    model: &Model,
    outputs: Vec<Output<TestContext>>,
) -> Observation {
    let cs = &model.cs;
    let round_value =
        |r: Option<i64>, value: &crate::types::Value| match (r, value_from_model(value)) {
            (Some(r), Some(v)) => Some(RoundValue::new(v, Round::from(r))),
            (None, None) => None,
            pair => panic!("inconsistent model round/value pair: {pair:?}"),
        };
    assert!(model.chain.len() <= 1, "two decisions at the same height");
    Observation {
        height: Height::new(cs.height.try_into().unwrap()),
        round: Round::from(cs.round),
        step: cs.step.to_round_step().unwrap_or(Step::Unstarted),
        locked: round_value(cs.locked_round, &cs.locked_value),
        valid: round_value(cs.valid_round, &cs.valid_value),
        decided: model.chain.last().map(|entry| {
            let proposal = fixture.proposal(&entry.decision);
            RoundValue::new(proposal.value, proposal.round)
        }),
        outputs,
    }
}

fn rust_observation(
    driver: &Driver<TestContext>,
    outputs: Vec<Output<TestContext>>,
) -> Observation {
    let state = driver.round_state();
    Observation {
        height: state.height,
        round: state.round,
        step: state.step,
        locked: state.locked.clone(),
        valid: state.valid.clone(),
        decided: state.decision.clone(),
        outputs,
    }
}

// Quint folds the engine's NewRound callback into the input that triggered it.
// Follow each actual callback using the fixture's proposer schedule before comparing.
fn process(
    fixture: &Fixture,
    driver: &mut Driver<TestContext>,
    input: Input<TestContext>,
) -> Vec<Output<TestContext>> {
    let mut effects = Vec::new();
    for output in driver.process(input).expect("driver rejected trace input") {
        match output {
            Output::NewRound(h, r) => {
                effects.extend(process(
                    fixture,
                    driver,
                    Input::NewRound(h, r, fixture.leader(r)),
                ));
            }
            output => effects.push(output),
        }
    }
    effects
}

#[test]
fn driver_itf() {
    let mut differences = Vec::new();
    replay_cases(
        "consensus/quint/tests/driver/driverTest.qnt",
        |name, trace: &itf::Trace<Frame>| {
            let first = &trace.states[0].value;
            let (fixture, mut driver) = Fixture::new(&first.model, &first.leaders);
            assert_eq!(
                model_observation(&fixture, &first.model, vec![]),
                rust_observation(&driver, vec![])
            );
            for pair in trace.states.windows(2) {
                let previous = &pair[0].value;
                let frame = &pair[1].value;
                if frame.sequence == previous.sequence {
                    assert_eq!(frame, previous, "stuttering state changed");
                    continue;
                }
                assert_eq!(frame.sequence, previous.sequence + 1);
                let input = fixture.input(&frame.event, &driver);
                let outputs = process(&fixture, &mut driver, input);
                let model = model_observation(&fixture, &frame.model, fixture.outputs(frame));
                let actual = rust_observation(&driver, outputs);
                if actual != model {
                    let mut fields = Vec::new();
                    macro_rules! compare {
                        ($field:ident) => {
                            if model.$field != actual.$field {
                                fields.push(format!(
                                    "  {}: Quint {} / Rust {}",
                                    stringify!($field),
                                    fixture.display(&model.$field),
                                    fixture.display(&actual.$field)
                                ));
                            }
                        };
                    }
                    compare!(height);
                    compare!(round);
                    compare!(step);
                    compare!(locked);
                    compare!(valid);
                    compare!(decided);
                    compare!(outputs);
                    differences.push(format!(
                        "{name} event {}\n{}",
                        frame.sequence,
                        fields.join("\n")
                    ));
                }
            }
        },
    );
    check_differences(include_str!("driver/known-differences.txt"), &differences);
}
