use rand::rngs::StdRng;
use rand::SeedableRng;

use super::replay_cases;
use crate::consensus::State;

pub mod runner;
pub mod utils;

use runner::ConsensusRunner;

const RANDOM_SEED: u64 = 0x42;

#[test]
fn test_itf() {
    replay_cases(
        "consensus/quint/tests/consensus/consensusTest.qnt",
        |_, trace: &itf::Trace<State>| {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let addresses = utils::build_address_map(trace, &mut rng);
            trace.run_on(ConsensusRunner::new(addresses)).unwrap();
        },
    );
}
