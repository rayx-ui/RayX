//! RayX's 0.x release policy, which `rayx` follows since it ships with RayX: `fix`, `perf` and
//! `feat` bump the patch, a breaking change (`!` or a `BREAKING CHANGE:` footer) bumps the minor,
//! and a breaking commit carries `Migration:` footers the changelog renders.

use std::path::PathBuf;

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn release_plz() -> toml::Table {
    read("release-plz.toml").parse().expect("release-plz.toml")
}

#[test]
fn semver_checks_stay_off_and_only_feat_fix_perf_start_a_release() {
    let config = release_plz();
    let workspace = config["workspace"].as_table().expect("[workspace]");
    assert_eq!(workspace["semver_check"].as_bool(), Some(false));
    assert_eq!(
        workspace["release_commits"].as_str(),
        Some("^(feat|fix|perf)")
    );
}

#[test]
fn the_changelog_body_prints_migration_footers_under_their_commit() {
    let config = release_plz();
    let body = config["changelog"]["body"].as_str().expect("body");
    let commit_loop = body
        .split("{% for commit in commits %}")
        .nth(1)
        .expect("the commit loop");
    let footers = commit_loop
        .find("{% for footer in commit.footers")
        .expect("a loop over the commit's footers");
    let message = commit_loop
        .rfind("{{ commit.message }}")
        .expect("the commit line");
    assert!(message < footers, "footers come after the commit line");
    let footer_loop = &commit_loop[footers..];
    assert!(
        footer_loop.contains("footer.token == \"Migration\""),
        "{footer_loop}"
    );
    assert!(
        footer_loop.contains("- Migration: {{ footer.value }}"),
        "a nested `- Migration: <value>` line: {footer_loop}"
    );
}

#[test]
fn releasing_md_states_the_0x_policy() {
    let text = flat(&read("docs/releasing.md"));
    for needle in [
        "## Commit types and the 0.x policy",
        "patch (`0.4.0` → `0.4.1`)",
        "minor (`0.4.x` → `0.5.0`)",
        "`BREAKING CHANGE:` footer",
        "a `Migration:` footer per affected consumer",
    ] {
        assert!(text.contains(needle), "docs/releasing.md lacks `{needle}`");
    }
    assert!(
        !text.contains("The 1.x policy"),
        "the CLI's own 1.x policy is gone"
    );
}

#[test]
fn claude_and_agents_state_the_policy_in_a_releases_paragraph() {
    assert_eq!(
        read("CLAUDE.md").trim(),
        "@AGENTS.md",
        "CLAUDE.md includes AGENTS.md"
    );
    let agents = read("AGENTS.md");
    let releases = agents
        .split("\n## Releases\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .map(flat)
        .expect("a Releases section");
    for needle in [
        "one tag `vX.Y.Z`",
        "cargo-dist builds the `rayx` binaries",
        "0.x policy",
        "→ minor",
        "`Migration:` footer",
        "owner's go-ahead",
    ] {
        assert!(releases.contains(needle), "Releases lacks `{needle}`");
    }
}

/// The text of the `## <title>` section of `docs/releasing.md`, flattened.
fn releasing_section(title: &str) -> String {
    let releasing = read("docs/releasing.md");
    let heading = format!("\n## {title}\n");
    let section = releasing
        .split_once(&heading)
        .map(|(_, rest)| rest.split("\n## ").next().unwrap_or(rest))
        .unwrap_or_else(|| panic!("docs/releasing.md has a `## {title}` section"));
    flat(section)
}

#[test]
fn releasing_md_covers_the_first_release_of_rayx_cli() {
    let section = releasing_section("How a release happens");
    for needle in [
        "builds the `rayx` binaries for all six targets without publishing them",
        "The tag starts dist's Release workflow",
        "`git_tag_name = \"rayx-cli-v{{ version }}\"`",
        "until its first release",
        "Remove the line in the first commit after the release's tag",
    ] {
        assert!(
            section.contains(needle),
            "the release section lacks `{needle}`"
        );
    }
    // The line is there until that release, and release-plz.toml says to remove it.
    let config = release_plz();
    let cli = config["package"]
        .as_array()
        .expect("[[package]]")
        .iter()
        .find(|package| package["name"].as_str() == Some("rayx-cli"))
        .expect("rayx-cli");
    if let Some(name) = cli.get("git_tag_name").and_then(toml::Value::as_str) {
        assert_eq!(name, "rayx-cli-v{{ version }}");
        assert!(
            read("release-plz.toml").contains("Remove the line right after that release's tag")
        );
    }
}

#[test]
fn releasing_md_covers_the_token_the_fallback_and_checking_a_release() {
    let token = releasing_section("The `RELEASE_PLZ_TOKEN` secret");
    for needle in [
        "fine-grained personal access token",
        "repository access: RayX",
        "**Contents** read and write",
        "**Pull requests** read and write",
        "the tag it pushed would not start the Release workflow",
        "Rotate before the expiry date",
        "Neither the crates nor `rayx` ever read it",
    ] {
        assert!(token.contains(needle), "the token section lacks `{needle}`");
    }
    let fallback = releasing_section("By-hand fallback");
    for needle in [
        "git tag -a vX.Y.Z",
        "git push origin vX.Y.Z",
        "`## [X.Y.Z] - YYYY-MM-DD`",
        "dist creates the GitHub Release",
    ] {
        assert!(
            fallback.contains(needle),
            "the fallback section lacks `{needle}`"
        );
    }
    let checking = releasing_section("Checking a release");
    for needle in [
        "gh release view vX.Y.Z --repo rayx-ui/RayX",
        "gh attestation verify",
        "`sha256.sum`",
    ] {
        assert!(
            checking.contains(needle),
            "the checking section lacks `{needle}`"
        );
    }
}
