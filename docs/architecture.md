# Architecture

RayX is a set of crates released together under one version. An app depends on the umbrella `rayx`; a service crate
depends on `rayx_app` alone.

| Crate | What it holds | NSH |
| --- | --- | --- |
| `rayx_app` | Dependency injection (singleton, scoped and transient services, scopes, async startup), configuration sources, .NET's options model and the `RayXAppBuilder` | no |
| `rayx_services_<name>` | RayX's implementation of one core service contract each, on NSH: `rayx_services_platform`, `rayx_services_suspension`, `rayx_services_assets`, `rayx_services_fonts`, `rayx_services_navigation` (with its regions as NSH navigation stacks), `rayx_services_operations`, `rayx_services_history`, `rayx_services_selection`, `rayx_services_input_map`, `rayx_services_application_configuration`, `rayx_services_i18n`, `rayx_services_storage` | yes |
| `rayx_app_nsh` | The NSH layer: the app's start on each surface, a root provider per surface, a scope per screen, view-model activation and binding, the TS side (`rayx_app_nsh::ts`: TS's `RayXAppBuilder`, its `ServiceProvider`, services written in TS, contracts and the services' classes), and the NSH side of the services | yes |
| `rayx_components` | The component library: the design system's tokens as an NSH theme document (`theme.json`, generated from the Figma export) read through typed keys, its modes as the document's modifiers, and the components (`Button`) on nsh-base, for Rust and TSX | yes |
| `rayx_devtools_protocol` | The agent protocol of the dev tools: the JSON-RPC messages between an app's dev-build agent and a tool's hub ([Dev tools](devtools.md)); serde and serde_json alone | no |
| `rayx_app_macros` | `#[derive(AppService)]`, re-exported by `rayx_app`, `rayx_app_nsh` and `rayx`; `#[app_extension]` and `#[contract]`, re-exported by `rayx_app_nsh` and `rayx`; `#[native_module]`, used as `#[rayx::native_module]` | no |
| `rayx` | The umbrella: re-exports the other crates (each service crate behind a feature of its name) and dXn.I18n and dXn.Intl, with a prelude, and `#[rayx::native_module]`, the attribute of a TSX app's UI set | yes |
| `rayx-cli` | The `rayx` command-line tool | no (runs the `nsh` CLI) |

## Two layers

The core has no NSH dependency: it builds for every target, `wasm32-unknown-unknown` included, and holds each service's
contract (a `#[contract]` trait in `rayx_app::services`) with a fake of each in `rayx_app::testing`. RayX's
implementations are the service crates, one per service, on NSH; an app registers its own implementation of any
contract, in Rust or TS, in place of RayX's.

`rayx_app_nsh` and `rayx_components` are the crates that depend on NSH. `rayx_app_nsh` ties the core's lifetimes to what NSH runs (see
[Dependency injection](dependency-injection.md#lifetimes-on-nsh)) and gives TSX the same builder and container (see
[TSX](tsx.md)): a TSX app builds and starts itself in TS, its services written in TS live in the one container, and
Rust reaches them through contracts.

## The umbrella

`rayx` re-exports:

- `rayx::app`: `rayx_app`, the core, with every service's contract at `rayx::app::services::<name>`
- `rayx::app_nsh`: `rayx_app_nsh`
- `rayx::services::<name>`: `rayx_services_<name>`, for each service crate its feature enables
- `rayx::components`: `rayx_components` (not in the prelude: its names, `Button`, `Density`, would clash)
- `rayx::devtools_protocol`: `rayx_devtools_protocol`
- `rayx::i18n` and `rayx::intl`: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
  [dXn.Intl](https://github.com/dXnSix/dXn.Intl), for their message, number and date types
- `rayx::checked` (`checked::tr!`, checked against the app's messages when it builds), with `i18n`

### Features

Each service crate is an optional dependency of the umbrella behind one feature named after the service:
`application_configuration`, `assets`, `fonts`, `history`, `i18n`, `input_map`, `navigation`, `operations`,
`platform`, `selection`, `storage` and `suspension`. A feature enables its crate and nothing else; no feature implies
another, so a service that needs one the app did not register (history over operations, i18n over the platform and
asset services) fails the app's start with the missing service's hint, as any missing dependency does.

The defaults are the four services the host requires: `platform`, `suspension`, `assets` and `fonts`. Everything
else is opt-in, and a feature off keeps its crate out of the build:

```toml
[dependencies]
rayx = { version = "…", features = ["i18n", "storage", "operations", "history"] }
```

`use rayx::prelude::*;` brings the builder and its extensions (`start()` among them), each enabled service crate's
builder extension (`add_platform`, `add_fonts(..)`, `add_i18n(..)` with `tr!`, `add_navigation(..)` with
`navigation_region` and `navigation_view`, …), the hooks, contract registration (`registry.contract::<dyn Trait>()`),
`Registry` and the dev tools.

`#[rayx::native_module("my-app")]` on a TSX app's registry function is its UI set: NSH's `#[native_module]` whose body
first calls the umbrella's registration (`rayx::__private::register`, compiled with the umbrella's features), which
adds RayX's TS classes, the set of each enabled service crate (its classes, its contract's interface and its builder
method) and RayX's components, then runs the function's body. With the default features a TS `RayXAppBuilder` has
`addPlatform`, `addSuspension`, `addAssets` and `addFonts` and no `addI18n`; the `i18n` feature adds it.
`#[rayx::app_extension]` and `#[rayx::contract]` are the umbrella's other attributes.

## Dependencies

- View-model state comes from [dXn.Reactive](https://github.com/dXnSix/dXn.Reactive) (`dxn_reactive`); translations
  use MessageFormat 2 messages from dXn.I18n and dXn.Intl.
- NSH comes from an integration tag `nshx-*` of the fork [dXnSix/nsh](https://github.com/dXnSix/nsh), through
  `[patch.crates-io]` in `Cargo.toml`. A consumer's own patch decides which NSH it builds with.
- Every dependency is a git tag: no path, branch or commit dependencies.

## Releases

release-plz releases every crate under one version and one tag `vX.Y.Z`, and cargo-dist builds the `rayx` binaries for
that tag. See [Releasing RayX](releasing.md).
