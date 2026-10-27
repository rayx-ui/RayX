# Releasing RayX

A release is a version tag `vX.Y.Z` on `main` plus a GitHub Release whose notes are that version's
`CHANGELOG.md` section. The three crates, `rayx_app`, `rayx_core` and `rayx_app_macros`, share
one version, one tag, one GitHub Release and one root `CHANGELOG.md`; `rayx_core` and
`rayx_app_macros` never get a tag or a Release of their own. Nothing is published to crates.io
(`publish = false`).

release-plz does all of it (`release-plz.toml`, `.github/workflows/release-plz.yml`) in git-only
mode (`git_only = true`): versions come from `v*` tags, not from a registry.

- [How a release happens](#how-a-release-happens)
- [Commit types and the 0.x policy](#commit-types-and-the-0x-policy)
- [Before merging a release pull request](#before-merging-a-release-pull-request)
- [The changelog](#the-changelog)
- [CI on a release pull request](#ci-on-a-release-pull-request)
- [The `RELEASE_PLZ_TOKEN` secret](#the-release_plz_token-secret)
- [Tags never move](#tags-never-move)
- [By-hand fallback](#by-hand-fallback)
- [Consuming a release](#consuming-a-release)
- [Pushing to `main`](#pushing-to-main)

## How a release happens

1. Commit to `main` with [Conventional Commits](https://www.conventionalcommits.org/). The commit
   types decide the next version (see below).
2. On every push to `main` the Release-plz workflow's `release-pr` job opens or updates the
   release pull request (head branch `release-plz-…`): the `[workspace.package]` version bump in
   `Cargo.toml`, `Cargo.lock` and a new `CHANGELOG.md` section.
3. Review it (see [before merging](#before-merging-a-release-pull-request)) and merge it.
4. On that push the `release` job finds a version in `Cargo.toml` with no tag, pushes `vX.Y.Z` and
   creates the GitHub Release with the new changelog section as its notes.

`v0.4.0`, the first release, was tagged by hand: it renamed the crates of `nsh-app` `v0.3.0`, and
in git-only mode release-plz looks a package up by its current name inside the previous tag, so it
cannot release a rename. From `v0.4.0` on it finds the crates by name; its first run releases
nothing.

A later crate rename releases through release-plz with a package-level `git_tag_name` that matches
no tag, such as `git_tag_name = "<crate>-v{{ version }}"` on the renamed crate's `[[package]]` in
`release-plz.toml`: the crate then counts as a first release, while the version group keeps it on
the others' version. Remove the line in the first commit after the release's tag, or every later
release pull request counts the crate as new again and repeats its history. `rayx_app_di` became
`rayx_core` this way in `v0.5.0`.

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

Migration: Gamify: replace use_screen_scope with use_scope in the shell and sportsbook slices
```

The changelog prints each `Migration:` footer under its entry, so the migration reaches the GitHub
Release notes. `semver_check = false`: breaking changes are declared by their commits, and
cargo-semver-checks would build NSH for both the tag and `main` on every run.

## Before merging a release pull request

- The proposed version is the expected one: a minor bump only when a commit since the last tag is
  breaking.
- The pull request changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- The changelog section reads well (see below).

## The changelog

`CHANGELOG.md` is written by release-plz from the commits (git-cliff, the `[changelog]` section of
`release-plz.toml`): no hand edits in feature commits. A line that reads badly is fixed on the
release pull request right before merging it. Do it last: when `main` moves on, release-plz
replaces a release pull request that carries human commits.

The `0.4.0` section is hand-written; generated sections start with the next version. The file's
header must stay identical to `[changelog] header`.

## CI on a release pull request

A release pull request runs only `ci.yml`, like any other pull request: one `ubuntu-24.04` job with
`cargo fmt --all --check`, Clippy, `cargo test --workspace` and the `wasm32-unknown-unknown` check
of `rayx_core`. `ci.yml` also runs on every push to `main`; `release-plz.yml` runs no tests.

## The `RELEASE_PLZ_TOKEN` secret

Events made with the workflow's default `GITHUB_TOKEN` start no other workflows, so a release pull
request opened with it would get no CI. release-plz therefore checks out and pushes with a
fine-grained personal access token stored as the repository's Actions secret `RELEASE_PLZ_TOKEN`.
The crates never read it.

- resource owner `rayx-ui`;
- repository access: RayX;
- repository permissions: **Contents** read and write, **Pull requests** read and write (Metadata
  read is added automatically);
- an expiry of at most one year, with the date noted.

Release pull requests, tags, Releases and release-plz's commits appear as the token's owner.

**When it expires, nothing fails loudly:** the Release-plz runs turn red at checkout or push, no
release pull request is updated and no tag is pushed; releases silently stop until the token is
replaced. Rotate it before the expiry date with the same settings; nothing else changes.

## Tags never move

A release tag is never updated, deleted or force-moved: consumers' `Cargo.lock` files record its
commit. A bad release is fixed forward with a new version.

## By-hand fallback

If release-plz cannot cut a release (a broken workflow, an expired token you cannot replace yet), the owner releases by hand on an up-to-date `main` (tags are pushed only by
release-plz or the owner):

```bash
# 1. Set X.Y.Z in [workspace.package] version of the root Cargo.toml, then:
cargo update --workspace
# 2. Add a `## [X.Y.Z] - YYYY-MM-DD` section to CHANGELOG.md, right below the header
# 3. Commit and push it (see "Pushing to main"), then tag that commit and push the tag
git tag -a vX.Y.Z -m vX.Y.Z
git push origin vX.Y.Z
# 4. Save that section's body to notes.md and create the GitHub Release with it
gh release create vX.Y.Z --repo rayx-ui/RayX --title vX.Y.Z --notes-file notes.md
```

release-plz continues from the by-hand tag on its next run.

## Consuming a release

Consumers, Gamify first among them, depend on a release tag, never on a branch, a commit, a path or
crates.io:

```toml
[dependencies]
rayx_core = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" } # service crates
rayx_app = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }    # NSH bridge crates
```

`Cargo.lock` records the exact commit. Bump the tag by hand to take a new release; the latest one is
on the repository's Releases page. A change Gamify needs is released here first; then Gamify bumps
its tag and validates its affected slices.

## Pushing to `main`

Work goes straight to `main`, always rebased on the remote first:

```bash
git fetch origin && git rebase origin/main
git push --force-with-lease origin main
```

`--force-with-lease` refuses to overwrite commits you have not fetched, so a merged release pull
request or a tagged commit is never dropped. There are no feature branches and no pull requests
except release-plz's release pull requests.
