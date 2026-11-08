//! A RayX app made by `rayx new` checked and tested with rayx's default nsh (PROOF-2), ignored by
//! default: it installs that nsh's release into a temporary `RAYX_HOME` (about 1.4 GB) and builds
//! the app with `rayx` at this `rayx`'s release tag, which needs the network and that tag. It
//! proves the app's `.cargo/config.toml` patches every NSH crate nsh's generated workspace depends
//! on (`DEFAULTS.nsh.generated`).
//!
//! ```text
//! cargo test -p rayx-cli --test proof_app_rayx -- --ignored --test-threads=1
//! ```

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::Workspace;
use common::proof::rayx_ok;
use rayx_cli::project::pins::DEFAULTS;

/// The NSH crates the manifests under `dir` depend on (`nsh-<name> = …` lines), the app's own
/// crates (`nsh-app-*`) left out.
fn nsh_dependencies_under(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            nsh_dependencies_under(&path, out);
        } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            let text = std::fs::read_to_string(&path).expect("manifest");
            out.extend(
                text.lines()
                    .filter_map(|line| line.split_once(" = ").map(|(name, _)| name.trim()))
                    .filter(|name| name.starts_with("nsh-") && !name.starts_with("nsh-app-"))
                    .map(str::to_string),
            );
        }
    }
}

#[test]
#[ignore = "installs nsh and builds a RayX app (network, many minutes)"]
fn a_new_rayx_app_checks_and_runs_its_tests_with_every_nsh_crate_patched() {
    let home = Workspace::empty();
    let work = Workspace::empty();
    rayx_ok(&home.root, &work.root, &["new", "hello"]);
    let app = work.path("hello");

    rayx_ok(&home.root, &app, &["app", "check"]);
    let mut used = BTreeSet::new();
    nsh_dependencies_under(&app.join(".nsh/rust"), &mut used);
    assert!(used.contains("nsh-tui-host"), "{used:?}");
    let patched: BTreeSet<String> = DEFAULTS
        .nsh
        .crates
        .iter()
        .chain(DEFAULTS.nsh.generated)
        .map(|name| name.to_string())
        .collect();
    let unpatched: Vec<&String> = used.difference(&patched).collect();
    assert!(
        unpatched.is_empty(),
        "nsh's generated workspace depends on NSH crates the app does not patch: {unpatched:?}"
    );

    rayx_ok(&home.root, &app, &["app", "test"]);
}
