# RayX contributor guidance

RayX is an app framework for [Native Shell (NSH)](https://github.com/dXnSix/nsh) apps. It replaces
the GPUI and gpux stack of the previous RayX, now `RayX.Old` (`../RayX.Old`), which is not a
dependency. The workspace has four crates sharing one version:

- `crates/rayx_core`: the pure-Rust core: dependency injection (singleton, scoped and transient
  services, scopes, async startup), configuration sources, .NET's options model and the
  `RayXAppBuilder`. It has no NSH dependency and must keep building for `wasm32-unknown-unknown`;
  service crates depend on it alone.
- `crates/rayx_runtime`: RayX's app services (storage, operations and history, navigation, input
  maps, platform and lifecycle state, app configuration, assets, fonts), ported from RayX.Old's
  `rayx_runtime`. It depends on `rayx_core` alone, has no NSH dependency and builds for
  `wasm32-unknown-unknown`; what a service needs from the platform is a trait here that
  `rayx_app` implements. `docs/services.md` lists what each service ports and leaves out.
- `crates/rayx_app`: the NSH layer: a root provider per engine surface, a scope per screen, view
  models activated with their screen, the `AppServices`/`ServiceScope` classes for TSX, and the
  NSH side of the services (bundled settings, asset reading, font registration, the device's
  state). It is the only crate that depends on `nsh-core`.
- `crates/rayx_app_macros`: the `#[derive(AppService)]` proc macro, re-exported by both layers.

## Dependencies

- `dxn_reactive` comes from [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) by release tag.
- NSH comes from the fork `github.com/dXnSix/nsh` through `[patch.crates-io]`, pinned to one of
  its integration tags `fork-*` (the newest RayX needs); a consumer's own patch decides which NSH
  it builds with.
- Git tags only: never path, branch or commit dependencies.
- GPUI, gpux and RayX.Old's crates (its `rayx`, `rayx_runtime`, `rayx_collab`, …) must not appear
  in the dependency tree; this workspace's `rayx_runtime` is a port, not a dependency on them.

## Validation

The toolchain is `rust-toolchain.toml`'s (Rust 1.99.0 with the wasm32 target). Run before
committing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p rayx_core -p rayx_runtime --target wasm32-unknown-unknown
```

CI (`.github/workflows/ci.yml`) runs the same on every pull request and every push to `main`.

Test only what you changed: run the touched crate's focused tests while iterating, and the full
list above once before committing. Each feature has its own integration-test group in
`crates/<crate>/tests/<feature>.rs` (`di_provider`, `di_startup`, `nsh_layer`, …); select one with
`cargo test -p <crate> --test <feature>`. A new feature or behavior change gets a new group.

Raw diagnostic output goes to the ignored `artifacts-temp/`, never into commits.

## Consumers

Consumers, the sportsbook demo Gamify (`../../Six/Six.Gamify`) first among them, depend on release
tags only:
`rayx_app = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }` (and `rayx_core` from the
same tag for service crates). A change Gamify needs is released here first; then Gamify bumps its
tag by hand and validates its affected slices.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) with meaningful scopes:
`<type>(<scope>): <Sentence>`, for example `feat(di): …`, `fix(screen): …`, `ci(release): …`.
Never add Claude co-author or other AI attribution lines.

## Releases

release-plz releases the three crates under one version and one tag `vX.Y.Z`; see
[docs/releasing.md](docs/releasing.md).

- Conventional Commits drive the version. 0.x policy: `fix`, `perf` and `feat` → patch; a breaking
  change (`!` after the type or scope, or a `BREAKING CHANGE:` footer) → minor; other types ride
  along in the next release.
- A breaking commit's body states the break and a `Migration:` footer tells each affected consumer
  what to change (`Migration: Gamify: …`).
- `CHANGELOG.md` is written by release-plz; do not edit it in feature commits.
- A release is the merge of release-plz's release pull request, after checking that it proposes
  the expected version and changes only `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md`.
- Tags are pushed only by release-plz or the owner, and never moved or deleted.
- Push straight to `main`: `git fetch origin && git rebase origin/main`, then
  `git push --force-with-lease origin main`. No feature branches or pull requests except release
  pull requests. Pushing and tagging need the owner's go-ahead.
