//! CI (`.github/workflows/ci.yml`): the workspace job on Ubuntu, and `rayx-cli`'s formatting,
//! Clippy and tests on Linux, macOS and Windows, on pull requests and pushes to `main`.

use std::path::PathBuf;

use serde_yaml_ng::Value as Yaml;

fn ci() -> Yaml {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml_ng::from_str(&text).expect("ci.yml parses")
}

fn get<'a>(value: &'a Yaml, key: &str) -> &'a Yaml {
    value
        .get(key)
        .unwrap_or_else(|| panic!("missing key `{key}`"))
}

fn runs(job: &Yaml) -> Vec<&str> {
    get(job, "steps")
        .as_sequence()
        .expect("steps")
        .iter()
        .filter_map(|step| step.get("run").and_then(Yaml::as_str))
        .collect()
}

#[test]
fn ci_runs_on_pull_requests_and_main_pushes() {
    let ci = ci();
    let on = get(&ci, "on");
    assert!(on.get("pull_request").is_some());
    let branches = get(get(on, "push"), "branches")
        .as_sequence()
        .expect("push branches");
    assert_eq!(
        branches.iter().map(Yaml::as_str).collect::<Vec<_>>(),
        [Some("main")],
        "ci.yml runs on pushes to main only, never on other branches"
    );
}

#[test]
fn the_cli_is_checked_on_linux_macos_and_windows() {
    let ci = ci();
    let job = get(get(&ci, "jobs"), "cli");
    let mut os: Vec<&str> = get(get(get(job, "strategy"), "matrix"), "os")
        .as_sequence()
        .expect("os matrix")
        .iter()
        .filter_map(Yaml::as_str)
        .collect();
    os.sort_unstable();
    assert_eq!(os, ["macos-latest", "ubuntu-24.04", "windows-latest"]);
    assert_eq!(get(job, "runs-on").as_str(), Some("${{ matrix.os }}"));
    assert_eq!(
        runs(job),
        [
            "cargo fmt -p rayx-cli --check",
            "cargo clippy -p rayx-cli --all-targets -- -D warnings",
            "cargo test -p rayx-cli"
        ],
        "CI formats, lints and runs the tests; the ignored proofs stay out"
    );
}

#[test]
fn the_workspace_job_stays_on_ubuntu() {
    let ci = ci();
    let job = get(get(&ci, "jobs"), "rust");
    assert_eq!(get(job, "runs-on").as_str(), Some("ubuntu-24.04"));
    assert_eq!(
        runs(job),
        [
            "sudo apt-get update && sudo apt-get install -y clang libclang-dev",
            "cargo fmt --all --check",
            "cargo clippy --workspace --all-targets -- -D warnings",
            "cargo test --workspace",
            "cargo check -p rayx_core -p rayx_runtime --target wasm32-unknown-unknown"
        ]
    );
}
