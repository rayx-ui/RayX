# Startup

Services can initialize asynchronously before the app is ready: loading a cache, opening a connection, reading the
app's settings.

## Startup services

Register them with `add_startup_singleton` (or `add_startup_scoped`, `add_startup_transient`, and the `_as` forms) and
implement two traits:

- `AsyncInitializable`: `initialize(context)`, a future, `is_initialized()`, and optionally
  `initialization_timeout()`.
- `StartupInitializable`: optionally `initialization_depends_on()`, the services that must initialize first, and
  `startup_service_type()`, the key others name to wait for it. Leave `startup_service_type()` out to key the service
  by what it was registered as: `add_startup_singleton::<T>` keys it `T`, `add_startup_singleton_as::<dyn X>` keys it
  `dyn X` (the scoped and transient forms alike).

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

// Registered with `add_startup_singleton(Catalog::default())`, so keyed `Catalog`.
impl StartupInitializable for Catalog {
    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<ApiClient>()]
    }
}
```

A startup service written in TS implements the interfaces of the same names ([below](#startup-services-in-ts)); this
one waits for the translations:

```tsx
class Catalog implements StartupInitializable {
  private state = new StartupInitializationState();
  isInitialized() { return this.state.isInitialized(); }
  async initialize(context: StartupInitializationContext) {
    // load what the service needs
    this.state.markInitialized();
  }
  initializationDependsOn() { return [I18nService]; }
}

builder.addStartupSingleton(Catalog, () => new Catalog());
```

## Implementing a trait

A contract (or any `dyn Trait`) is not `StartupInitializable`, so `add_startup_singleton_as::<dyn Trait>` cannot
register an implementation of it that loads at the start. `add_startup_singleton_for::<dyn Trait, Impl>(Arc<Impl>)`
does: it registers `Impl` as the app's `dyn Trait` and as a startup service registered as `dyn Trait`, keyed by its own
`startup_service_type()`, else `dyn Trait`, so the services that wait for the trait wait for it:

```rust
struct KeychainStore { /* … */ }
impl KeyValueStore for KeychainStore { /* … */ }
impl AsyncInitializable for KeychainStore { /* opens the keychain */ }
impl StartupInitializable for KeychainStore {}

builder.add_startup_singleton_for::<dyn KeyValueStore, _>(Arc::new(KeychainStore::new()))
```

- `add_startup_singleton_factory_for::<dyn Trait, Impl>(|services| Arc::new(..))` makes it on first resolve (or when
  startup runs); `add_startup_scoped_for` and `add_startup_transient_for` are its scoped and transient forms.
- `add_startup_service_for::<dyn Trait, _>(Arc::new(..))` adds a startup service registered as the trait without
  registering anything to resolve (`add_suspension()` follows NSH's app state this way beside its service).
- Every contract can be registered this way (`#[contract]` implements `ImplementedBy` for it); a plain trait of the
  app's needs `rayx::app::implemented_by!(MyTrait);` beside it.
- An async factory form is not offered: the trait resolves synchronously, before startup has run, so its value must
  exist by then; `add_startup_resolved_async` makes a startup service asynchronously, without a value to resolve.

