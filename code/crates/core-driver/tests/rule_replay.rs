//! Comparative replay fixture for scripts/compare-driver.sh.

use std::fs::File;
use std::io::{self, BufWriter, Write};

use arc_malachitebft_core_driver::{Driver, Input, Output};
use malachitebft_core_types::{Round, Validity};
use malachitebft_test::utils::validators::make_validators;
use malachitebft_test::{Address, Height, TestContext, ValidatorSet, Value};

#[path = "it/utils.rs"]
pub mod utils;
use utils::*;

fn deliver(
    driver: &mut Driver<TestContext>,
    input: Input<TestContext>,
    proposer: Address,
    trace: &mut impl Write,
) -> io::Result<()> {
    writeln!(trace, "input {input:?}")?;
    let outputs = driver
        .process(input)
        .expect("fixture input must be accepted");
    let state = driver.round_state();
    writeln!(trace, "outputs {outputs:?}")?;
    writeln!(
        trace,
        "state {:?}",
        (
            state.height,
            state.round,
            state.step,
            &state.locked,
            &state.valid,
            &state.decision,
            state.scheduled_timeouts
        )
    )?;
    writeln!(
        trace,
        "last votes {:?}",
        (driver.last_prevote(), driver.last_precommit())
    )?;
    writeln!(trace, "proposals {:?}", driver.proposals().all_rounds())?;
    writeln!(trace, "votes {:?}", driver.votes().all_rounds())?;
    writeln!(
        trace,
        "certificates {:?}",
        (
            driver.round_certificate(),
            driver.polka_certificates(),
            driver.commit_certificates()
        )
    )?;
    writeln!(
        trace,
        "equivocations {:?}",
        (driver.proposals().evidence(), driver.votes().evidence())
    )?;

    // The engine supplies a proposer for every requested round. Capture the
    // boundary both before and after this callback.
    for output in outputs {
        if let Output::NewRound(height, round) = output {
            deliver(
                driver,
                Input::NewRound(height, round, proposer),
                proposer,
                trace,
            )?;
        }
    }
    Ok(())
}

fn permutations(
    order: &mut [usize; 6],
    next: usize,
    visit: &mut impl FnMut(&[usize; 6]) -> io::Result<()>,
) -> io::Result<()> {
    if next == order.len() {
        return visit(order);
    }
    for i in next..order.len() {
        order.swap(next, i);
        permutations(order, next + 1, visit)?;
        order.swap(next, i);
    }
    Ok(())
}

#[test]
#[ignore = "run scripts/compare-driver.sh BASE [CANDIDATE] to compare two revisions"]
fn driver_delivery_orders() -> io::Result<()> {
    let path = std::env::var_os("MALACHITE_DRIVER_TRACE").expect("trace path required");
    let mut trace = BufWriter::new(File::create(path)?);
    let validators = make_validators([1, 1, 1, 1]).map(|(validator, _)| validator);
    let voters = validators.each_ref().map(|validator| validator.address);
    let validator_set = ValidatorSet::new(validators.to_vec());
    let proposer = voters[0];
    let local = voters[3];
    let height = Height::new(1);
    let r0 = Round::new(0);
    let r1 = Round::new(1);
    let value = Value::new(7);
    let proposal =
        |round, pol| proposal_input(round, value.clone(), pol, Validity::Valid, proposer);

    let cases = [
        (
            "current polka and commit",
            [
                proposal(r0, Round::Nil),
                polka_certificate_input_at(r0, value.clone(), &voters[..3]),
                prevote_input_at(r0, value.clone(), &voters[0]),
                timeout_propose_input(r0),
                timeout_prevote_input(r0),
                commit_certificate_input_at(r0, value.clone(), &voters[..3]),
            ],
        ),
        (
            "previous polka and round entry",
            [
                proposal(r1, r0),
                polka_certificate_input_at(r0, value.clone(), &voters[..3]),
                prevote_input_at(r1, value.clone(), &voters[0]),
                prevote_input_at(r1, value.clone(), &voters[1]),
                timeout_propose_input(r1),
                polka_certificate_input_at(r1, value.clone(), &voters[..3]),
            ],
        ),
        (
            "future commit and skip",
            [
                proposal(r1, Round::Nil),
                precommit_input(r1, value.clone(), &voters[0]),
                precommit_input(r1, value.clone(), &voters[1]),
                precommit_input(r1, value.clone(), &voters[2]),
                precommit_nil_input(r0, &local),
                timeout_propose_input(r1),
            ],
        ),
        (
            "nil polka and retained evidence",
            [
                proposal(r0, Round::Nil),
                proposal_input(r0, Value::new(9), Round::Nil, Validity::Valid, proposer),
                prevote_nil_input_at(r0, &voters[0]),
                prevote_nil_input_at(r0, &voters[1]),
                prevote_nil_input_at(r0, &voters[2]),
                polka_certificate_input_at(r0, value.clone(), &voters[..3]),
            ],
        ),
    ];

    for (case, inputs) in cases {
        permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |order| {
            let mut driver = Driver::new(
                TestContext::new(),
                height,
                validator_set.clone(),
                local,
                Default::default(),
            );
            writeln!(trace, "schedule {case} {order:?}")?;
            deliver(
                &mut driver,
                new_round_input(r0, proposer),
                proposer,
                &mut trace,
            )?;
            for &index in order {
                deliver(&mut driver, inputs[index].clone(), proposer, &mut trace)?;
            }
            Ok(())
        })?;
    }
    trace.flush()
}
