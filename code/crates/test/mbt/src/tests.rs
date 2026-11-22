pub mod consensus;
mod driver;
mod host;
mod streaming;
mod sync;
pub mod votekeeper;

use serde::de::DeserializeOwned;

use crate::utils::{generate_named_test_traces, quint_seed};

/// Generate one ITF file per named scenario and replay each against the implementation.
fn replay_cases<T: DeserializeOwned>(spec: &str, mut replay: impl FnMut(&str, &itf::Trace<T>)) {
    let dir = trace_dir();
    let seed = quint_seed();
    let cases = generate_named_test_traces(spec, dir.path().to_str().unwrap(), seed);
    for (name, path) in cases {
        let trace = read_trace(&path);
        assert!(
            trace.states.len() > 1,
            "{name}, seed {seed}: empty execution"
        );
        println!(
            "Replaying {name}, seed {seed}, {} ITF states",
            trace.states.len()
        );
        replay(&name, &trace);
    }
}

/// A newly fixed difference also changes the baseline. Strict mode demands full agreement.
fn check_differences(baseline: &str, differences: &[String]) {
    let strict = std::env::var("MBT_STRICT").as_deref() == Ok("1");
    let expected = if strict { "" } else { baseline.trim_end() };
    let actual = differences.join("\n");
    assert!(actual == expected, "Quint/Rust differences changed (MBT_STRICT={strict}).\nObserved:\n{actual}\nEnd observed.\nExpected:\n{expected}");
}

fn trace_dir() -> tempfile::TempDir {
    let mut dir = tempfile::tempdir().expect("create trace directory");
    if std::env::var_os("KEEP_TEMP").is_some() {
        dir.disable_cleanup(true);
        println!("ITF traces: {}", dir.path().display());
    }
    dir
}

fn read_trace<T: DeserializeOwned>(path: &std::path::Path) -> itf::Trace<T> {
    let data = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    itf::trace_from_str(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