A startup service that is not a Rust type is keyed by a runtime key (`StartupServiceType::runtime(&key)`) and may be
made asynchronously when startup runs: `add_startup_resolved_async(&key, |context| async { … }.boxed_local())`. It is
ordered, initialized, timed and reported (by the key's display name) with every other startup service, and it may wait
for typed services and they for it. A factory that fails, or does not finish within the default timeout, fails startup,
naming the service.

## How the app starts

Starting the app checks the registrations, builds the root provider (its configuration sources may load
asynchronously), makes the options registered with `validate_on_start`, makes the startup services that factories make,
and initializes every startup service in dependency order, the services of one layer concurrently. The app is ready,
and services resolve, only once all of them initialized.

- **Required services:** on NSH the start requires the platform, suspension, asset and font services
  (`dyn PlatformInfoService`, `dyn SuspensionService`, `dyn AssetService`, `dyn FontService`): RayX's
  (`add_platform()`, `add_suspension()`, `add_assets(..)`, `add_fonts(..)`, from the umbrella's default features) or
  the app's own, in Rust or TS. Nothing is registered for the app: `RayXAppBuilder::new()` and `new RayXAppBuilder()`
  are empty.
- **Declared dependencies:** a service crate's builder method declares what its service resolves instead of
  registering it (`add_i18n` the asset and platform services, `add_fonts` the asset service, `add_history` an operation
  service, `add_persistent_history` and `add_input_map` a key-value store, `add_selection` a navigation service,
  `add_application_configuration` the platform service). Each is satisfied by RayX's implementation or the app's own
  `dyn Trait`, registered before or after it.
- **The checks:** before any source loads or any factory runs, the build checks every requirement and fails with one
  `ServiceValidationError` listing each service missing, who needs it and how to register it (`add_fonts(..) from
  rayx_services_fonts, or register your own dyn FontService`; in TS, the TS method and contract), and each singleton
  that depends on a scoped service ([Dependency injection](dependency-injection.md#required-services-and-declared-dependencies)).
  A startup service waiting for a key nothing registered fails startup with `MissingDependency`, a cycle with
  `CircularDependency`.
- **Failure:** one failing service fails startup, with the services that failed and why; nothing is installed.
- **Timeout:** each service may set `initialization_timeout()`; one that sets none gets the builder's default
  (`default_initialization_timeout(..)`, 30 s unless set; TS's `defaultInitializationTimeout(ms)`). A service past its
  timeout fails startup.

## Startup services in TS

TS startup services are in the same startup graph as Rust's: they wait for Rust services and Rust services for them,
with the same timeout and failure report. They are registered with `addStartupSingleton`, `addStartupScoped`,
`addStartupTransient` and their `As` forms, and implement:

- `AsyncInitializable`: `isInitialized()`, `initialize(context)` (it may return a promise; `context.services` is the
  root provider) and, optionally, `initializationTimeout()` in milliseconds.
- `StartupInitializable`: the same members and, optionally, `initializationDependsOn()`, the services (TS classes,
  `ServiceToken`s, RayX's classes) that must initialize first, and `startupServiceType()`, the key others name to wait
  for it. Without it, the service is keyed by the token it was registered under.

`StartupInitializationState` (`isInitialized()`, `markInitialized()`) holds the flag, as in Rust.

- `startupServiceType`, `initializationDependsOn` and `initializationTimeout` are read once, when the service is made;
  `isInitialized` is asked just before it would initialize, so a service another one initialized meanwhile is skipped.
- A service that throws or rejects fails startup, named by its class (or token).
- A timeout fails startup but cannot stop the JS work: what `initialize` started still runs.

## Ready hooks

A ready hook runs once every startup service initialized, before the app counts as ready: the place for what needs the
services in place, such as restoring the language the user picked or opening the screen of a pending deep link.

```rust
builder
    .on_ready(|context| async move {
        let store = context.services().required_service::<dyn KeyValueStore>()?;
        let saved = store.bucket("settings").try_get_json::<String>("language").await.ok().flatten();
        if let Some(language) = saved {
            let i18n = context.services().required_service::<dyn I18nService>()?;
            i18n.set_language(language).await.map_err(|error| ReadyError::new(error.to_string()))?;
        }
        Ok(())
    })
    .on_ready_when_platform(|platform| platform.form_factor.is_handheld(), |_| async {
        // only on phones and tablets
        Ok(())
    })
```

- Hooks run one after another, in the order they were added; `on_ready_when(predicate, hook)` runs its hook only when
  the predicate holds for the root provider, and `on_ready_when_platform` (`PlatformReadyAppBuilderExt`, in
  `rayx::app::services::platform`) asks the platform service's snapshot, and does not run without one.
- A hook gets a `ReadyContext` (`services()`, `options::<T>()`) and returns a `ReadyResult` (`?` takes service resolution and options errors); the first that fails
  stops the rest and fails startup (`AppError::Ready`, `AppState::Failed`).
- On NSH they run on the engine's thread, so they may call NSH's functions; `app_state()` turns `Ready` after them.

The same in TS: `onReady(hook)` and `onReadyWhen(predicate, hook)`, whose hook gets a `ReadyContext`
(`context.services`) and may return a promise. TS and Rust hooks (from an [app extension](tsx.md#app-extensions)) run
in the order they were added; one that throws or rejects fails `start()` with its message.

```tsx
builder.onReady(async (context) => {
  const store = context.services.getRequiredService(KeyValueStore);
  const language = await store.getJson("settings", "language");
  if (typeof language === "string") await context.services.getRequiredService(I18nService).setLanguage(language);
});
```

## Starting from TSX

```tsx
const services = await new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addFonts()
  .addStorage()
  .addI18n()
  .start();                           // once: rejects if a service is missing, failed or timed out
const scope = services.createScope(); // on a screen's mount
scope.dispose();                      // in its cleanup
```

The app's crate is `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) {}`, whose set holds the
builder, RayX's classes and the set of each service crate the umbrella's features enable (here `storage` and `i18n`
besides the defaults; [TSX](tsx.md#the-apps-crate)). `start()` is asynchronous because startup awaits the services;
everything after it is synchronous. RayX's platform and suspension services keep themselves current from NSH from
their startup on, in a TSX app as in a Rust one.

## Starting from a Rust root

```rust
#[component]
fn Root() -> impl IntoElement {
    RayXAppBuilder::new()
        .add_platform()
        .add_suspension()
        .add_assets(|_| {})
        .add_fonts(|_| {})
        .add_storage()
        .add_i18n(|_| {})
        .start();
    dynamic(|| match rayx_app_nsh::app_state() {
        AppState::Ready => home().into_element(),
        AppState::Failed(message) => error_screen(message).into_element(),
        _ => splash().into_element(),
    })
}
```

`start()` starts the app on the surface in a task of its own; a second start on a surface whose app started (or is
starting) keeps that app. `app_state()` is `NotStarted`, `Starting`, `Ready` or `Failed(message)`; reading it
subscribes, so the root shows a splash until the app is ready. `start_async()` instead gives the root provider, or the
`AppError`, to await in an NSH task. The start requires the platform, suspension, asset and font services and fails,
before anything is made, naming each one missing (`AppError::Services`; `AppState::Failed` with its message).
