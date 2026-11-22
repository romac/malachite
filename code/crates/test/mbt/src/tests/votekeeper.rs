use rand::rngs::StdRng;
use rand::SeedableRng;

use super::{check_differences, read_trace, replay_cases, trace_dir};
use crate::utils::{quint_seed, run_quint};
use crate::votekeeper::{State, VoteKeeperOutput};

pub mod runner;
pub mod utils;

use runner::VoteKeeperRunner;

const RANDOM_SEED: u64 = 0x42;

#[test]
fn test_itf() {
    let mut differences = Vec::new();
    replay_cases(
        "consensus/quint/tests/votekeeper/votekeeperTest.qnt",
        |case, trace| {
            let observed = VoteKeeperRunner::replay(trace, StdRng::seed_from_u64(RANDOM_SEED));
            differences.extend(observed.into_iter().map(|diff| format!("{case} {diff}")));
        },
    );
    check_differences(
        include_str!("votekeeper/known-differences.txt"),
        &differences,
    );
}

const SAMPLE_STEPS: usize = 24;

fn vote_simulator() -> (std::process::Command, std::path::PathBuf) {
    let spec = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("specs/consensus/quint/tests/votekeeper/voteReplaySampling.qnt");
    let mut command = std::process::Command::new("quint");
    // Quint checks the model's declared invariants for both random and directed runs.
    command
        .args(["run", "--verbosity=0", "--max-steps"])
        .arg(SAMPLE_STEPS.to_string());
    (command, spec)
}

#[test]
fn sampled_vote_schedules() {
    const TRACES: usize = 32;
    let dir = trace_dir();
    let seed = quint_seed();
    let (mut command, spec) = vote_simulator();
    command
        .arg("--max-samples")
        .arg(TRACES.to_string())
        .arg("--n-traces")
        .arg(TRACES.to_string())
        .arg("--out-itf")
        .arg(dir.path().join("sample-{seq}.itf.json"));
    run_quint(command, &spec, seed);
    for i in 0..TRACES {
        let trace: itf::Trace<State> = read_trace(&dir.path().join(format!("sample-{i}.itf.json")));
        println!("Replaying vote sample {i}, seed {seed}");
        assert_eq!(trace.states.len(), SAMPLE_STEPS + 1);
        for (index, frame) in trace.states.iter().enumerate() {
            assert_eq!(frame.value.sequence, Some(index as u64));
        }
        assert!(
            trace
                .states
                .iter()
                .any(|s| matches!(s.value.last_emitted, VoteKeeperOutput::Skip(_))),
            "sample {i} did not exercise a future-round skip"
        );
        let differences = VoteKeeperRunner::replay(&trace, StdRng::seed_from_u64(RANDOM_SEED));
        assert!(
            differences.is_empty(),
            "sample {i}, seed {seed}: {differences:?}"
        );
    }
}

#[test]
fn replay_targeted_vote_observations() {
    let dir = trace_dir();
    let seed = quint_seed();
    for goal in [
        "emitPrecommitValue",
        "emitPolkaAny",
        "emitPolkaNil",
        "emitSkip",
    ] {
        let path = dir.path().join(format!("{goal}.itf.json"));
        let (mut command, spec) = vote_simulator();
        command
            .args(["--max-samples=512", "--observation", goal, "--out-itf"])
            .arg(&path);
        run_quint(command, &spec, seed);
        let trace: itf::Trace<State> = read_trace(&path);
        let last = &trace
            .states
            .last()
            .expect("empty observation trace")
            .value
            .last_emitted;
        assert!(
            matches!(
                (goal, last),
                ("emitPrecommitValue", VoteKeeperOutput::PrecommitValue(..))
                    | ("emitPolkaAny", VoteKeeperOutput::PolkaAny(_))
                    | ("emitPolkaNil", VoteKeeperOutput::PolkaNil(_))
                    | ("emitSkip", VoteKeeperOutput::Skip(_))
            ),
            "{goal}, seed {seed}: unexpected terminal output {last:?}"
        );
        let differences = VoteKeeperRunner::replay(&trace, StdRng::seed_from_u64(RANDOM_SEED));
        assert!(
            differences.is_empty(),
            "{goal}, seed {seed}: {differences:?}"
        );
    }
}
