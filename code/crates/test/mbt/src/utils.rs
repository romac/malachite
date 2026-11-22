use std::path::{Path, PathBuf};

pub fn generate_test_traces(spec_rel_path: &str, gen_dir: &str, quint_seed: u64) {
    generate_named_test_traces(spec_rel_path, gen_dir, quint_seed);
}

pub(crate) fn generate_named_test_traces(
    spec_rel_path: &str,
    gen_dir: &str,
    quint_seed: u64,
) -> Vec<(String, PathBuf)> {
    println!("🪄 Generating test traces for {spec_rel_path:?}...");

    let spec_abs_path = format!("{}/specs/{spec_rel_path}", env!("CARGO_MANIFEST_DIR"),);
    let spec_path = Path::new(&spec_abs_path);

    // `quint test` picks up `run` definitions from every module in scope, including
    // the pure unit tests pulled in from imported modules (e.g. `isQuorumTest` in
    // `votekeeper.qnt`). Those tests don't mutate state, so their ITF traces have
    // `"vars": []` and cannot be deserialized into the Rust `State` type. Restrict
    // `quint test` to the runs defined directly in the target spec via `--match`.
    let source = std::fs::read_to_string(spec_path).expect("read Quint test module");
    let names = top_level_run_names(&source);
    assert!(!names.is_empty(), "no test runs in {spec_rel_path}");
    let match_pattern = format!("^({})$", names.join("|"));

    let mut command = std::process::Command::new("quint");
    command
        .arg("test")
        .arg("--out-itf")
        .arg(format!("{gen_dir}/test_{{test}}.itf.json"))
        .arg("--match")
        .arg(match_pattern);
    run_quint(command, spec_path, quint_seed);

    names
        .into_iter()
        .map(|name| {
            let path = Path::new(gen_dir).join(format!("test_{name}.itf.json"));
            assert!(
                path.is_file(),
                "Quint emitted no trace for {spec_rel_path}::{name}, seed {quint_seed}"
            );
            (name, path)
        })
        .collect()
}

/// Extract the names of top-level `run NAME = ...` definitions from a Quint source.
///
/// "Top-level" here means defined directly in a module body, not nested inside
/// another definition. Quint test modules in this repo each declare a single
/// top-level module, and `run` declarations sit one level deep inside its braces;
/// nothing here ever nests a `run` inside another `run`. A brace-depth counter
/// plus skipping of strings and comments is enough to isolate them.
fn top_level_run_names(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut names = Vec::new();
    let mut depth: i32 = 0;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        match b {
            // Line comment
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            // Block comment
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            // String literal
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                i = (i + 1).min(bytes.len());
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            // `run NAME` at module level: depth must be exactly 1 (inside the
            // single top-level module, not nested deeper).
            _ if depth == 1 && is_keyword_at(bytes, i, b"run") => {
                i += 3;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let name_start = i;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                if name_start < i {
                    names.push(String::from_utf8_lossy(&bytes[name_start..i]).into_owned());
                }
            }
            _ => i += 1,
        }
    }

    names
}

fn is_keyword_at(bytes: &[u8], i: usize, kw: &[u8]) -> bool {
    if !bytes[i..].starts_with(kw) {
        return false;
    }
    let before_ok = i == 0 || !is_ident_continue(bytes[i - 1]);
    let after_ok = bytes
        .get(i + kw.len())
        .is_none_or(|b| !is_ident_continue(*b));
    before_ok && after_ok
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(crate) fn run_quint(mut command: std::process::Command, spec: &Path, seed: u64) {
    let output = command
        .arg("--backend")
        .arg(std::env::var_os("QUINT_BACKEND").unwrap_or_else(|| "rust".into()))
        .arg("--seed")
        .arg(seed.to_string())
        .arg(spec)
        .current_dir(spec.parent().unwrap())
        .output()
        .expect("run Quint");
    assert!(
        output.status.success(),
        "Quint failed for {} with seed {seed} ({}):\n{}\n{}",
        spec.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const DEFAULT_QUINT_SEED: u64 = 118;

pub fn quint_seed() -> u64 {
    let seed = std::env::var("QUINT_SEED")
        .ok()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or(DEFAULT_QUINT_SEED);

    println!("Using QUINT_SEED={seed}");

    seed
}

#[cfg(test)]
mod tests {
    use super::top_level_run_names;

    #[test]
    fn extracts_top_level_runs() {
        let src = r#"
            module foo {
                run firstTest = all { assert(true) }
                run secondTest =
                    initWith().then(step)
                pure def helper = 1
                run thirdTest = all { assert(helper == 1) }
            }
        "#;
        assert_eq!(
            top_level_run_names(src),
            vec!["firstTest", "secondTest", "thirdTest"],
        );
    }

    #[test]
    fn skips_comments_and_strings() {
        let src = r#"
            module foo {
                // run commentedOut = 1
                /* run blockCommented = 1 */
                val s = "run insideString = 1"
                run realOne = 1
            }
        "#;
        assert_eq!(top_level_run_names(src), vec!["realOne"]);
    }

    #[test]
    fn skips_nested_runs() {
        // Hypothetical: a `run` mentioned inside a nested block should not be
        // picked up as a top-level test.
        let src = r#"
            module foo {
                run topLevel = {
                    // This "run" is at depth 2 and must not be picked up.
                    nested.run x = 1
                }
                run another = 2
            }
        "#;
        assert_eq!(top_level_run_names(src), vec!["topLevel", "another"]);
    }
}
