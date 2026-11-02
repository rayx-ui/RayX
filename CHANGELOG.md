# Changelog

All notable changes to `rayx`, `rayx_app`, `rayx_core`, `rayx_runtime`, `rayx_app_macros` and
`rayx-cli` (the `rayx` command-line tool) are documented in this file. They share one version and
one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

## [0.6.0] - 2026-11-02

### Build and CI
- Check rayx-cli on Linux, macOS and Windows
- *(nsh)* Pin NSH to fork-0.3.0-3, in RayX and as rayx's default

### Documentation
- *(cli)* Fold RayX.CLI's contributor rules and decisions into RayX's docs

### Features
- *(cli)* Import RayX.CLI as the rayx-cli crate
- *(cli)* Ship rayx with RayX's releases, on RayX's version
- *(cli)* Install and update rayx from rayx-ui/RayX
- *(cli)* Make rayx's default NSH pin RayX's own
- *(cli)* Scaffold RayX apps with rayx new
- *(cli)* Serve index.html for an app's own routes in rayx serve
- *(runtime)* [**breaking**] Chain add_selection like the other add_* methods
  - Migration: an app that registers the selection service drops the `?` (or `.unwrap()`) after `add_selection()`.
- *(runtime)* Keep history in the app's key-value store
- *(runtime)* [**breaking**] Give navigation one location for all its regions, taken in from the platform
  - Migration: a `NavigationLocationSynchronizer` takes `synchronize(&self, location: &str, mode)` and may answer `location()` with the platform's location at startup.
- *(rayx)* Put rayx_app::services in the prelude
- *(app)* Keep the app's key-value store in NSH's storage
- *(app)* Give TSX the navigation, storage, configuration, asset, font and platform services
- *(app)* Bind dXn.Reactive view models to NSH signals and TSX classes
- *(app)* Show navigation regions as NSH navigation stacks, with a TSX class

### Fixes
- *(macros)* Let #[derive(AppService)] name the rayx umbrella

### Refactoring
- *(core)* [**breaking**] Remove ShutdownService and the AppScope, WindowScope and ViewScope wrappers
  - Migration: a service that implemented `ShutdownService` releases what it holds in `Drop` instead.

### Tests
- *(cli)* Give the Lima check test a df of its own

## [0.5.1] - 2026-10-29

### Build and CI
- *(runtime)* Add the rayx_runtime crate for RayX's services

### Features
- *(runtime)* Add the operation service
- *(runtime)* Add the history service
- *(runtime)* Add the in-memory key-value storage service
- *(runtime)* Add the navigation service
- *(runtime)* Add the selection service
- *(runtime)* Add the input map service
- *(runtime)* Add the platform service
- *(runtime)* Add the suspension service
- *(runtime)* Add the application configuration service
- *(runtime)* Add the asset service
- *(runtime)* Add the font service
- *(runtime)* Add the i18n service
- *(core)* Let services keep change listeners
- *(rayx)* Add the rayx umbrella crate with a prelude
- *(app)* Run RayX's services on NSH
- *(app)* Give TSX the i18n service

## [0.5.0] - 2026-10-27

### Build and CI
- *(nsh)* [**breaking**] Pin NSH to fork-0.3.0-1
  - Migration: patch NSH to `fork-0.3.0-1` (or a newer `fork-0.3.0-*` tag) in `[patch.crates-io]`, depend on the NSH crates at `0.3.0`, and move the app's own NSH imports to NSH 0.3's modules (`nsh_core::ui::*` to `nsh_core::prelude` or `nsh_core::element`, TSX `nsh/device` and `nsh/animated` to `nsh`, `Input` to `nsh/base`).

### Features
- *(core)* [**breaking**] Add configuration sources, .NET's options model and RayXAppBuilder
  - Migration: register options with `services_mut().add_options::<T>()` (`.bind("Section")`, `.configure(..)`, `.validate(..)`) or `services_mut().configure::<T>(move |o| *o = options.clone())` instead of `configuration_mut().options_value(options)`, and read them with `options::<T>()?` instead of `required_options::<T>()?`.
  - Migration: replace the test builders that implement `AppBuilder` and call `build_options_sync` and `with_added_options` with `RayXAppBuilder::new()` and `.build()?`, and rename `NshAppBuilder` to `RayXAppBuilder` (`set_app` takes `fn(RayXAppBuilder) -> RayXAppBuilder`).

### Refactoring
- *(core)* [**breaking**] Rename rayx_app_di to rayx_core
  - Migration: depend on `rayx_core` instead of `rayx_app_di` (same tag) and rename the `rayx_app_di::` paths to `rayx_core::`, including any dependency checks.

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
- The workspace builds with the NSH fork's tag `fork-0.2.0-4`; the NSH layer
  test builds its engine config with `EngineConfig::new`, which that tag added.

### Migration

- Coming from `nsh-app` `v0.3.0`: depend on `rayx_app_di`/`rayx_app` from
  `{ git = "https://github.com/rayx-ui/RayX", tag = "v0.4.0" }` instead of `nsh_app_di`/`nsh_app`,
  rename their paths (`nsh_app_di::` to `rayx_app_di::`, `nsh_app::` to `rayx_app::`), and move to
  `dxn_reactive` `v1.1.0`.
