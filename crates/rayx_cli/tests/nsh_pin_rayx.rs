//! The one NSH pin: the default nsh `rayx` names (`project::pins::DEFAULTS.nsh`, which `rayx new`
//! patches a new app to) is the NSH RayX's own `[patch.crates-io]` names, in the workspace's root
//! `Cargo.toml`, at the commit its `Cargo.lock` holds. Moving one moves the other, or this fails.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rayx_cli::nsh::pin::is_nsh_crate;
use rayx_cli::project::pins::DEFAULTS;

fn read(relative: &str) -> toml::Table {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .parse()
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_default_pin_patches_what_rayx_patches_at_the_same_tag() {
    let manifest = read("Cargo.toml");
    let patch = manifest["patch"]["crates-io"]
        .as_table()
        .expect("RayX's [patch.crates-io]");
    let mut patched = BTreeSet::new();
    for (name, spec) in patch {
        assert_eq!(
            spec["git"].as_str(),
            Some(DEFAULTS.nsh.git),
            "RayX patches {name} from the default's repository"
        );
        assert_eq!(
            spec["tag"].as_str(),
            Some(DEFAULTS.nsh.tag),
            "RayX patches {name} to the default's tag"
        );
        patched.insert(name.as_str());
    }
    let default: BTreeSet<&str> = DEFAULTS.nsh.crates.iter().copied().collect();
    assert_eq!(
        default, patched,
        "the default pin's crates are RayX's patched crates"
    );
    assert_eq!(default.len(), DEFAULTS.nsh.crates.len(), "each crate once");
}

#[test]
fn the_default_pin_is_the_commit_and_version_rayx_locks() {
    let lock = read("Cargo.lock");
    let source = format!(
        "git+{}?tag={}#{}",
        DEFAULTS.nsh.git, DEFAULTS.nsh.tag, DEFAULTS.nsh.commit
    );
    let mut locked = BTreeSet::new();
    for package in lock["package"].as_array().expect("[[package]]") {
        let name = package["name"].as_str().expect("name");
        if !is_nsh_crate(name) {
            continue;
        }
        assert_eq!(
            package["source"].as_str(),
            Some(source.as_str()),
            "RayX's lock takes {name} from the default pin's commit"
        );
        assert_eq!(
            package["version"].as_str(),
            Some(DEFAULTS.nsh.version),
            "{name} has the default pin's version"
        );
        locked.insert(name.to_string());
    }
    for name in ["nsh-core", "nsh-base"] {
        assert!(
            locked.contains(name),
            "RayX's lock holds {name}: {locked:?}"
        );
    }
}
