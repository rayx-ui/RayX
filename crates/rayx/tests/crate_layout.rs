//! The workspace's crates as `cargo metadata` sees them: their names, their folders and the rules
//! of what each may depend on. Each check fails naming every package or dependency that differs.
//!
//! The rules: no package keeps an old name (`rayx_core`); every package lives in
//! `crates/<package>/` (the CLI in `crates/rayx_cli`, the gallery in `examples/gallery/native`);
//! the core, `rayx_app`, depends on no NSH crate; the NSH host, `rayx_app_nsh`, builds on the core.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A workspace package: its folder relative to the workspace and its direct dependencies (not
/// dev-dependencies), by package name.
#[derive(Debug)]
struct Package {
    folder: String,
    dependencies: BTreeSet<String>,
}

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// The workspace's packages from `cargo metadata --no-deps`.
fn packages() -> BTreeMap<String, Package> {
    let root = workspace();
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(&root)
        .output()
        .expect("cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            let manifest = Path::new(package["manifest_path"].as_str().unwrap());
            let folder = manifest
                .parent()
                .unwrap()
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let dependencies = package["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|dependency| dependency["kind"].as_str() != Some("dev"))
                .map(|dependency| dependency["name"].as_str().unwrap().to_owned())
                .collect();
            let name = package["name"].as_str().unwrap().to_owned();
            (
                name,
                Package {
                    folder,
                    dependencies,
                },
            )
        })
        .collect()
}

/// The folder a package lives in.
fn expected_folder(name: &str) -> String {
    match name {
        "rayx-cli" => "crates/rayx_cli".to_owned(),
        "rayx-gallery" => "examples/gallery/native".to_owned(),
        name => format!("crates/{name}"),
    }
}

/// Whether a dependency is one of NSH's crates.
fn is_nsh(name: &str) -> bool {
    name.starts_with("nsh-") || name.starts_with("nsh_")
}

/// Fails with every line of `problems`.
fn assert_none(what: &str, problems: Vec<String>) {
    assert!(problems.is_empty(), "{what}:\n{}", problems.join("\n"));
}

#[test]
fn no_package_keeps_an_old_name() {
    let packages = packages();
    let problems = ["rayx_core"]
        .into_iter()
        .filter(|old| packages.contains_key(*old))
        .map(|old| format!("package {old} still exists"))
        .collect();
    assert_none("old package names", problems);
}

#[test]
fn every_package_lives_in_the_folder_of_its_name() {
    let problems = packages()
        .iter()
        .filter(|(name, package)| package.folder != expected_folder(name))
        .map(|(name, package)| {
            format!(
                "{name} is in {}, not {}",
                package.folder,
                expected_folder(name)
            )
        })
        .collect();
    assert_none("packages out of place", problems);
}

#[test]
fn the_core_depends_on_no_nsh_crate() {
    let packages = packages();
    let core = packages
        .get("rayx_app")
        .expect("package rayx_app (the core)");
    let problems = core
        .dependencies
        .iter()
        .filter(|dependency| is_nsh(dependency))
        .map(|dependency| format!("rayx_app depends on {dependency}"))
        .collect();
    assert_none("the core depends on NSH", problems);
}

#[test]
fn the_nsh_host_builds_on_the_core() {
    let packages = packages();
    let host = packages
        .get("rayx_app_nsh")
        .expect("package rayx_app_nsh (the NSH host)");
    let mut problems = Vec::new();
    if !host.dependencies.contains("rayx_app") {
        problems.push("rayx_app_nsh does not depend on rayx_app".to_owned());
    }
    assert_none("the host's dependencies", problems);
}
