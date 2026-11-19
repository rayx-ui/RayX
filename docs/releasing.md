# Releasing RayX

A release is a version tag `vX.Y.Z` on `main` plus a GitHub Release whose notes are that version's
`CHANGELOG.md` section and whose assets are the `rayx` command-line tool for six targets, with its
installers. The crates, `rayx` (the umbrella), `rayx_app` (the core), `rayx_app_nsh` (the NSH host), the twelve
service crates (`rayx_services_<name>`), `rayx_components`, `rayx_devtools_protocol`, `rayx_app_macros` and `rayx-cli`
(the `rayx` binary), share one version, one tag, one GitHub Release and one root `CHANGELOG.md`; none of them gets a tag or a Release of its own. Nothing is
published to crates.io (`publish = false`).

Two tools split the work:

- **release-plz** (`release-plz.toml`, `.github/workflows/release-plz.yml`), in git-only mode
  (`git_only = true`: versions come from `v*` tags, not from a registry), turns Conventional
  Commits into a release pull request and, when that pull request is merged, pushes the tag. It
  creates no GitHub Release (`git_release_enable = false`).
- **cargo-dist** (`.github/workflows/release.yml`, generated from `dist-workspace.toml`) starts on
  the tag, builds `rayx-cli` for the six targets, writes the checksums, attestations and installers,
  and creates the GitHub Release with the notes from the matching `CHANGELOG.md` section.

