//! `rayx new` patches the NSH crates of nsh's generated workspace too: the hosts, runners,
//! platforms and test runner nsh writes for an app's Rust crate depend on NSH crates that
//! crates.io lacks (`nsh-tui-host`, `nsh-platform-tui`) or has only as upstream's builds, so the
//! app's `.cargo/config.toml` sends them to the default pin's tag with RayX's own crates.
//! `proof_app_rayx` checks the list against what the pinned nsh writes.

use rayx_cli::new::cargo_config;
use rayx_cli::project::pins::DEFAULTS;

/// The crates a config patches, in its order.
fn patched(config: &str) -> Vec<&str> {
    config
        .lines()
        .filter(|line| line.ends_with(&format!("tag = \"{}\" }}", DEFAULTS.nsh.tag)))
        .filter_map(|line| line.split(" = ").next())
        .collect()
}

#[test]
fn the_generated_crates_are_nsh_crates_rayx_does_not_patch_already() {
    let generated = DEFAULTS.nsh.generated;
    assert!(!generated.is_empty());
    let mut sorted = generated.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, generated, "sorted, each once");
    for name in generated {
        assert!(name.starts_with("nsh-"), "{name}");
        assert!(!name.starts_with("nsh-app-"), "{name} is an app's crate");
        assert!(
            !DEFAULTS.nsh.crates.contains(name),
            "{name} is one of RayX's crates"
        );
    }
    // The TUI host crates.io lacks, which stopped a new app's `nsh check`.
    for name in ["nsh-tui-host", "nsh-platform-tui"] {
        assert!(generated.contains(&name), "{name}");
    }
}

#[test]
fn a_new_app_patches_rayx_s_and_the_generated_crates_to_the_default_tag() {
    let config = cargo_config(&DEFAULTS.nsh, &[]);
    let mut expected: Vec<&str> = DEFAULTS
        .nsh
        .crates
        .iter()
        .chain(DEFAULTS.nsh.generated)
        .copied()
        .collect();
    expected.sort_unstable();
    assert_eq!(patched(&config), expected);
    for name in &expected {
        assert!(
            config.contains(&format!(
                "\n{name} = {{ git = \"{}\", tag = \"{}\" }}\n",
                DEFAULTS.nsh.git, DEFAULTS.nsh.tag
            )),
            "{name}: {config}"
        );
    }
    assert!(
        config.contains("a build that does not use\n# them names in a warning"),
        "the header says why Cargo warns: {config}"
    );
}

#[test]
fn a_crate_the_app_names_is_patched_once_whatever_list_has_it() {
    let also = [
        "nsh-platform-macos".to_string(),
        "nsh-dev".to_string(),
        "nsh-core".to_string(),
    ];
    let config = cargo_config(&DEFAULTS.nsh, &also);
    let names = patched(&config);
    for name in ["nsh-platform-macos", "nsh-dev", "nsh-core"] {
        assert_eq!(
            names.iter().filter(|n| **n == name).count(),
            1,
            "{name} once: {config}"
        );
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "sorted");
    assert_eq!(
        names.len(),
        DEFAULTS.nsh.crates.len() + DEFAULTS.nsh.generated.len() + 1,
        "nsh-dev is the one crate neither list has"
    );
}
