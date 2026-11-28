//! The umbrella's features as `cargo metadata` and `cargo tree` see them: one per service crate,
//! named after the service, each enabling exactly its crate and implying no other feature; the
//! defaults are the four services the host requires (`platform`, `suspension`, `assets`,
//! `fonts`); and a feature off keeps its crate out of the umbrella's build. Each check fails
//! naming every feature that differs.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The service crates, by the name after `rayx_services_`: the feature names.
const SERVICES: [&str; 14] = [
    "application_configuration",
    "assets",
    "commands",
    "fonts",
    "history",
    "i18n",
    "input_map",
    "navigation",
    "operations",
    "platform",
    "selection",
    "storage",
    "suspension",
    "theme",
];

/// The services the host requires: the default features.
const DEFAULTS: [&str; 4] = ["platform", "suspension", "assets", "fonts"];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Runs cargo in the workspace and returns its standard output.
fn cargo(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO"))
        .args(args)
        .current_dir(workspace())
        .output()
        .expect("cargo");
    assert!(
        output.status.success(),
        "cargo {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The umbrella's `[features]`, from `cargo metadata --no-deps`.
fn features() -> BTreeMap<String, Vec<String>> {
    let metadata: serde_json::Value = serde_json::from_str(&cargo(&[
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--offline",
    ]))
    .unwrap();
    let umbrella = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "rayx")
        .expect("package rayx");
    umbrella["features"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, enables)| {
            let enables = enables
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect();
            (name.clone(), enables)
        })
        .collect()
}

/// The umbrella's direct normal dependencies with `--no-default-features` and `features` on,
/// from `cargo tree`: what its build compiles.
fn dependencies_with(features: &[&str]) -> BTreeSet<String> {
    let features = features.join(",");
    let mut args = vec![
        "tree",
        "-p",
        "rayx",
        "--no-default-features",
        "-e",
        "normal",
        "--depth",
        "1",
        "--prefix",
        "none",
        "--format",
        "{p}",
        "--offline",
    ];
    if !features.is_empty() {
        args.extend(["--features", &features]);
    }
    cargo(&args)
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// Fails with every line of `problems`.
fn assert_none(what: &str, problems: Vec<String>) {
    assert!(problems.is_empty(), "{what}:\n{}", problems.join("\n"));
}

#[test]
fn there_is_one_feature_per_service_crate_named_after_it() {
    let names: BTreeSet<String> = features()
        .into_keys()
        .filter(|name| name != "default")
        .collect();
    let expected: BTreeSet<String> = SERVICES.map(str::to_owned).into_iter().collect();
    let mut problems: Vec<String> = names
        .difference(&expected)
        .map(|name| format!("unexpected feature {name}"))
        .collect();
    problems.extend(
        expected
            .difference(&names)
            .map(|name| format!("missing feature {name}")),
    );
    assert_none("the umbrella's features", problems);
}

#[test]
fn each_feature_enables_exactly_its_crate() {
    let features = features();
    let problems = SERVICES
        .into_iter()
        .filter_map(|name| {
            let expected = vec![format!("dep:rayx_services_{name}")];
            match features.get(name) {
                Some(enables) if *enables == expected => None,
                Some(enables) => Some(format!("{name} enables {enables:?}, not {expected:?}")),
                None => Some(format!("no feature {name}")),
            }
        })
        .collect();
    assert_none("what each feature enables", problems);
}

#[test]
fn no_feature_implies_another() {
    let features = features();
    let problems = features
        .iter()
        .filter(|(name, _)| *name != "default")
        .flat_map(|(name, enables)| {
            enables
                .iter()
                .filter(|enabled| !enabled.starts_with("dep:"))
                .map(move |enabled| format!("{name} implies {enabled}"))
        })
        .collect();
    assert_none("features implying features", problems);
}

#[test]
fn the_defaults_are_the_four_services_the_host_requires() {
    let defaults: BTreeSet<String> = features()["default"].iter().cloned().collect();
    let expected: BTreeSet<String> = DEFAULTS.map(str::to_owned).into_iter().collect();
    assert_eq!(defaults, expected);
}

#[test]
fn a_feature_off_keeps_its_crate_out_of_the_build() {
    let base = dependencies_with(&[]);
    let mut problems: Vec<String> = base
        .iter()
        .filter(|dependency| dependency.starts_with("rayx_services_"))
        .map(|dependency| format!("no features: {dependency} is built"))
        .collect();
    for name in SERVICES {
        let added: BTreeSet<String> = dependencies_with(&[name])
            .difference(&base)
            .cloned()
            .collect();
        let expected = BTreeSet::from([format!("rayx_services_{name}")]);
        if added != expected {
            problems.push(format!("{name} adds {added:?}, not {expected:?}"));
        }
    }
    let added: BTreeSet<String> = dependencies_with(&["default"])
        .difference(&base)
        .cloned()
        .collect();
    let expected: BTreeSet<String> = DEFAULTS
        .map(|name| format!("rayx_services_{name}"))
        .into_iter()
        .collect();
    if added != expected {
        problems.push(format!("the defaults add {added:?}, not {expected:?}"));
    }
    assert_none("the umbrella's build per feature", problems);
}