- [How a release happens](#how-a-release-happens)
- [Commit types and the 0.x policy](#commit-types-and-the-0x-policy)
- [Before merging a release pull request](#before-merging-a-release-pull-request)
- [The changelog](#the-changelog)
- [CI on a release pull request](#ci-on-a-release-pull-request)
- [The `RELEASE_PLZ_TOKEN` secret](#the-release_plz_token-secret)
- [Tags never move](#tags-never-move)
- [By-hand fallback](#by-hand-fallback)
- [Checking a release](#checking-a-release)
- [Consuming a release](#consuming-a-release)
- [Pushing to `main`](#pushing-to-main)

## How a release happens

1. Commit to `main` with [Conventional Commits](https://www.conventionalcommits.org/). The commit
   types decide the next version (see below).
2. When you want a release, start the Release-plz workflow by hand (Actions > Release-plz > Run
   workflow, or `gh workflow run release-plz.yml`). Its `release-pr` job opens or updates the
   release pull request (head branch `release-plz-…`): the `[workspace.package]` version bump in
   `Cargo.toml`, `Cargo.lock` and a new `CHANGELOG.md` section, covering every commit since the
   last tag. Pushes to `main` no longer touch it, so several pushes share one release, one release
   pull request and one CI run. The binaries are not built on the pull request
   (`pr-run-mode = "skip"`): the tag's Release run builds them.
3. Review it (see [before merging](#before-merging-a-release-pull-request)) and merge it.
4. On that push the `release` job finds a version in `Cargo.toml` with no tag and pushes `vX.Y.Z`.
5. The tag starts dist's Release workflow. When it is green the GitHub Release exists, with the new
   changelog section as its notes and the archives, checksums, `rayx-cli-installer.sh`,
   `rayx-cli-installer.ps1` and `sha256.sum` as its assets.

`v0.4.0`, the first release, was tagged by hand: it renamed the crates of `nsh-app` `v0.3.0`, and
in git-only mode release-plz looks a package up by its current name inside the previous tag, so it
cannot release a rename. From `v0.4.0` on it finds the crates by name; its first run releases
nothing.

A later crate rename releases through release-plz with a package-level `git_tag_name` that matches
no tag, such as `git_tag_name = "<crate>-v{{ version }}"` on the renamed crate's `[[package]]` in
`release-plz.toml`: the crate then counts as a first release, while the version group keeps it on
the others' version. Remove the line in the first commit after the release's tag, or every later
release pull request counts the crate as new again and repeats its history. `rayx_app_di` became
`rayx_core` this way in `v0.5.0`. A new crate takes the same route: `rayx_runtime` and `rayx`, which
the `v0.5.0` tag does not have, carried the line until `v0.5.1`, and `rayx-cli`, which moved here
from `rayx-ui/RayX.CLI` (never released there), carried `git_tag_name = "rayx-cli-v{{ version }}"`
until `v0.6.0`. `rayx_components` and `rayx_devtools_protocol` carried the line until
`v0.11.0`. `rayx_app_nsh`, the NSH host's name after `v0.11.0` (where it was
`rayx_app`), and the `rayx_services_*` crates carried it until `v0.12.0`. `rayx_services_theme`, new after
`v0.12.0`, carried it until `v0.13.0` (`release_automation.rs`' `NEW_SINCE_THE_LAST_RELEASE` names
such a crate meanwhile). A crate that is never
released (`release = false`, such as `rayx-gallery`)
still needs it, and keeps it: release-plz looks every workspace crate up in the previous tag.

A `Migration:` footer may wrap over several lines: the changelog template reads it from the commit's
raw message and joins its lines (the footer parser alone drops some continuation lines, such as
those that start with inline code).

## Commit types and the 0.x policy

| Commit | Release |
| --- | --- |
| `fix:`, `perf:`, `feat:` | patch (`0.4.0` → `0.4.1`) |
| breaking: `!` after the type or scope, or a `BREAKING CHANGE:` footer | minor (`0.4.x` → `0.5.0`) |
| `docs:`, `test:`, `ci:`, `build:`, `chore:`, `refactor:` | none on their own (`release_commits = "^(feat\|fix\|perf)"`); they are listed in the next release's changelog when they touch a crate |

`chore(agentx)`, `chore(release)` and `chore(deps)` commits are left out of the changelog.

A breaking commit's body states the break, and a `Migration:` footer per affected consumer tells it
what to change:

```text
feat(screen)!: Name the screen scope hook use_scope

`use_screen_scope` is gone; `use_scope` does the same.

Migration: apps: replace use_screen_scope with use_scope where a screen makes its scope
```

The changelog prints each `Migration:` footer under its entry, so the migration reaches the GitHub
Release notes. `semver_check = false`: breaking changes are declared by their commits, and
cargo-semver-checks would build NSH for both the tag and `main` on every run.

## Before merging a release pull request

- The proposed version is the expected one: a minor bump only when a commit since the last tag is
  breaking.
- The pull request changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- The changelog section reads well (see below).
- Its CI is green. The binaries for the six targets are built only by the tag's Release run (step
  5); if one fails there, fix it and tag the fix as the next patch release.

## The changelog

`CHANGELOG.md` is written by release-plz from the commits (git-cliff, the `[changelog]` section of
`release-plz.toml`): no hand edits in feature commits. A line that reads badly is fixed on the
release pull request right before merging it. Do it last: when `main` moves on, release-plz
replaces a release pull request that carries human commits.

The `0.4.0` section is hand-written; generated sections start with the next version. The file's
header must stay identical to `[changelog] header`.

## CI on a release pull request

A release pull request runs `ci.yml`, like any other pull request: one `ubuntu-24.04` job with
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and the `wasm32-unknown-unknown`
check of the NSH-free crates (`cargo check -p rayx_app -p rayx_devtools_protocol --target wasm32-unknown-unknown`),
and the `rayx-cli` job, which formats, lints and tests the CLI on
Linux, macOS and Windows. dist's Release workflow does not run on pull requests
(`pr-run-mode = "skip"`); it runs on the release tag. `ci.yml` runs on `main` every three hours,
and only when `main` moved since its last scheduled run (`changed.yml`), never on each push: one
run covers every push since the last one and keeps the caches pull requests restore warm. Start it
by hand (`gh workflow run ci.yml`) to check `main` sooner. `release-plz.yml` runs no tests.

## The `RELEASE_PLZ_TOKEN` secret

Events made with the workflow's default `GITHUB_TOKEN` start no other workflows, so a release pull
request opened with it would get no CI and the tag it pushed would not start the Release workflow.
release-plz therefore checks out and pushes with a fine-grained personal access token stored as the
repository's Actions secret `RELEASE_PLZ_TOKEN`. Neither the crates nor `rayx` ever read it.

- resource owner `rayx-ui`;
- repository access: RayX;
- repository permissions: **Contents** read and write, **Pull requests** read and write (Metadata
  read is added automatically);
- an expiry of at most one year, with the date noted.

Release pull requests, tags, Releases and release-plz's commits appear as the token's owner.

**When it expires, nothing fails loudly:** the Release-plz runs turn red at checkout or push, no
release pull request is updated and no tag is pushed; releases silently stop until the token is
replaced. Rotate before the expiry date: generate a new token with the same settings and replace
the secret's value. Nothing else changes.

## Tags never move

A release tag is never updated, deleted or force-moved: consumers' `Cargo.lock` files record its
commit. A bad release is fixed forward with a new version.

## By-hand fallback

If release-plz cannot cut a release (a broken workflow, an expired token you cannot replace yet),
the owner releases by hand on an up-to-date `main` (tags are pushed only by release-plz or the
owner):

```bash
# 1. Set X.Y.Z in [workspace.package] version of the root Cargo.toml, then:
cargo update --workspace
# 2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to CHANGELOG.md, right below the header
# 3. Commit and push it (see "Pushing to main"), then tag that commit and push the tag
git tag -a vX.Y.Z -m vX.Y.Z
git push origin vX.Y.Z
```

The tag starts the Release workflow exactly as release-plz's tag does, and dist creates the GitHub
Release. Tags always have the form `vX.Y.Z`, because dist's trigger, the WSL and Lima guests' installs
(`rayx setup --wsl`, `--lima`) and `rayx self update` all read it. release-plz continues from the
by-hand tag on its next run.

## Checking a release

```bash
gh release view vX.Y.Z --repo rayx-ui/RayX
gh release download vX.Y.Z --repo rayx-ui/RayX --pattern 'rayx-cli-aarch64-apple-darwin.tar.xz'
gh attestation verify rayx-cli-aarch64-apple-darwin.tar.xz --repo rayx-ui/RayX
```

Each archive has a `.sha256` file next to it, and `sha256.sum` lists all of them. The shell
installer verifies the download against its checksum; the CLI's
[README](../crates/rayx_cli/README.md#install) has the install one-liners.

macOS binaries and Windows executables are not signed or notarized yet; Gatekeeper only quarantines
archives downloaded by hand in a browser, not the installers' downloads.

## Consuming a release

Consumers depend on a release tag, never on a branch, a commit, a path or
crates.io, and install the `rayx` of the same release:

```toml
[dependencies]
# Apps: the umbrella, with a prelude; the default features are the four services the host requires.
rayx = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z", features = ["storage", "i18n"] }
# Crates of NSH-free services and contracts:
rayx_app = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }
# Crates with an NSH side, and a single RayX service without the umbrella:
rayx_app_nsh = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }
rayx_services_storage = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }
```

`Cargo.lock` records the exact commit. Bump the tag by hand to take a new release; the latest one is
on the repository's Releases page. A change a consumer needs is released here first; then the
consumer bumps its tag and validates what the change touches.

## Pushing to `main`

Work goes straight to `main`, always rebased on the remote first:

```bash
git fetch origin && git rebase origin/main
git push --force-with-lease origin main
```

`--force-with-lease` refuses to overwrite commits you have not fetched, so a merged release pull
request or a tagged commit is never dropped. There are no feature branches and no pull requests
except release-plz's release pull requests.
