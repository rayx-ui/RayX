# RayX

RayX is an app framework for [Native Shell (NSH)](https://github.com/dXnSix/nsh) apps, written in Rust and used from
Rust and TSX.

- **Dependency injection, configuration and options** with the API of .NET's `Microsoft.Extensions`: singleton, scoped
  and transient services, services that initialize asynchronously at startup, and typed options bound from
  configuration.
- **App services:** storage, operations with undo and redo, navigation, input maps, platform and lifecycle state,
  assets, fonts, and translations with MessageFormat 2 messages.
- **NSH integration:** a scope per screen, view models bound to NSH signals, navigation regions as native navigation
  stacks, and a TSX class for every service.
- **Components:** the RayX design system in code: its roles, a default theme that maps them onto its Figma tokens
  (any NSH theme document that maps them draws the components), and its components written in Rust on NSH's base
  components, for Rust and TSX ([Components and theme](docs/components.md),
  the [gallery](examples/gallery/README.md)).
- **The `rayx` CLI:** sets a machine up for NSH work, runs apps through the NSH their project pins, and creates new RayX
  apps.

## Quick start

Install the CLI (macOS and Linux; Windows is in the [CLI's README](crates/rayx_cli/README.md#install)) and create an
app:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rayx-ui/RayX/releases/latest/download/rayx-cli-installer.sh | sh
rayx new my-app
```

Or add RayX to an existing crate. Releases are git tags `vX.Y.Z`; nothing is published to a registry:

```toml
[dependencies]
rayx = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z", features = ["storage", "operations", "history"] }
```

Each service is a crate behind a `rayx` feature of its name (`rayx::services::<name>`); the defaults are the four the
host requires, `platform`, `suspension`, `assets` and `fonts`, and the app lists the others it uses. An app builds its
services with `RayXAppBuilder` and starts them, the same way in Rust and in TSX:

```rust
use rayx::prelude::*;

// In the root component: the four the host requires, then the app's own choice.
RayXAppBuilder::new()
    .add_platform()
    .add_suspension()
    .add_assets(|_| {})
    .add_fonts(|_| {})
    .add_app_settings("my-app")
    .add_storage()
    .add_operations()
    .add_history()
    .start();
```

```tsx
// The crate's src/lib.rs: `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) {}`
import { KeyValueStore, RayXAppBuilder } from "my-app";

new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addFonts()
  .addAppSettings("my-app")
  .addStorage()
  .addOperations()
  .addHistory()
  .start() // resolves once every startup service is ready
  .then((services) => {
    const store = services.getRequiredService(KeyValueStore);
  });
```

[Getting started](docs/getting-started.md) walks through it.

## Crates

| Crate | What it is |
| --- | --- |
| `rayx` | The umbrella an app depends on, with `use rayx::prelude::*;`, `#[rayx::native_module]` and one feature per service crate (defaults `platform`, `suspension`, `assets`, `fonts`) |
| `rayx_app` | The core: dependency injection, configuration, options, the app builder, every service's contract (`rayx_app::services::<name>`) and a fake of each (`rayx_app::testing`); no NSH, builds for wasm32 |
| `rayx_app_nsh` | The NSH host: the app's start (requiring the platform, suspension, asset and font services), surfaces, screens, view-model binding, and the TS side: TS's `RayXAppBuilder` and `ServiceProvider`, TS services, contracts |
| `rayx_services_<name>` | RayX's implementation of one service each (platform, suspension, assets, fonts, storage, navigation, i18n, operations, history, selection, input map, application configuration), on NSH, with its TS class |
| `rayx_components` | The component library and its theme, for Rust and TSX |
| `rayx_devtools_protocol` | The dev tools' agent protocol |
| `rayx_app_macros` | `#[derive(AppService)]`, `#[app_extension]`, `#[contract]`, `contract_nsh!`, `#[native_module]` |
| `rayx-cli` | The `rayx` command-line tool |

## Documentation

The [documentation index](docs/INDEX.md) lists every guide:

- [Getting started](docs/getting-started.md)
- [Architecture](docs/architecture.md)
- [Dependency injection](docs/dependency-injection.md)
- [Configuration and options](docs/configuration.md)
- [Startup](docs/startup.md)
- [View models](docs/view-models.md)
- [Navigation](docs/navigation.md)
- [TSX](docs/tsx.md)
- [App services](docs/services.md)
- [Components and theme](docs/components.md)
- [Dev tools](docs/devtools.md)
- [The `rayx` CLI](crates/rayx_cli/README.md)

## Status

0.x, on NSH 0.5 (the fork's `nshx-*` tags). A minor release (`0.5` → `0.6`) may break the API; [CHANGELOG.md](CHANGELOG.md) lists every release,
with `Migration:` notes for each break.

## License

RayX is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT),
at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in RayX,
as defined in the Apache-2.0 license, is dual licensed as above, without any additional terms or conditions.
