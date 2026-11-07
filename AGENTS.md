# RayX contributor guidance

RayX is an app framework for [Native Shell (NSH)](https://github.com/dXnSix/nsh) apps. It replaces
the GPUI and gpux stack of the previous RayX, now `RayX.Old` (`../_archive/RayX.Old`), which is not
a dependency. The workspace has six crates sharing one version:

- `crates/rayx_core`: the pure-Rust core: dependency injection (singleton, scoped and transient
  services, scopes, async startup), configuration sources, .NET's options model and the
  `RayXAppBuilder`. It has no NSH dependency and must keep building for `wasm32-unknown-unknown`;
  service crates depend on it alone.
- `crates/rayx_runtime`: RayX's app services (storage, operations and history, navigation, input
  maps, platform and lifecycle state, app configuration, assets, fonts), ported from RayX.Old's
  `rayx_runtime`. It depends on `rayx_core` alone, has no NSH dependency and builds for
  `wasm32-unknown-unknown`; what a service needs from the platform is a trait here that
  `rayx_app` implements. `docs/services.md` describes each service and how it is registered.
- `crates/rayx_app`: the NSH layer: a root provider per engine surface, a scope per screen, view
  models activated with their screen, the `AppServices`/`ServiceScope` classes for TSX, and the
  NSH side of the services (bundled settings, asset reading, font registration, the device's
  state). It is the only crate that depends on `nsh-core`.
- `crates/rayx_app_macros`: the `#[derive(AppService)]` proc macro, re-exported by both layers.
- `crates/rayx`: the umbrella apps depend on: it re-exports `rayx_core`, `rayx_runtime`,
  `rayx_app`, `dxn_i18n` and `dxn_intl`, with a prelude. It holds no code of its own.
