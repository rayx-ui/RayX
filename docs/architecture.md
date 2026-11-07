# Architecture

RayX is a set of crates released together under one version. An app depends on the umbrella `rayx`; a service crate
depends on `rayx_core` alone.

| Crate | What it holds | NSH |
| --- | --- | --- |
| `rayx_core` | Dependency injection (singleton, scoped and transient services, scopes, async startup), configuration sources, .NET's options model and the `RayXAppBuilder` | no |
| `rayx_runtime` | The app services: storage, operations and history, navigation, input maps, platform and lifecycle state, app configuration, assets, fonts, translations | no |
| `rayx_app` | The NSH layer: a root provider per surface, a scope per screen, view-model activation and binding, navigation regions, the TSX classes, and the NSH side of the services | yes |
| `rayx_app_macros` | `#[derive(AppService)]`, re-exported by `rayx_core`, `rayx_app` and `rayx` | no |
| `rayx` | The umbrella: re-exports the other crates and dXn.I18n and dXn.Intl, with a prelude | yes |
| `rayx-cli` | The `rayx` command-line tool | no (runs the `nsh` CLI) |

## Two layers

The core and the services have no NSH dependency. They build for every target, `wasm32-unknown-unknown` included, and
service crates stay testable with plain `cargo test`. What a service needs from the platform is a trait in
`rayx_runtime` that `rayx_app` implements on NSH: reading the app's assets, registering fonts, the device's state, the
bundled `appsettings.json`.

`rayx_app` is the only crate that depends on `nsh-core`. It ties the core's lifetimes to what NSH runs (see
[Dependency injection](dependency-injection.md#lifetimes-on-nsh)) and gives TSX its classes (see [TSX](tsx.md)).

## The umbrella

`rayx` re-exports:

- `rayx::core`: `rayx_core`
- `rayx::runtime`: `rayx_runtime`
- `rayx::app`: `rayx_app`
- `rayx::i18n` and `rayx::intl`: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
  [dXn.Intl](https://github.com/dXnSix/dXn.Intl), for their message, number and date types

`use rayx::prelude::*;` brings the builder and its extensions, the hooks and the TSX classes.

## Dependencies

- View-model state comes from [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) (`dxn_reactive`); translations
  use MessageFormat 2 messages from dXn.I18n and dXn.Intl.
- NSH comes from an integration tag `nshx-*` of the fork [dXnSix/nsh](https://github.com/dXnSix/nsh), through
  `[patch.crates-io]` in `Cargo.toml`. A consumer's own patch decides which NSH it builds with.
- Every dependency is a git tag: no path, branch or commit dependencies.

## Releases

release-plz releases every crate under one version and one tag `vX.Y.Z`, and cargo-dist builds the `rayx` binaries for
that tag. See [Releasing RayX](releasing.md).
