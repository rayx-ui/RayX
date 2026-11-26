//! The workspace's crates as `cargo metadata` sees them: their names, their folders and the rules
//! of what each may depend on. Each check fails naming every package or dependency that differs.
//!
//! The rules: the packages are exactly the final set (none of the old names, [`OLD_NAMES`]); every
//! package lives in `crates/<package>/` (the CLI in `crates/rayx_cli`, the gallery in
//! `examples/gallery/native`); the core, `rayx_app`, depends on no NSH crate and on no RayX crate
//! but `rayx_app_macros`; the NSH host, `rayx_app_nsh`, builds on the core and depends on no
//! service crate; every service crate builds on the core and depends on no other service crate;
//! the umbrella, `rayx`, depends on every service crate, each optional.

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
    /// The direct dependencies that are optional.
    optional: BTreeSet<String>,
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
    // The manifests' folders are relative to the root cargo reports, not to `root`: on Windows
    // `canonicalize` gives `\\?\D:\…`, and cargo's paths start with `D:\…`.
    let cargo_root = PathBuf::from(metadata["workspace_root"].as_str().unwrap());
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            let manifest = Path::new(package["manifest_path"].as_str().unwrap());
            let folder = manifest
                .parent()
                .unwrap()
                .strip_prefix(&cargo_root)
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
            let optional = package["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|dependency| {
                    dependency["kind"].as_str() != Some("dev")
                        && dependency["optional"].as_bool() == Some(true)
                })
                .map(|dependency| dependency["name"].as_str().unwrap().to_owned())
                .collect();
            let name = package["name"].as_str().unwrap().to_owned();
            (
                name,
                Package {
                    folder,
                    dependencies,
                    optional,
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

/// The service crates, by the name after `rayx_services_`.
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

/// Whether a dependency is one of RayX's crates.
fn is_rayx(name: &str) -> bool {
    name == "rayx" || name.starts_with("rayx_") || name.starts_with("rayx-")
}

#[test]
fn the_packages_are_exactly_the_final_set() {
    let mut expected: BTreeSet<String> = [
        "rayx",
        "rayx_app",
        "rayx_app_macros",
        "rayx_app_nsh",
        "rayx_components",
        "rayx_devtools_protocol",
        "rayx-cli",
        "rayx-gallery",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    expected.extend(SERVICES.map(|name| format!("rayx_services_{name}")));
    let actual: BTreeSet<String> = packages().into_keys().collect();
    let mut problems: Vec<String> = actual
        .difference(&expected)
        .map(|name| format!("unexpected package {name}"))
        .collect();
    problems.extend(
        expected
            .difference(&actual)
            .map(|name| format!("missing package {name}")),
    );
    assert_none("the workspace's packages", problems);
}

#[test]
fn the_core_depends_on_no_rayx_crate_but_its_macros() {
    let packages = packages();
    let problems = packages["rayx_app"]
        .dependencies
        .iter()
        .filter(|dependency| is_rayx(dependency) && *dependency != "rayx_app_macros")
        .map(|dependency| format!("rayx_app depends on {dependency}"))
        .collect();
    assert_none("the core's RayX dependencies", problems);
}

#[test]
fn the_nsh_host_depends_on_no_service_crate() {
    let packages = packages();
    let problems = packages["rayx_app_nsh"]
        .dependencies
        .iter()
        .filter(|dependency| dependency.starts_with("rayx_services_"))
        .map(|dependency| format!("rayx_app_nsh depends on {dependency}"))
        .collect();
    assert_none("the host's service dependencies", problems);
}

#[test]
fn every_service_crate_builds_on_the_core_and_on_no_other_service_crate() {
    let packages = packages();
    let mut problems = Vec::new();
    for name in SERVICES {
        let crate_name = format!("rayx_services_{name}");
        let Some(package) = packages.get(&crate_name) else {
            problems.push(format!("no package {crate_name}"));
            continue;
        };
        if !package.dependencies.contains("rayx_app") {
            problems.push(format!("{crate_name} does not depend on rayx_app"));
        }
        problems.extend(
            package
                .dependencies
                .iter()
                .filter(|dependency| dependency.starts_with("rayx_services_"))
                .map(|dependency| format!("{crate_name} depends on {dependency}")),
        );
    }
    assert_none("the service crates' dependencies", problems);
}

#[test]
fn the_umbrella_depends_on_every_service_crate_optionally() {
    let packages = packages();
    let umbrella = &packages["rayx"];
    let problems = SERVICES
        .map(|name| format!("rayx_services_{name}"))
        .into_iter()
        .filter(|crate_name| !umbrella.optional.contains(crate_name))
        .map(|crate_name| format!("rayx does not depend on {crate_name} optionally"))
        .collect();
    assert_none("the umbrella's service dependencies", problems);
}

/// The packages' former names: the core's before it was `rayx_app`, and the crate the services
/// lived in before each became a crate of its own. Spelled in parts, so a search of the docs and
/// sources for the old names finds none.
const OLD_NAMES: [&str; 2] = [concat!("rayx", "_core"), concat!("rayx", "_runtime")];

#[test]
fn no_package_keeps_an_old_name() {
    let packages = packages();
    let problems = OLD_NAMES
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