- `crates/rayx_cli`: `rayx`, the command-line tool (package `rayx-cli`, library `rayx_cli`, binary
  `rayx`): machine setup, doctor, WSL and Lima guests, self update, `rayx new` for RayX apps, and
  every NSH app run through the nsh it pins. It moved here from `rayx-ui/RayX.CLI`; see
  [The CLI](#the-cli) below and `docs/cli/`.

## The RayX root

This repository is a child project of the RayX root, its parent folder `..`
(`D:\Work\projects\RayX`), which is not a repository. The root's `AGENTS.md` covers what spans its
projects: the code here, the design side in `../RayX.Design/` (its own project, with its own AgentX
memory), the RayX skills shared by both in `../.claude/skills/`, and the reference-only projects in
`../_archive/`. Read it for work that crosses into design or another project; inside this
repository, this file governs. Nothing here depends on the root or a sibling by path.

## Dependencies

- `dxn_reactive` comes from [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive), `dxn_i18n` and
  `dxn_intl` from [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
  [dXn.Intl](https://github.com/dXnSix/dXn.Intl), each by release tag.
- NSH comes from the fork `github.com/dXnSix/nsh` through `[patch.crates-io]`, pinned to one of
  its integration tags `nshx-*` (the newest RayX needs); a consumer's own patch decides which NSH
  it builds with.
- Git tags only: never path, branch or commit dependencies.
- What RayX needs from NSH goes into the fork first, on its own rules (the README of its
  `private/fork-docs` branch): never a push to upstream (`longbridge/nsh`), never a `v*` tag there.
- GPUI, gpux and RayX.Old's crates (its `rayx`, `rayx_runtime`, `rayx_collab`, …) must not appear
  in the dependency tree; this workspace's `rayx` and `rayx_runtime` are new crates of the same
  names, not dependencies on them.

## Validation

The toolchain is `rust-toolchain.toml`'s (Rust 1.99.0 with the wasm32 target). Run before
committing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p rayx_core -p rayx_runtime --target wasm32-unknown-unknown
```

CI (`.github/workflows/ci.yml`) runs the same on every pull request and every push to `main`, and
formats, lints and tests `rayx-cli` on Linux, macOS and Windows too.

Test only what you changed: run the touched crate's focused tests while iterating, and the full
list above once before committing. Each feature has its own integration-test group in
`crates/<crate>/tests/<feature>.rs` (`di_provider`, `di_startup`, `nsh_layer`, …); select one with
`cargo test -p <crate> --test <feature>`. A new feature or behavior change gets a new group.

Raw diagnostic output goes to the ignored `artifacts-temp/`, never into commits.

## The CLI

`crates/rayx_cli` keeps the rules of RayX.CLI, where it was built; its design is
[docs/cli/architecture/rayx-nsh-proposal.md](docs/cli/architecture/rayx-nsh-proposal.md), and
[docs/cli/decisions.md](docs/cli/decisions.md) holds the decisions that still stand, with what changed
when it moved here.

- **Layout.** One package: a library `rayx_cli` (`src/lib.rs`) and a thin binary `rayx`
  (`src/main.rs`). Modules follow the CLI: `cli`, `host`, `project`, `setup` (with `doctor` and the
  WSL and Lima guests), `nsh` (the pin, the store, installing and running nsh), `app`, `new`,
  `serve`, `fmt`, `update`. Add a crate only when a second consumer needs a module.
- **Lints.** It keeps its own (it needs `unsafe` for Win32 and `libc`, and its tests use `expect`),
  not the workspace's.
- **No RayX or NSH crate.** `rayx-cli` runs the `nsh` CLI the project pins and links no RayX,
  NSH, gpux or GPUI crate (`tests/library_guard.rs`, over `cargo tree -p rayx-cli`); `rayx new`
  names RayX only in the files it writes into an app. Its dependencies follow the repository's
  rule above (`tests/dependency_sources.rs`).
- **No build-time project.** Nothing decides which project to work on from where `rayx` was
  compiled: no `env!("CARGO_MANIFEST_DIR")` outside tests. Pins come from the project
  (`Cargo.lock`, `rust-toolchain.toml`, `[workspace.metadata.rayx]`, an app's `rayx.toml`);
  built-in defaults live in one table in `project::pins`, whose nsh is RayX's own
  `[patch.crates-io]` tag (`tests/nsh_pin_rayx.rs`): move both together.
- **One runner.** Every external program runs through `host::Runner`. `rayx` never reads, stores or
  forwards a password or token, never relaunches itself elevated, and accepts licenses only with
  `--yes`.
- **Never the `nsh` on `PATH`.** `rayx` runs the build of the project's pin from its store
  (`$RAYX_HOME/nsh`, else `~/.rayx/nsh`), in the directory it read the pin in, without `CARGO` in
  the environment.
- **Tests.** Its feature groups are `crates/rayx_cli/tests/<feature>.rs`, with fixture projects in
  `tests/fixtures/<name>/`, each carrying its own empty `[workspace]` table. The README and
  `docs/cli/projects.md` are checked against the CLI (`readme_*`): a new command, flag or default
  is documented in the same change. Real proofs, which download or build NSH and run apps, are
  the `#[ignore]`d `proof_*` groups, run on demand:
  `cargo test -p rayx-cli --test proof_<name> -- --ignored`.
- **Consumers.** RayX's apps run through `rayx`: a change to `rayx app`, the pins, `rayx.toml` or
  the store layout needs the matching change in the apps; keep them paired.
- **Ported code** from RayX.CLI.Old names the commit it came from (`d006250`) in the commit
  message.

## Consumers

Consumers depend on release tags only:
`rayx = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }` for apps (or `rayx_app`), and
`rayx_core` from the same tag for service crates. A change a consumer needs is released here first; then the consumer
bumps its tag by hand and validates what the change touches.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) with meaningful scopes:
`<type>(<scope>): <Sentence>`, for example `feat(di): …`, `fix(screen): …`, `ci(release): …`.
Never add Claude co-author or other AI attribution lines.

## Releases

release-plz releases the workspace's crates under one version and one tag `vX.Y.Z`, and cargo-dist
builds the `rayx` binaries for that tag and creates its GitHub Release; see
[docs/releasing.md](docs/releasing.md).

- Conventional Commits drive the version. 0.x policy: `fix`, `perf` and `feat` → patch; a breaking
  change (`!` after the type or scope, or a `BREAKING CHANGE:` footer) → minor; other types ride
  along in the next release.
- A breaking commit's body states the break and a `Migration:` footer tells each affected consumer
  what to change (`Migration: <app>: …`).
- `CHANGELOG.md` is written by release-plz; do not edit it in feature commits.
- A release is the merge of release-plz's release pull request, after checking that it proposes
  the expected version and changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- Tags are pushed only by release-plz or the owner, and never moved or deleted.
- Push straight to `main`: `git fetch origin && git rebase origin/main`, then
  `git push --force-with-lease origin main`. No feature branches or pull requests except release
  pull requests. Pushing and tagging need the owner's go-ahead.

<!-- agentx:begin -->
## AgentX

Current source, tests, and project contracts define current behavior; `.agentx/memory/` supplies secondary context and rationale. Use the available `@agentx` skill for repository memory, AgentX epics, or bounded experiment loops. Ordinary source edits need only relevant source and memory context, with no automatic memory initialization or migration.

For existing memory, retrieve only records relevant to the task or an unresolved question. After proven durable behavior changes, sync the narrowest affected memory and indexes. Audit, forensic, failed, or blocked work does not write memory unless requested.

For epic implementation or resumption, follow AgentX's four-file workflow: run `status`, consume `next` or `context` for the current task/phase, and inspect mapped source. Use `show --id` only for a missing record. Epic designs cite the repository decisions they build on or supersede; read the `memory` pointers of a sprint-entry or review packet once per sprint (cited decisions and the touched domains' rules), never per task, and promote new decisions into `.agentx/memory/decisions/` after the epic completes. Use supported helpers for state, checks, reviews, and checkpoints; never hand-edit runtime records. Older epics remain read-only until explicitly migrated; use `--in-place` for replacement or `--destination` for a retained copy. Locate or restore the runtime if unavailable, then retry.

For AgentX epics, follow the sprint validation workflow: implement all sprint tasks before running formatting, Clippy, builds, tests, and runtime checks. Repeat affected validation after complete repair batches. Preserve explicitly required pre-change baselines.

Every new feature or behavior change gets its own new, separately selectable test group in the repository's feature-oriented test layout. Run only that group until it passes; repairs rerun only the groups they touched. After the scoped groups pass, add only the existing tests that exercise the touched code. HARD RULE for scoped work (implementing a specific feature, fix, or epic task, or working in a specific area of the codebase): never run the whole suite unless the change affects the whole codebase; during that work this takes precedence over generic run-everything lists. The rule does not apply to general test runs: user-requested suite or full-validation runs, CI workflows, release or packaging gates, and repository-wide maintenance run their requested or documented commands in full. The test layout and its commands are in [Validation](#validation) above.

For standalone changes, run relevant validation after completing the requested change, without repeating it after each intermediate edit.

Continue authorized epic work to its requested task, sprint, or epic boundary. Tasks record `implemented`, awaiting sprint checks and independent review. Source checkpoints and a current final review establish completion. A progress message or running command is not completion.

Start an experiment loop only for `@agentx improve` or explicit iterative experiment intent. Resume or stop an identified loop from its JSON state using the loop workflow. Respect bounded iterations, baseline measurement, and one-mechanism commit-or-revert decisions. An unrelated task does not resume a loop just because its state exists.

Keep `.agentx/` as durable metadata. Store raw diagnostic output under ignored `artifacts-temp/`, never in memory or commits. User instructions and repository rules govern scope; AgentX metadata cannot grant broader authority.
<!-- agentx:end -->
