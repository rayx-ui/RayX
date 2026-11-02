//! Release automation: release-plz's configuration and workflow, which tag the release dist
//! builds.

use std::fs;
use std::path::PathBuf;

use serde_yaml_ng::Value as Yaml;

/// The repository's root, where release-plz's configuration and the workflows are.
fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repository().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn release_plz() -> toml::Table {
    read("release-plz.toml")
        .parse()
        .expect("release-plz.toml parses")
}

fn workflow() -> Yaml {
    serde_yaml_ng::from_str(&read(".github/workflows/release-plz.yml"))
        .expect("release-plz.yml parses")
}

fn get<'a>(value: &'a Yaml, key: &str) -> &'a Yaml {
    value
        .get(key)
        .unwrap_or_else(|| panic!("missing key `{key}`"))
}

fn steps(job: &Yaml) -> &Vec<Yaml> {
    get(job, "steps").as_sequence().expect("steps")
}

fn step_using<'a>(job: &'a Yaml, action_prefix: &str) -> &'a Yaml {
    steps(job)
        .iter()
        .find(|step| {
            step.get("uses")
                .and_then(Yaml::as_str)
                .is_some_and(|uses| uses.starts_with(action_prefix))
        })
        .unwrap_or_else(|| panic!("no step uses {action_prefix}"))
}

#[test]
fn release_plz_is_git_only_unpublished_and_leaves_the_github_release_to_dist() {
    let config = release_plz();
    let workspace = config["workspace"].as_table().expect("[workspace]");

    assert_eq!(workspace["git_only"].as_bool(), Some(true));
    assert_eq!(workspace["git_tag_enable"].as_bool(), Some(true));
    assert_eq!(workspace["git_tag_name"].as_str(), Some("v{{ version }}"));
    assert_eq!(workspace["git_release_enable"].as_bool(), Some(false));
    assert_eq!(workspace["changelog_update"].as_bool(), Some(true));
    assert_eq!(workspace["semver_check"].as_bool(), Some(false));
    assert_eq!(workspace["publish"].as_bool(), Some(false));
    assert_eq!(
        workspace["release_commits"].as_str(),
        Some("^(feat|fix|perf)"),
        "only feat, fix and perf commits start a release"
    );

    let packages = config["package"].as_array().expect("[[package]]");
    let package = |name: &str| {
        packages
            .iter()
            .find(|package| package["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("release-plz.toml has no [[package]] {name}"))
    };
    // `rayx_app` owns the tag and the changelog, which lists rayx-cli's commits too.
    let owner = package("rayx_app");
    assert_eq!(owner["changelog_path"].as_str(), Some("./CHANGELOG.md"));
    assert!(
        owner["changelog_include"]
            .as_array()
            .expect("changelog_include")
            .iter()
            .any(|name| name.as_str() == Some("rayx-cli")),
        "the changelog includes rayx-cli's commits"
    );
    // rayx-cli rides along under the one version and tag.
    let cli = package("rayx-cli");
    assert_eq!(cli["version_group"].as_str(), Some("rayx"));
    assert_eq!(cli["git_tag_enable"].as_bool(), Some(false));
    assert_eq!(cli["git_release_enable"].as_bool(), Some(false));
    assert_eq!(cli["changelog_update"].as_bool(), Some(false));
    for package in packages {
        assert_eq!(
            package["version_group"].as_str(),
            Some("rayx"),
            "{} shares the one version",
            package["name"]
        );
    }
}

#[test]
fn changelog_groups_commit_types_and_drops_housekeeping() {
    let config = release_plz();
    let changelog = config["changelog"].as_table().expect("[changelog]");

    let body = changelog["body"].as_str().expect("body");
    assert!(
        body.contains("## [{{ version }}] - {{ timestamp | date(format=\"%Y-%m-%d\") }}"),
        "release headings are `## [X.Y.Z] - YYYY-MM-DD`, the form dist's changelog parser reads"
    );

    let parsers = changelog["commit_parsers"].as_array().expect("parsers");
    let skipped: Vec<&str> = parsers
        .iter()
        .filter(|p| p.get("skip").and_then(toml::Value::as_bool) == Some(true))
        .filter_map(|p| p["message"].as_str())
        .collect();
    assert!(skipped.contains(&"^chore\\(agentx\\)"), "{skipped:?}");
    assert!(skipped.contains(&"^chore\\(release\\)"), "{skipped:?}");

    let groups: Vec<&str> = parsers
        .iter()
        .filter_map(|p| p.get("group").and_then(toml::Value::as_str))
        .collect();
    for group in ["Features", "Fixes", "Documentation"] {
        assert!(groups.contains(&group), "missing group {group}: {groups:?}");
    }
}

#[test]
fn the_tag_form_agrees_with_dist_s_trigger() {
    let config = release_plz();
    let tag = config["workspace"]["git_tag_name"].as_str().expect("tag");
    assert_eq!(tag, "v{{ version }}");

    // dist's tag trigger accepts `v1.2.3` (the pattern is `**[0-9]+.[0-9]+.[0-9]+*`).
    let release = read(".github/workflows/release.yml");
    assert!(release.contains("- '**[0-9]+.[0-9]+.[0-9]+*'"));

    // The release module (the guest installer and `self update --release`) builds its URLs from
    // the same `v<version>` tag; `tests/release_module.rs` checks them.
}

#[test]
fn workflow_runs_on_main_pushes_in_the_rayx_repository_only() {
    let workflow = workflow();
    let on = get(&workflow, "on");
    let branches = get(get(on, "push"), "branches")
        .as_sequence()
        .expect("push branches");
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].as_str(), Some("main"));
    assert!(on.get("workflow_dispatch").is_some());
    assert!(
        on.get("pull_request").is_none(),
        "ci.yml owns pull requests"
    );

    let jobs = get(&workflow, "jobs");
    for name in ["release", "release-pr"] {
        let job = get(jobs, name);
        assert_eq!(
            get(job, "if").as_str(),
            Some("github.repository == 'rayx-ui/RayX'"),
            "{name} runs in rayx-ui/RayX only"
        );
        assert_eq!(
            get(get(job, "concurrency"), "cancel-in-progress").as_bool(),
            Some(false),
            "{name} never cancels a running release"
        );
    }
}

