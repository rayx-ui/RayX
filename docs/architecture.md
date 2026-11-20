# Architecture

RayX is a set of crates released together under one version. An app depends on the umbrella `rayx`, with a feature per
service it registers; an app's own crate of NSH-free services depends on `rayx_app` alone.

| Crate | What it holds | NSH |
| --- | --- | --- |
| `rayx_app` | The core: dependency injection (singleton, scoped and transient services, scopes, async startup, required services and declared dependencies), configuration sources, .NET's options model, the `RayXAppBuilder`, every service's contract (`rayx_app::services::<name>`) and a fake of each (`rayx_app::testing`) | no |
| `rayx_app_nsh` | The NSH host: the app's start on each surface (requiring the platform, suspension, asset and font services), a root provider per surface, a scope per screen, view-model activation and binding, `add_app_settings`, the dev tools' agent, and the TS side (`rayx_app_nsh::ts`: TS's `RayXAppBuilder`, its `ServiceProvider`, services written in TS, contracts with `ContractState` and `ContractEvents`) | yes |
| `rayx_services_<name>` | RayX's implementation of one core service contract each, on NSH, with its builder extension, its TS class and its set: `rayx_services_platform`, `rayx_services_suspension`, `rayx_services_assets`, `rayx_services_fonts`, `rayx_services_storage`, `rayx_services_navigation` (with its regions as NSH navigation stacks), `rayx_services_i18n` (with `tr!`), `rayx_services_operations`, `rayx_services_history`, `rayx_services_selection`, `rayx_services_input_map`, `rayx_services_application_configuration`, `rayx_services_theme` (the theme's modifier contexts, light/dark with the color scheme) | yes |
| `rayx_components` | The component library: the design system's roles (`roles.json`, typed handles in `theme::roles`), RayX's default theme that maps them (`theme.json`, an NSH theme document exported from the design system's DTCG by RayX.Dtcg, its modes the document's modifiers), and the components (`Button`) on nsh-base, reading roles only, each with its preview file (`button.preview.tsx`), for Rust and TSX, where it is the UI set `rayx/components` (`set()`) every app's set serves | yes |
| `rayx_devtools_protocol` | The agent protocol of the dev tools: the JSON-RPC messages between an app's dev-build agent and a tool's hub ([Dev tools](devtools.md)); serde and serde_json alone | no |
| `rayx_app_macros` | `#[derive(AppService)]`, re-exported by `rayx_app`, `rayx_app_nsh` and `rayx`; `#[app_extension]` and `#[contract]`, re-exported by `rayx_app_nsh` and `rayx`; `contract_nsh!`, re-exported by `rayx_app_nsh`; `#[native_module]`, used as `#[rayx::native_module]` | no |
| `rayx` | The umbrella: re-exports the other crates (each service crate behind a feature of its name, the four the host requires by default) and dXn.I18n and dXn.Intl, with a prelude, and `#[rayx::native_module]`, the attribute of a TSX app's UI set | yes |
| `rayx-cli` | The `rayx` command-line tool | no (runs the `nsh` CLI) |

## Two layers

The core has no NSH dependency: it builds for every target, `wasm32-unknown-unknown` included, and holds each service's
contract (a `#[contract]` trait in `rayx_app::services`) with a fake of each in `rayx_app::testing`. RayX's
implementations are the service crates, one per service, on NSH; an app registers its own implementation of any
contract, in Rust or TS, in place of RayX's ([App services](services.md#replacing-a-service)).

`rayx_app_nsh` ties the core's lifetimes to what NSH runs (see
[Dependency injection](dependency-injection.md#lifetimes-on-nsh)) and gives TSX the same builder and container (see
[TSX](tsx.md)): a TSX app builds and starts itself in TS, its services written in TS live in the one container, and
Rust reaches them through contracts. It registers no service for the app; it requires the four it starts with.

## Dependency rules

| Crate | May depend on | Never on |
| --- | --- | --- |
| `rayx_app` | `rayx_app_macros`, dXn.I18n and dXn.Intl, plain Rust crates | NSH, any other RayX crate; it keeps building for `wasm32-unknown-unknown` |
| `rayx_app_nsh` | `rayx_app`, `rayx_app_macros`, `rayx_devtools_protocol`, NSH, dXn.Reactive | a service crate |
| `rayx_services_<name>` | `rayx_app`, `rayx_app_nsh`, `nsh-core` (and what its service needs: dXn.I18n and dXn.Intl for i18n) | another service crate: a service it needs is resolved by trait and declared (`require_for`) |
| `rayx_components` | `nsh-core`, `nsh-base` | the other RayX crates |
| `rayx_devtools_protocol` | serde, serde_json | RayX, NSH, sockets, threads, clocks; it builds for `wasm32-unknown-unknown` |
| `rayx` | every crate above, each service crate optional | |
| `rayx-cli` | its own tools | any RayX, NSH, gpux or GPUI crate (it runs the `nsh` CLI) |

`crates/rayx/tests/crate_layout.rs` checks the packages, their folders and these rules over `cargo metadata`, and the
CLI's `tests/library_guard.rs` its dependency tree.

## The umbrella

`rayx` re-exports:

- `rayx::app`: `rayx_app`, the core, with every service's contract at `rayx::app::services::<name>`
- `rayx::app_nsh`: `rayx_app_nsh`
- `rayx::services::<name>`: `rayx_services_<name>`, for each service crate its feature enables
- `rayx::components`: `rayx_components` (not in the prelude: a view imports `rayx::components::prelude::*`, the
  components, `ComponentSize`, the role groups, `roles` and `value`)
- `rayx::devtools_protocol`: `rayx_devtools_protocol`
- `rayx::i18n` and `rayx::intl`: [dXn.I18n](https://github.com/dXnSix/dXn.I18n) and
  [dXn.Intl](https://github.com/dXnSix/dXn.Intl), for their message, number and date types
- `rayx::checked` (`checked::tr!`, checked against the app's messages when it builds), with `i18n`

### Features

Each service crate is an optional dependency of the umbrella behind one feature named after the service:
`application_configuration`, `assets`, `fonts`, `history`, `i18n`, `input_map`, `navigation`, `operations`,
`platform`, `selection`, `storage`, `suspension` and `theme`. A feature enables its crate and nothing else; no feature implies
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
adds RayX's TS classes and the set of each enabled service crate (its classes, its contract's interface and its
builder method), and serves RayX's component library as its own set, `rayx/components` (`registry.serve(rayx::components::set())`:
TSX imports `Button` and `Previews` from there), then runs the function's body. With the default features a TS `RayXAppBuilder` has
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
