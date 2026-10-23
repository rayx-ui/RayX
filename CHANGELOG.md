# Changelog

All notable changes to `rayx_app`, `rayx_app_di` and `rayx_app_macros` are documented in this
file. The three crates share one version and one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

## [0.4.0] - 2026-10-23

The first release of RayX on [Native Shell (NSH)](https://github.com/dXnSix/nsh). RayX moves from
GPUI and gpux to NSH; the GPUI-based RayX is now `RayX.Old`. The workspace starts from the crates of
`nsh-app` `v0.3.0` (`19bd4c4`), renamed with the repository, and its version continues
`nsh-app`'s.

### Added

- `rayx_app_di` (was `nsh_app_di`): the pure-Rust core with RayX's DI API: a `ServiceProvider` with
  singleton, scoped and transient services, scopes, options, configuration and the `AppBuilder`,
  and startup services initialized asynchronously in dependency order, each within its timeout. It
  has no NSH dependency and builds for every target, wasm32 included.
- `rayx_app` (was `nsh_app`): the NSH layer: one root provider per engine surface (`set_app`,
  `install`), startup from TSX (`AppServices.load()`) or a Rust root (`start`, `app_state`), a scope
  per screen (`use_screen_scope`, `ServiceScope`) and view models activated with their screen
  (`use_view_model`).
- `rayx_app_macros` (was `nsh_app_macros`): `#[derive(AppService)]`, re-exported by both layers.

### Changed

- `rayx_app` activates view models with `dxn_reactive` `v1.1.0` (formerly `rayx_reactive`) and
  re-exports its `ActivatableViewModel`, `ActivationToken` and `ViewModelActivator`.
- The workspace builds with the NSH fork's tag `fork-0.2.0-4`, the one Gamify uses; the NSH layer
  test builds its engine config with `EngineConfig::new`, which that tag added.

### Migration

- Coming from `nsh-app` `v0.3.0`: depend on `rayx_app_di`/`rayx_app` from
  `{ git = "https://github.com/rayx-ui/RayX", tag = "v0.4.0" }` instead of `nsh_app_di`/`nsh_app`,
  rename their paths (`nsh_app_di::` to `rayx_app_di::`, `nsh_app::` to `rayx_app::`), and move to
  `dxn_reactive` `v1.1.0`.
