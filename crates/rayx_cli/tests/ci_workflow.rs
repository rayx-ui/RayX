//! CI (`.github/workflows/ci.yml`): the workspace job on Ubuntu, and `rayx-cli`'s formatting,
//! Clippy and tests on Linux, macOS and Windows, on pull requests and pushes to `main`; and every
//! feature-gated test group (`[[test]] required-features`) run somewhere in CI.

use std::path::{Path, PathBuf};

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
            "cargo clippy -p rayx --all-features --all-targets -- -D warnings",
            "cargo test -p rayx --all-features",
            "cargo check -p rayx_app -p rayx_devtools_protocol --target wasm32-unknown-unknown"
        ]
    );
}

/// The workspace's root folder.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest(path: &Path) -> toml::Table {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.parse()
        .unwrap_or_else(|e| panic!("{} parses: {e}", path.display()))
}

/// Every workspace member's manifest.
fn members() -> Vec<toml::Table> {
    let root = root();
    let workspace = manifest(&root.join("Cargo.toml"));
    workspace["workspace"]["members"]
        .as_array()
        .expect("workspace members")
        .iter()
        .map(|member| {
            manifest(
                &root
                    .join(member.as_str().expect("member path"))
                    .join("Cargo.toml"),
            )
        })
        .collect()
}

fn package_name(manifest: &toml::Table) -> &str {
    manifest["package"]["name"].as_str().expect("package name")
}

/// The features of `package` some member's dependency (of any kind, on any target) turns on:
/// what a `cargo test --workspace` run builds it with.
fn features_enabled_by_members(members: &[toml::Table], package: &str) -> Vec<String> {
    fn tables(manifest: &toml::Table) -> Vec<&toml::Table> {
        let kinds = ["dependencies", "dev-dependencies", "build-dependencies"];
        let mut found: Vec<&toml::Table> = kinds
            .iter()
            .filter_map(|kind| manifest.get(*kind).and_then(toml::Value::as_table))
            .collect();
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for target in targets.values().filter_map(toml::Value::as_table) {
                found.extend(
                    kinds
                        .iter()
                        .filter_map(|kind| target.get(*kind).and_then(toml::Value::as_table)),
                );
            }
        }
        found
    }
    members
        .iter()
        .flat_map(tables)
        .filter_map(|dependencies| dependencies.get(package))
        .filter_map(|dependency| dependency.get("features").and_then(toml::Value::as_array))
        .flatten()
        .filter_map(|feature| feature.as_str().map(str::to_owned))
        .collect()
}

#[test]
fn every_feature_gated_test_group_runs_in_ci() {
    let ci = ci();
    let job = get(get(&ci, "jobs"), "rust");
    let runs = runs(job);
    let members = members();
    let mut unrun = Vec::new();
    for member in &members {
        let package = package_name(member);
        let Some(tests) = member.get("test").and_then(toml::Value::as_array) else {
            continue;
        };
        let all_features = format!("cargo test -p {package} --all-features");
        let enabled = features_enabled_by_members(&members, package);
        for test in tests {
            let Some(required) = test
                .get("required-features")
                .and_then(toml::Value::as_array)
            else {
                continue;
            };
            let in_workspace_run = required
                .iter()
                .filter_map(toml::Value::as_str)
                .all(|feature| enabled.iter().any(|on| on == feature));
            if !in_workspace_run && !runs.contains(&all_features.as_str()) {
                let name = test
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("?");
                unrun.push(format!("{package}'s `{name}`"));
            }
        }
    }
    assert!(
        unrun.is_empty(),
        "CI's workspace job never runs these feature-gated test groups (add `cargo test -p <crate> \
         --all-features`): {}",
        unrun.join(", ")
    );
}

#[test]
fn the_umbrella_runs_and_is_linted_with_every_feature() {
    let ci = ci();
    let runs = runs(get(get(&ci, "jobs"), "rust"));
    for step in [
        "cargo test -p rayx --all-features",
        "cargo clippy -p rayx --all-features --all-targets -- -D warnings",
    ] {
        assert!(runs.contains(&step), "the workspace job runs `{step}`");
    }
}
