//! The host's source layout: `src/` holds exactly the files of the expected tree, grouped by
//! topic (`app/`, `di/`, `configuration/`, `devtools/`), with only what service crates build on
//! under `services/` (each service's NSH side is its crate's), and no `ts/` folder: the TS
//! classes live beside what they wrap, in files ending in `_ts.rs`, and `ts.rs` re-exports them.
//! The check names every file or folder that is missing or extra.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::Path;

/// The topic folders and the files at the root of `src/`.
const ROOT: &[&str] = &[
    "lib.rs",
    "ts.rs",
    "binding.rs",
    "testing.rs",
    "app/mod.rs",
    "app/surface.rs",
    "app/builder_ts.rs",
    "app/ready_ts.rs",
    "app/state_ts.rs",
    "di/mod.rs",
    "di/screen.rs",
    "di/screen_ts.rs",
    "di/provider_ts.rs",
    "di/container_ts.rs",
    "di/contract_ts.rs",
    "di/startup_ts.rs",
    "di/view_model_ts.rs",
    "configuration/mod.rs",
    "configuration/settings.rs",
    "configuration/options_ts.rs",
    "configuration/configuration_builder_ts.rs",
    "devtools/mod.rs",
    "devtools/act.rs",
    "devtools/agent.rs",
    "devtools/layer.rs",
    "devtools/link.rs",
    "devtools/tree.rs",
];

/// `services/`: what the service crates build on (the core's services re-exported, `json` and the
/// listeners helper); no service has a folder here.
const SERVICES: &[&str] = &["services/mod.rs", "services/listeners_ts.rs"];

/// Every file under `dir`, relative to `base`, with `/` separators. Hidden entries (a name
/// starting with `.`: Finder's `.DS_Store`, an editor's swap file) are not the layout's and are
/// skipped, as Git ignores them.
fn files(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            files(base, &path, out);
        } else {
            let relative = path.strip_prefix(base).unwrap();
            out.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn src() -> BTreeSet<String> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = BTreeSet::new();
    files(&base, &base, &mut out);
    out
}

/// The folders a set of files lives in, each with its parents.
fn folders(files: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for file in files {
        let mut path = Path::new(file).parent();
        while let Some(folder) = path.filter(|folder| !folder.as_os_str().is_empty()) {
            out.insert(folder.to_string_lossy().replace('\\', "/"));
            path = folder.parent();
        }
    }
    out
}

#[test]
fn src_holds_exactly_the_expected_files_and_folders() {
    let expected: BTreeSet<String> = ROOT
        .iter()
        .chain(SERVICES)
        .map(|file| (*file).to_owned())
        .collect();
    let actual = src();
    let mut problems = Vec::new();
    for folder in folders(&expected).difference(&folders(&actual)) {
        problems.push(format!("missing folder src/{folder}/"));
    }
    for folder in folders(&actual).difference(&folders(&expected)) {
        problems.push(format!("extra folder src/{folder}/"));
    }
    for file in expected.difference(&actual) {
        problems.push(format!("missing file src/{file}"));
    }
    for file in actual.difference(&expected) {
        problems.push(format!("extra file src/{file}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_folder_has_a_mod_rs_and_no_ts_folder_remains() {
    let actual = src();
    let mut problems = Vec::new();
    for folder in folders(&actual) {
        if !actual.contains(&format!("{folder}/mod.rs")) {
            problems.push(format!("src/{folder}/ has no mod.rs"));
        }
        if folder == "ts" || folder.ends_with("/ts") {
            problems.push(format!("src/{folder}/ remains"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
