# RayX's services

`rayx_runtime` holds RayX's app services, ported from RayX.Old's `rayx_runtime::services` (`../RayX.Old`) onto
`rayx_core`. Like the core, it has no NSH dependency and builds for `wasm32-unknown-unknown`; what a service needs from
the platform is a trait in `rayx_runtime` that `rayx_app` implements on NSH.

Every service is registered on the app builder through an `AppBuilder` extension trait (`add_<service>(..)`, which keeps
a service registered earlier, and `with_<service>_service(..)`, which replaces it) and reads its options with
`rayx_core`'s options model from the configuration section `RayX:Services:<Name>` (the app's own section,
`RayX:Application`, for the application configuration). Options bind with the core's key matching, so
`appsettings.json` may spell keys in camelCase.

Porting changes `use` paths (`rayx_runtime::services::<service>` stays), `RayXService` (now `AppService`) and the
registration (the `with_*` builder methods and `configure_options` plans become `add_*` and `add_options`). What every
service left behind:

- `AppContext` and the `*AppContextExt` / `*GpuxAppExt` traits: GPUI globals. Resolve services from the provider
  (`services.required_service::<T>()`, or `rayx_app::services()` in a component).
- `initialize_gpux` startup phases and anything else that touches a GPUI `App`, `Window` or `Entity`.

The services, and what each ports and leaves out:

## Operations (`services::operations`)

Typed, versioned domain operations applied to registered targets: `OperationService` registers an
`OperationTargetHandler<T>` per `OperationTargetId`, applies `DomainOperation`s atomically in transactions (join, new or
reject nesting), returns receipts with their inverse operations and publishes `OperationEvent`s. Registered with
`add_operations()` (a startup singleton) or `with_operation_service(..)`.

- Ported whole, with its contract tests (`tests/operations.rs`); the `rayx_diagnostics` feature is `diagnostics`.
- Left out: `OperationsAppContextExt` and `OperationsGpuxAppExt` (GPUI globals).

## History (`services::history`)

Undo and redo per history scope over the operation service: `HistoryService` records the receipts of the targets bound
to a scope (`bind_target_scope`), undoes and redoes them through their inverse operations, enforces size limits, keeps
scope lifecycles and publishes `HistoryEvent`s; `HistoryStore` persists each scope's journal (`HistoryJournalV2`), with
`InMemoryHistoryStore` built in. Registered with `add_history()` (history in memory) or `add_history_with_store(store)`,
both registering the operation service too.

- Ported whole, with its tests (`tests/history.rs`: contracts, scope lifecycle, persistence, receipt admission, replay,
  several windows, journal versions).
- Changed: every target persists through the store's synchronous methods. RayX.Old's WebAssembly path loaded and
  saved in the background with `wasm_bindgen_futures` (and refused `clear_scope` while a store was set); the core has
  no browser dependency, and the in-memory stores of this iteration need none.
- Left out: `HistoryAppContextExt` and `HistoryGpuxAppExt` (GPUI globals). A persistent store over NSH's storage is a
  later step; storage is in memory in this iteration (see Storage).

## Storage (`services::storage`)

Values as bytes under keys in named buckets, behind the `KeyValueStore` trait (sync methods, async ones that default to
them), with JSON helpers on `store.bucket(name)`. Registered with `add_storage()` (an `InMemoryKeyValueStore`, unless a
store is registered) or `with_key_value_store(store)`.

- Kept: the provider trait (now `KeyValueStore`, with `keys` and `clear`), buckets of JSON values, async variants.
- Left out, as agreed for this iteration: every persistent backend (the SurrealDB, RocksDB, TiKV, SQLite and IndexedDB
  variants and their stand-ins, which were in-memory stores under persistent names), so there is no web persistence
  plan to honour yet. A persistent store implements `KeyValueStore`; NSH's own storage (`nsh/storage`) is the natural
  first one.
- Left out: `SecureStorageService`: NSH has no keychain or keystore access yet, and the RayX.Old service only ever
  reported itself unavailable.

## Navigation (`services::navigation`)

Routes matched from URIs (`RouteMap`, `/sports/{sport}`, literal segments before parameters, query and fragment,
external and not-found status), named regions with their own journals (push, replace, reset, none; back and forward),
lifecycle subscribers (`on_navigated_from`, `on_navigated_to`), route and location events, and a
`NavigationLocationSynchronizer`. Registered with `add_navigation(|options| ..)`: `NavigationOptions` bind from
`RayX:Services:Navigation`, then the closure changes them; routes that do not build fail startup.

- Ported: the routes, journals, regions, service and their tests (`tests/navigation.rs`).
- Left out: `NavigationRouter`, `NavigationRegionView` and the transitions (`NavigationTransition`, screen effects,
  snapshot origins): GPUI views and GPUI screen effects. On NSH a screen renders the current route itself, and NSH
  animates its own screen changes; `NavigationOptions::default_transition` went with them.
- Left out: `BrowserNavigationLocationSynchronizer` and the browser location read at startup (`web-sys`): the core
  has no browser access. A synchronizer over NSH's web location is NSH glue for later; the default synchronizer does
  nothing.

## Selection (`services::selection`)

The app-wide summary of the active selection area: which area (`SelectionAreaId`) publishes, and its
`SelectionStatus` (participants, selected characters, collapsed, selecting), never the selected content. Only the
active area publishes; every route change clears it. Registered with `add_selection()` after the navigation.

