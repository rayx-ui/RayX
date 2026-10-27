# Changelog

All notable changes to `rayx_app`, `rayx_app_di` and `rayx_app_macros` are documented in this
file. The three crates share one version and one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

## [0.5.0] - 2026-10-27

### Build and CI
- *(nsh)* [**breaking**] Pin NSH to fork-0.3.0-1
  - Migration: Gamify: patch NSH to `fork-0.3.0-1` (or a newer `fork-0.3.0-*` tag) in `[patch.crates-io]`, depend on the NSH crates at `0.3.0`, and move its own NSH imports to NSH 0.3's modules (`nsh_core::ui::*` to `nsh_core::prelude` or `nsh_core::element`, TSX `nsh/device` and `nsh/animated` to `nsh`, `Input` to `nsh/base`).

### Features
- *(core)* [**breaking**] Add configuration sources, .NET's options model and RayXAppBuilder
  - Migration: Gamify: register options with `services_mut().add_options::<T>()` (`.bind("Section")`, `.configure(..)`, `.validate(..)`) or `services_mut().configure::<T>(move |o| *o = options.clone())` instead of `configuration_mut().options_value(options)`, and read them with `options::<T>()?` instead of `required_options::<T>()?`.
  - Migration: Gamify: replace the test builders that implement `AppBuilder` and call `build_options_sync` and `with_added_options` with `RayXAppBuilder::new()` and `.build()?`, and rename `NshAppBuilder` to `RayXAppBuilder` (`set_app` takes `fn(RayXAppBuilder) -> RayXAppBuilder`).

### Refactoring
- *(core)* [**breaking**] Rename rayx_app_di to rayx_core
  - Migration: Gamify: depend on `rayx_core` instead of `rayx_app_di` (same tag) and rename the `rayx_app_di::` paths to `rayx_core::`, including the xtask's dependency check.

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
