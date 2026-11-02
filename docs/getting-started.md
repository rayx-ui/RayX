# Getting started

## Install the CLI

`rayx` is RayX's command-line tool. It sets a machine up for NSH work, runs every NSH app through the `nsh` its project
pins, and creates new RayX apps. On macOS and Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rayx-ui/RayX/releases/latest/download/rayx-cli-installer.sh | sh
```

The Windows one-liner, and every command, is in the [CLI's README](../crates/rayx_cli/README.md#install).

## Create an app

```bash
rayx new my-app
```

`rayx new` creates an app on the CLI's own release: its `rayx` dependency and NSH pin already match.

## Add RayX to an existing crate

Releases are git tags `vX.Y.Z` (the latest is on the repository's Releases page); nothing is published to a registry.
Depend on a tag, the same one for every RayX crate:

```toml
[dependencies]
rayx = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }         # apps: everything, with a prelude
rayx_core = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }    # service crates
rayx_runtime = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" } # RayX's services
rayx_app = { git = "https://github.com/rayx-ui/RayX", tag = "vX.Y.Z" }     # NSH bridge crates
```

NSH comes from an integration tag `fork-*` of the fork [dXnSix/nsh](https://github.com/dXnSix/nsh), through
`[patch.crates-io]`. An app takes the same tag as the RayX release it depends on: copy the `[patch.crates-io]` table
from RayX's `Cargo.toml` at that tag.

## A first app

An app has one composition root: a function that adds its configuration and services to a `RayXAppBuilder`.
`set_app` names it, and every NSH surface builds its own root provider from it when the app starts there. This is the
`src/lib.rs` `rayx new` writes for a TSX app, with the TSX classes the UI uses:

```rust
use nsh_core::prelude::*;
use rayx::prelude::*;

#[native_module("my-app")]
fn native(registry: &mut Registry) {
    set_app(build_app);
    registry
        .class::<AppServices>()
        .class::<ServiceScope>()
        .class::<I18n>();
}

fn build_app(builder: RayXAppBuilder) -> RayXAppBuilder {
    builder
        .add_app_settings("my-app") // assets/appsettings.json
        .add_storage()
        .add_history()
        .add_i18n(|options| {
            options.default_language("en");
        })
}
```

The UI starts the app before it uses a service:

```tsx
// App.tsx
const app = await AppServices.load(); // starts the app; rejects if a startup service failed
const scope = app.scope();            // one scope per screen
```

A Rust UI starts it in its root component instead:

```rust
#[component]
pub fn App() -> impl IntoElement {
    set_app(build_app);
    rayx::app::start();
    dynamic(|| match rayx::app::app_state() {
        AppState::Ready => home().into_element(),
        AppState::Failed(message) => error_screen(message).into_element(),
        _ => splash().into_element(),
    })
}
```

Next: [Dependency injection](dependency-injection.md), [Configuration and options](configuration.md) and
[Startup](startup.md).

## Upgrading

RayX is 0.x: a minor release (`0.5` → `0.6`) may break the API. Each breaking change has a `Migration:` note in
[CHANGELOG.md](../CHANGELOG.md).

- From `v0.4`: `rayx_app_di` is `rayx_core` now; rename the dependency and its paths.
- From `nsh-app` `v0.3.0`: rename the dependencies (`nsh_app` to `rayx_app`, `nsh_app_di` to `rayx_core`) and their
  paths, and move to `dxn_reactive` (formerly `rayx_reactive`), whose activators `rayx_app` re-exports.