- Ported: the service, its revisions and tests (`tests/selection.rs`).
- New: `subscribe(..)`, so a UI hears changes instead of polling the revisions (GPUI re-rendered and read them).
- Left out: `SelectionContextExt` and `SelectionGpuxExt` (GPUI globals and the `SelectionArea` element they fed).

## Input map (`services::input_map`)

Which action a key sequence or pointer gesture runs in a context, in three layers: the framework's and the
application's bindings (`InputMapOptions`, from code and `RayX:Services:InputMap`) and the user's document, kept in the
app's `KeyValueStore` and over the other two (`unbind` entries remove a lower layer's binding). `resolve(contexts,
binding)` tries contexts innermost first, then `""`; `bindings_for(action)` lists an action's shortcuts;
`apply_user_document(expected_revision, document)` validates, stores and then switches to the next revision; listeners
hear the changes. Registered with `add_input_map(|options| ..)`, which registers the storage too.

- Ported: the document model (`InputMapDocument`, entries, keyboard and pointer bindings, modifiers, validation) and
  the revisioned, stored user document.
- Changed: key sequences are validated by the model itself (`valid_key_sequence`), not by GPUI's `Keystroke::parse`;
  the service resolves bindings itself instead of compiling them into GPUI keymap layers, so any UI layer can feed it.
- Left out: `compile_input_layers` / `install_input_layers` (GPUI keymap layers and actions), the pointer binding
  recognizer (`PointerBindingRecognizer`, which reads GPUI mouse events), the migration of RayX.Old's legacy command
  keymap (GPUI's keymap format, and no NSH app stored one) and the diagnostics.

## Platform (`services::platform`)

`PlatformInfoService` holds a `PlatformSnapshot` of the device and the user's settings (OS family and version, form
factor, desktop behaviour, input capabilities, viewport, scale, safe area, keyboard height, colour scheme, font scale,
reduced motion, screen reader, locale, languages, time zone and regional settings) and notifies listeners of changes.
The UI layer keeps it current: `rayx_app`'s `use_platform_sync()` copies NSH's environment into it. Registered with
`add_platform()`.

- Ported: the snapshot (reworked onto what NSH reports), the service and its change listeners.
- Changed: per-window snapshots (`WindowHandle`, `current_for`, `try_update_window`) are gone: on NSH every surface
  (window) has its own root provider, so a surface's service describes that surface. The locale snapshot carries
  NSH's languages, time zone, hour cycle, first weekday, measurement system and temperature unit.
- Left out: the display topology (several monitors, display ids) and high contrast, which NSH does not report;
  `HostShape` (it was the form factor again); the provider traits (`PlatformInfoProvider`, `ViewportObserver`,
  `SafeAreaProvider`, …), which GPUI hosts implemented: `rayx_app` updates the service directly; the user-writable
  directory, which NSH does not expose (apps write through storage); `PlatformOptions` (it was empty).

## Suspension (`services::suspension`)

The app's lifecycle (not started, running, shutting down), activation (focus) and visibility (foreground,
background) as a `SuspensionSnapshot`, moved by the `SuspensionEvent`s the UI layer publishes (`rayx_app` publishes
NSH's `app_state()`), with listeners for every event and for changes of state. Registered with `add_suspension()`.

- Ported: the states, events and their rules, the service and its listeners.
- Changed: a change is a change of lifecycle, activation or visibility (an event that only moves the timestamp is not
  one); timestamps are optional and come from the system clock where the target has one, not from `js_sys` in
  WebAssembly (the core has no browser access); per-window snapshots are gone, as for the platform.
- Left out: the platform-specific reasons (Android activity, iOS application and scene, desktop window, browser page
  callbacks): NSH folds them into one app state, so the reason is `AppStateChanged` (or the host's start and stop);
  the browser visibility listener (`web-sys`); the platform snapshot inside each event (read the platform service);
  `SuspensionOptions` (it was empty).

## Application configuration (`services::application_configuration`)

The app's own settings, `ApplicationConfiguration` from the `RayX:Application` section and code: name and description
(startup only), motion policy, programmatic scrolling, tooltip and hover-card timing, notifications and layout
direction. `ApplicationConfigurationService` follows the configuration as it reloads (an options monitor), notifies
listeners, and gives the effective motion (the system's setting from the platform service, the app's policy, a
component's request) and programmatic scroll behaviour. Registered with `add_application_configuration(|options| ..)`,
which registers the platform service too; invalid settings fail startup.

The settings come from the app's bundled `appsettings.json`: `rayx_app`'s `AppSettingsSource` reads it with NSH's
asset reading (`asset:/` URLs through `fetch`), asynchronously, so the app builds with `RayXAppBuilder::build_async`
(`rayx_app::start_app` does).

- Ported: the settings that describe behaviour any UI layer shares, their validation, startup-only rules, revisions,
  effective motion and scrolling.
- Changed: unknown keys are ignored (as .NET does) instead of failing the bind, so a shared `appsettings.json` may carry
  keys of other parts; the app name no longer comes from the caller's `Cargo.toml` (`for_app_caller` read files at run
  time, which packaged and web apps cannot): set it in `appsettings.json` or code; reloads reach the service through
  the options monitor, where RayX.Old needed `replace_options` calls.
- Left out: scrollbar behaviour (GPUI drew its own scrollbars; NSH shows the platform's), cursor styles (gpux's
  `CursorStyle`), the accessibility tree mode and the wasm worker count (gpux settings), and `MotionDependencies`
  (re-rendering GPUI window roots on motion changes: NSH re-renders what reads the settings).
