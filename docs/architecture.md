# Architecture

RayX is a set of crates released together under one version. An app depends on the umbrella `rayx`; a service crate
depends on `rayx_core` alone.

| Crate | What it holds | NSH |
| --- | --- | --- |
| `rayx_core` | Dependency injection (singleton, scoped and transient services, scopes, async startup), configuration sources, .NET's options model and the `RayXAppBuilder` | no |
| `rayx_runtime` | The app services: storage, operations and history, navigation, input maps, platform and lifecycle state, app configuration, assets, fonts, translations | no |
| `rayx_app` | The NSH layer: the app's start on each surface, a root provider per surface, a scope per screen, view-model activation and binding, navigation regions, the TS side (`rayx_app::ts`: TS's `RayXAppBuilder`, its `ServiceProvider`, services written in TS, contracts and the services' classes), and the NSH side of the services | yes |
| `rayx_components` | The component library: the design system's tokens as an NSH theme document (`theme.json`, generated from the Figma export) read through typed keys, its modes as the document's modifiers, and the components (`Button`) on nsh-base, for Rust and TSX | yes |
| `rayx_devtools_protocol` | The agent protocol of the dev tools: the JSON-RPC messages between an app's dev-build agent and a tool's hub ([Dev tools](devtools.md)); serde and serde_json alone | no |
| `rayx_app_macros` | `#[derive(AppService)]`, re-exported by `rayx_core`, `rayx_app` and `rayx`; `#[app_extension]` and `#[contract]`, re-exported by `rayx_app` and `rayx`; `#[native_module]`, used as `#[rayx::native_module]` | no |
| `rayx` | The umbrella: re-exports the other crates and dXn.I18n and dXn.Intl, with a prelude, and `#[rayx::native_module]`, the attribute of a TSX app's UI set | yes |
| `rayx-cli` | The `rayx` command-line tool | no (runs the `nsh` CLI) |

## Two layers

The core and the services have no NSH dependency. They build for every target, `wasm32-unknown-unknown` included, and
service crates stay testable with plain `cargo test`. What a service needs from the platform is a trait in
`rayx_runtime` that `rayx_app` implements on NSH: reading the app's assets, registering fonts, the device's state, the
bundled `appsettings.json`.

`rayx_app` and `rayx_components` are the crates that depend on NSH. `rayx_app` ties the core's lifetimes to what NSH runs (see
[Dependency injection](dependency-injection.md#lifetimes-on-nsh)) and gives TSX the same builder and container (see
[TSX](tsx.md)): a TSX app builds and starts itself in TS, its services written in TS live in the one container, and
Rust reaches them through contracts.

## The umbrella

`rayx` re-exports:

- `rayx::core`: `rayx_core`
- `rayx::runtime`: `rayx_runtime`
- `rayx::app`: `rayx_app`
- `rayx::components`: `rayx_components` (not in the prelude: its names, `Button`, `Density`, would clash)
- `rayx::devtools_protocol`: `rayx_devtools_protocol`
- `rayx::i18n` and `rayx::intl`: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
  [dXn.Intl](https://github.com/dXnSix/dXn.Intl), for their message, number and date types

`use rayx::prelude::*;` brings the builder and its extensions (`start()` among them), the hooks, contract
registration (`registry.contract::<dyn Trait>()`), `Registry` and the dev tools. `#[rayx::native_module("my-app")]` on
a TSX app's registry function is its UI set: NSH's `#[native_module]` with every RayX TS class and RayX's components,
then the function's body; `#[rayx::app_extension]` and `#[rayx::contract]` are the umbrella's other attributes.

## Dependencies

- View-model state comes from [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) (`dxn_reactive`); translations
  use MessageFormat 2 messages from dXn.I18n and dXn.Intl.
- NSH comes from an integration tag `nshx-*` of the fork [dXnSix/nsh](https://github.com/dXnSix/nsh), through
  `[patch.crates-io]` in `Cargo.toml`. A consumer's own patch decides which NSH it builds with.
- Every dependency is a git tag: no path, branch or commit dependencies.

## Releases

release-plz releases every crate under one version and one tag `vX.Y.Z`, and cargo-dist builds the `rayx` binaries for
that tag. See [Releasing RayX](releasing.md).
