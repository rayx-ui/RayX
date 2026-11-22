//! The one NSH pin: the default nsh `rayx` names (`project::pins::DEFAULTS.nsh`, which `rayx new`
//! patches a new app to) is the NSH RayX's own `[patch.crates-io]` names, in the workspace's root
//! `Cargo.toml`, at the commit its `Cargo.lock` holds. Moving one moves the other, or this fails.
//!
//! While `rayx nshx use <checkout>` points RayX at a local NSH checkout, its patch entries are
//! paths and its lock takes NSH by path, with no source. Then this checks what holds on a
//! checkout instead: every crate of the default pin, and nothing else, patched from one NSH
//! checkout, each the crate that checkout holds under its name, and the lock taking NSH's crates
//! by path at one version. `rayx nshx use <tag>` moves RayX back onto a tag, where the full
//! repository, tag, commit and version check applies again.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rayx_cli::nsh::pin::is_nsh_crate;
use rayx_cli::nsh::switch::checkout_crates;
use rayx_cli::project::pins::DEFAULTS;

/// What a failure about the kind of a patch entry adds: the two commands move every entry at once.
const LOCAL: &str = " (`rayx nshx use <checkout>` moves every patch entry to a local NSH checkout \
     and `rayx nshx use <tag>` every one back to a tag)";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> toml::Table {
    let path = root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .parse()
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn patch(manifest: &toml::Table) -> &toml::Table {
    manifest["patch"]["crates-io"]
        .as_table()
        .expect("RayX's [patch.crates-io]")
}

/// Whether RayX's patch takes NSH from a local checkout (`rayx nshx use <checkout>`): an entry
/// names a `path`.
fn on_local_checkout(patch: &toml::Table) -> bool {
    patch.values().any(|spec| spec.get("path").is_some())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The NSH checkout that holds `folder`: its nearest ancestor with `crates/nsh-cli`.
fn checkout_of(folder: &Path) -> PathBuf {
    folder
        .ancestors()
        .find(|dir| dir.join("crates/nsh-cli").is_dir())
        .unwrap_or_else(|| panic!("{} is in no NSH checkout", folder.display()))
        .to_path_buf()
}

#[test]
fn the_default_pin_patches_what_rayx_patches_at_the_same_tag() {
    let manifest = read("Cargo.toml");
    let patch = patch(&manifest);
    let local = on_local_checkout(patch);
    let mut patched = BTreeSet::new();
    let mut checkouts = BTreeSet::new();
    for (name, spec) in patch {
        if local {
            let keys: Vec<&str> = spec
                .as_table()
                .expect("a patch entry")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                keys,
                ["path"],
                "RayX patches {name} from the local checkout by path alone{LOCAL}"
            );
            let folder = canonical(&root().join(spec["path"].as_str().expect("a path")));
            let checkout = checkout_of(&folder);
            assert_eq!(
                checkout_crates(&checkout)
                    .get(name)
                    .map(|path| canonical(path)),
                Some(folder.clone()),
                "RayX patches {name} from the crate {} holds under that name",
                checkout.display()
            );
            checkouts.insert(checkout);
        } else {
            assert_eq!(
                spec["git"].as_str(),
                Some(DEFAULTS.nsh.git),
                "RayX patches {name} from the default's repository{LOCAL}"
            );
            assert_eq!(
                spec["tag"].as_str(),
                Some(DEFAULTS.nsh.tag),
                "RayX patches {name} to the default's tag"
            );
        }
        patched.insert(name.as_str());
    }
    if local {
        assert_eq!(
            checkouts.len(),
            1,
            "RayX patches NSH from one checkout: {checkouts:?}"
        );
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
    let local = on_local_checkout(patch(&read("Cargo.toml")));
    let lock = read("Cargo.lock");
    let source = format!(
        "git+{}?tag={}#{}",
        DEFAULTS.nsh.git, DEFAULTS.nsh.tag, DEFAULTS.nsh.commit
    );
    let mut locked = BTreeSet::new();
    let mut versions = BTreeSet::new();
    for package in lock["package"].as_array().expect("[[package]]") {
        let name = package["name"].as_str().expect("name");
        if !is_nsh_crate(name) {
            continue;
        }
        let version = package["version"].as_str().expect("version");
        if local {
            assert_eq!(
                package.get("source").and_then(|source| source.as_str()),
                None,
                "RayX's lock takes {name} from the local checkout by path"
            );
            versions.insert(version.to_string());
        } else {
            assert_eq!(
                package["source"].as_str(),
                Some(source.as_str()),
                "RayX's lock takes {name} from the default pin's commit{LOCAL}"
            );
            assert_eq!(
                version, DEFAULTS.nsh.version,
                "{name} has the default pin's version"
            );
        }
        locked.insert(name.to_string());
    }
    if local {
        assert_eq!(
            versions.len(),
            1,
            "RayX's lock takes NSH's crates from one checkout at one version: {versions:?}"
        );
    }
    for name in ["nsh-core", "nsh-base"] {
        assert!(
            locked.contains(name),
            "RayX's lock holds {name}: {locked:?}"
        );
    }
}
