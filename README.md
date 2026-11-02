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
rayx = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }
```

An app names the function that builds its services, and its UI starts them:

```rust
use rayx::prelude::*;

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder.add_app_settings("my-app").add_storage().add_history()
}
```

```tsx
const app = await AppServices.load(); // resolves once every startup service is ready
```

[Getting started](docs/getting-started.md) walks through it.

## Crates

| Crate | What it is |
| --- | --- |
| `rayx` | The umbrella an app depends on, with `use rayx::prelude::*;` |
| `rayx_core` | Dependency injection, configuration, options and the app builder; no NSH, builds for wasm32 |
| `rayx_runtime` | The app services; no NSH, builds for wasm32 |
| `rayx_app` | The NSH layer: surfaces, screens, view-model binding, navigation regions, TSX classes |
| `rayx_app_macros` | `#[derive(AppService)]` |
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
- [The `rayx` CLI](crates/rayx_cli/README.md)

## Status

0.x, on NSH 0.3. A minor release (`0.5` → `0.6`) may break the API; [CHANGELOG.md](CHANGELOG.md) lists every release,
with `Migration:` notes for each break.

## License

RayX is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT),
at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in RayX,
as defined in the Apache-2.0 license, is dual licensed as above, without any additional terms or conditions.