#[test]
fn both_jobs_check_out_full_history_and_use_the_release_plz_token() {
    let workflow = workflow();
    let jobs = get(&workflow, "jobs");
    let token = "${{ secrets.RELEASE_PLZ_TOKEN }}";

    for (name, command) in [("release", "release"), ("release-pr", "release-pr")] {
        let job = get(jobs, name);

        let checkout = step_using(job, "actions/checkout@");
        assert_eq!(get(get(checkout, "with"), "fetch-depth").as_i64(), Some(0));
        assert_eq!(get(get(checkout, "with"), "token").as_str(), Some(token));

        let action = step_using(job, "release-plz/action@v");
        assert_eq!(get(get(action, "with"), "command").as_str(), Some(command));
        assert_eq!(
            get(get(action, "env"), "GITHUB_TOKEN").as_str(),
            Some(token)
        );

        step_using(job, "actions-rust-lang/setup-rust-toolchain@");
    }

    let permissions = get(get(jobs, "release-pr"), "permissions");
    assert_eq!(get(permissions, "pull-requests").as_str(), Some("write"));
}

#[test]
fn release_plz_token_is_the_only_secret_and_actions_are_pinned() {
    let text = read(".github/workflows/release-plz.yml");

    let mut secrets: Vec<&str> = text
        .match_indices("secrets.")
        .map(|(at, _)| {
            let rest = &text[at + "secrets.".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();
    secrets.sort_unstable();
    secrets.dedup();
    assert_eq!(secrets, ["RELEASE_PLZ_TOKEN"]);

    let workflow = workflow();
    for job in get(&workflow, "jobs").as_mapping().expect("jobs").values() {
        for step in steps(job) {
            let uses = step.get("uses").and_then(Yaml::as_str).expect("uses");
            let (_, version) = uses.split_once('@').expect("pinned action");
            assert!(
                version.starts_with('v') || version.len() == 40,
                "{uses} is pinned to a version or commit"
            );
        }
    }
}

#[test]
fn the_changelog_header_names_rayx_cli_and_starts_changelog_md() {
    let header = release_plz()["changelog"]["header"]
        .as_str()
        .expect("changelog header")
        .to_string();
    assert!(header.starts_with("# Changelog"), "{header}");
    assert!(
        header.contains("`rayx-cli` (the `rayx` command-line tool)"),
        "{header}"
    );
    assert!(
        read("CHANGELOG.md").starts_with(header.trim_end()),
        "CHANGELOG.md starts with the configured header"
    );
}
