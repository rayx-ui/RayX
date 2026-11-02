# Startup

Services can initialize asynchronously before the app is ready: loading a cache, opening a connection, reading the
app's settings.

## Startup services

Register them with `add_startup_singleton` (or `add_startup_scoped`, `add_startup_transient`, and the `_as` forms) and
implement two traits:

- `AsyncInitializable`: `initialize(context)`, a future, `is_initialized()`, and optionally
  `initialization_timeout()`.
- `StartupInitializable`: `startup_service_type()`, and optionally `initialization_depends_on()`, the services that must
  initialize first.

```rust
#[derive(Clone, Default, AppService)]
struct Catalog {
    initialized: StartupInitializationState,
}

impl AsyncInitializable for Catalog {
    fn is_initialized(&self) -> bool {
        self.initialized.is_initialized()
    }

    fn initialize(
        &self,
        _context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let initialized = self.initialized.clone();
        async move {
            // load what the service needs
            initialized.mark_initialized();
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for Catalog {
    fn startup_service_type(&self) -> StartupServiceType {
        StartupServiceType::of::<Self>()
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<ApiClient>()]
    }
}
```

## How the app starts

Starting the app builds the root provider (its configuration sources may load asynchronously), makes the options
registered with `validate_on_start`, and initializes every startup service in dependency order, the services of one
layer concurrently. The app is ready, and services resolve, only once all of them initialized.

- **Failure:** one failing service fails startup, with the services that failed and why; nothing is installed.
- **Timeout:** each service may set `initialization_timeout()`; one that sets none gets the builder's default
  (`default_initialization_timeout(..)`, 30 s unless set). A service past its timeout fails startup.

## Starting from TSX

```tsx
const app = await AppServices.load(); // once: starts the app; rejects if a service failed or timed out
const scope = app.scope();            // on a screen's mount
scope.dispose();                      // in its cleanup
```

Register `AppServices` and `ServiceScope` in the app's UI set. `load()` is asynchronous because startup awaits the
services; everything after it is synchronous. A TSX app has no root component, so `load()` also keeps the platform and
suspension services current from NSH.

## Starting from a Rust root

```rust
#[component]
fn Root() -> impl IntoElement {
    rayx_app::start();
    dynamic(|| match rayx_app::app_state() {
        AppState::Ready => home().into_element(),
        AppState::Failed(message) => error_screen(message).into_element(),
        _ => splash().into_element(),
    })
}
```

`app_state()` is `NotStarted`, `Starting`, `Ready` or `Failed(message)`; reading it subscribes, so the root shows a
splash until the app is ready. Call `use_platform_sync()` in the root to keep the platform and suspension services
current from NSH.
