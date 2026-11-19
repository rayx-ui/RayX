# Changelog

All notable changes to `rayx`, `rayx_app`, `rayx_core`, `rayx_runtime`, `rayx_app_macros` and
`rayx-cli` (the `rayx` command-line tool) are documented in this file. They share one version and
one release tag `vX.Y.Z`. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crates follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). It is generated from
Conventional Commits by release-plz.

## [0.13.0] - 2026-11-19

### Build and CI
- *(deps)* [**breaking**] Build on NSH nshx-0.6.0-3, which carries its theme format
  - Migration: move the app's `[patch.crates-io]` NSH entries to tag nshx-0.6.0-3 and its `nsh-*` requirements to 0.6.0, and replace the removed APIs. Light/dark no longer follows the system by itself: an app that wants it enables `rayx`'s `theme` feature and calls `addTheme()` / `add_theme`.

### Documentation
- *(components)* Call the intents intents in roles.json and the default theme

### Features
- *(components)* [**breaking**] Name the default theme's foreground tokens theme-colors-intents-*
  - Migration: RayX's default theme renames its tokens `theme-colors-roles-*` to `theme-colors-intents-*` (DTCG paths `theme.colors.roles.*` become `theme.colors.intents.*`). RayX's roles (`intent-*`, `surface-*`, `interaction-*`, …) do not change, so components and role-named classes need nothing. A theme patch or `updateTheme` role mapping keyed by an old token name is re-keyed: replace `theme-colors-roles-` with `theme-colors-intents-`.
- *(cli)* Build a project's NSH from a local checkout with `rayx nshx use`, and name the group nshx
- *(cli)* Enable the theme service in the apps `rayx new` writes
- *(cli)* Take `rayx nshx use local` and `latest`, and move NSH's version requirements with the switch
- *(components)* [**breaking**] Read RayX's roles from a default theme in NSH's format
  - Migration: an app's token tweaks move into a patch keyed by the new names, or re-point a role in the patch's `roles`. A key becomes its role (`theme::roles::*`), a modifier enum becomes `set_modifier` or the theme service's context, and `color!` becomes a hex value in a patch or a theme document.
- *(theme)* Add rayx_services_theme, which follows the system's light or dark
- *(contracts)* Add the theme contract, ThemeService, with its fake

### Fixes
- *(theme)* Pick the system's light or dark before a JS app's first frame
- *(cli)* Point NSH patch lines at a checkout by a path relative to their file, never an absolute one
- *(deps)* Build on NSH nshx-0.6.0-4, whose theme writes from a JS call reach its turn
- *(deps)* Build on NSH nshx-0.6.0-5, where TS's colorScheme() follows Rust

### Maintenance
- update Cargo.toml dependencies

### Tests
- *(theme)* Check that TS's colorScheme() follows the service's light/dark choice
- *(components)* Record every role's value from the starting theme
- *(cli)* Expect CI on main's schedule, release builds on tags only and the release pull request on demand
- *(cli)* Expect the release pull request on demand and the binaries built on the tag only
- *(cli)* Use made-up pins in the nshx use fixtures, so a pin bump leaves them alone
- *(cli)* Expect the theme feature in the rayx dependency rayx new writes
- *(cli)* Give the store's entry test made-up ids, so a pin bump leaves its order alone
- *(rayx)* Take the crates' folders relative to cargo's own workspace root
- *(rayx)* Read the copied NSH patch's relative paths from the workspace root in macro_paths

## [0.12.0] - 2026-11-18

### Build and CI
- Run and lint the umbrella with every feature, so its feature-gated test groups run

### Documentation
- *(history)* Put the storage hint's doc comment on its constant
- *(services)* Name the asset builder methods that take bundled files, and fix a redundant link
- *(nsh)* Say the host requires the four services and contracts are callable from any thread
- *(app)* Describe the core as it is, every contract and fake in it, with no old crate names

### Features
- *(storage)* Add TS's addInMemoryStorage()
- *(navigation)* [**breaking**] Make the navigation classes' commands promises
  - Migration: apps: await the navigation classes' commands (`await navigation.navigate(uri)`, `(await panel.goTo(0)).route`, `await panel.close()`) instead of reading their answers synchronously.
- *(history)* Let TS apps set the history's options
- *(history)* Keep the history in a store written in TS with addHistoryWithStore
- *(services)* [**breaking**] Platform service crate
  - Migration: apps: depend on `rayx_services_platform`, call `add_platform()` (`use rayx_services_platform::PlatformAppBuilderExt`), register its set with `rayx_services_platform::register(registry)` for TS, and read `platform_snapshot()` from it in place of `rayx_app_nsh::platform_snapshot`.
- *(services)* [**breaking**] Suspension service crate
  - Migration: apps: depend on `rayx_services_suspension`, call `add_suspension()` (`use rayx_services_suspension::SuspensionAppBuilderExt`) and register its set with `rayx_services_suspension::register(registry)` for TS; `NshSuspensionHost` and `with_suspension_host` are gone.
- *(services)* [**breaking**] Asset service crate
  - Migration: apps: depend on `rayx_services_assets`, call `add_assets(..)` and `add_asset_files(assets!())` from it (`use rayx_services_assets::AssetAppBuilderExt`) and register its set with `rayx_services_assets::register(registry)` for TS; `NshAssetReader`, `AssetReader` and `MemoryAssetReader` are no longer public (tests use `rayx_app::testing::FakeAssets`).
- *(services)* [**breaking**] Font service crate
  - Migration: apps: depend on `rayx_services_fonts`, call `add_fonts(..)` from it (`use rayx_services_fonts::FontAppBuilderExt`) with an asset service registered (`add_assets(..)`), and register its set with `rayx_services_fonts::register(registry)` for TS; `NshFontRegistrar` and `FontRegistrar` are no longer public.
- *(cli)* [**breaking**] Make new apps register the four services the host requires and name their features
  - Migration: apps: an app made by an earlier `rayx new` adds the features of the services it registers beyond the defaults to its `rayx` dependency (`features = ["storage", "operations", "history"]`) and registers the four the host requires, and operations before history, in its composition root: `add_platform()`, `add_suspension()`, `add_assets(..)`, `add_fonts(..)` and `add_operations()` in Rust, `addPlatform()`, `addSuspension()`, `addAssets()`, `addFonts()` and `addOperations()` in TSX, or registers its own implementations of those contracts.
- *(nsh)* Register a TS implementation of a contract from a builder method
- *(nsh)* Pin NSH to nshx-0.5.0-7
  - Migration: apps: patch every NSH crate in [patch.crates-io] to `nshx-0.5.0-7` and move rayx and rayx_app to this release's tag.
- *(rayx)* [**breaking**] Put each service crate behind a feature, the four the host requires by default
  - Migration: apps: the umbrella's defaults are `platform`, `suspension`, `assets` and `fonts`; add the feature of every other service the app registers to its `rayx` dependency (`features = ["storage", "operations", "history", "i18n"]`, ...), in Rust and in TSX (an app's set holds only the enabled crates' TS classes and builder methods). `rayx::navigation`, `rayx::history`, `rayx::operations` and `rayx::selection` are `rayx::services::navigation`, `rayx::services::history`, `rayx::services::operations` and `rayx::services::selection`; every service crate is `rayx::services::<name>`. A `#[rayx::native_module]` body no longer needs to register the service crates' sets.
- *(contracts)* [**breaking**] Contracts that publish, callable from any thread, and checked registrations
  - Migration: apps: implement a `#[contract]`'s `async fn` in Rust as a method returning `BoxFuture<'_, Result<T, E>>` (`Box::pin(async move { … })`); match `ContractError::EngineGone` where you matched `WrongThread`; `RayXAppBuilder::build()`/`build_async()` return `AppBuildError` (`Services` or `Configuration`) in place of `ConfigurationError`.
- *(app)* [**breaking**] Core service contracts, host hooks for service crates and the host's required services
  - Migration: apps: register the platform, suspension, asset and font services the start now requires, with `add_platform()`, `add_suspension()`, `add_assets(..)` and `add_fonts(..)` from `rayx_services_platform`, `rayx_services_suspension`, `rayx_services_assets` and `rayx_services_fonts` (in TS `addPlatform()`, `addSuspension()`, `addAssets(..)` and `addFonts(..)`, once each crate's set is registered), or your own implementations; remove `use_platform_sync()`, `add_nsh_services()` and `NshServicesAppBuilderExt`.
  - Migration: apps: resolve `dyn PlatformInfoService`, `dyn SuspensionService`, `dyn AssetService` and `dyn FontService` (from `rayx_app::services`) in place of the concrete services, reading their state with `platform()`, `suspension()`, `catalog()` and `fonts()`.
  - Migration: apps: register an `#[app_extension]` function with `registry.class_extension::<add_x>()` in place of `registry.module::<add_x>()`, and call it in TS as `builder.addX()` in place of `builder.apply(addX)`.
- *(services)* [**breaking**] Storage service crate
  - Migration: apps: depend on `rayx_services_storage`, call `add_storage()` or `add_in_memory_storage()` from it (`use rayx_services_storage::StorageAppBuilderExt`) and register its set with `rayx_services_storage::register(registry)` for TS; `NshKeyValueStore`, `PlatformKeyValueStore` and `with_key_value_store` are gone (register your own `dyn KeyValueStore` instead), and `InMemoryKeyValueStore` and `STORAGE_PREFIX` moved to `rayx_services_storage`.
  - Migration: apps: register a key-value store where the app calls `add_input_map(..)` or `add_persistent_history()` (`addInputMap`, `addPersistentHistory` in TS): they no longer add one.
  - Migration: apps: await the store's commands with owned buckets and keys (`store.get(bucket.into(), key.into()).await`) and the bucket's methods (`set_json`, `get_json`, `try_get_json`, `remove`, `keys`, `clear`), in place of the synchronous calls and the `_async` variants; in TS, await the `KeyValueStore` class's methods.
- *(services)* [**breaking**] Navigation service crate
  - Migration: apps: depend on `rayx_services_navigation` (or `rayx`), call `add_navigation(..)` from it (`use rayx_services_navigation::NavigationAppBuilderExt`), and register its set with `rayx_services_navigation::register(registry)` for TS's `addNavigation`, `NavigationService` and `NavigationRegion`; `with_navigation_service`, `with_navigation_location_synchronizer`, `NavigationLocationSynchronizer`, `NshLocationSynchronizer` and `NavigationSubscription` are gone (register your own `dyn NavigationService` to replace RayX's).
  - Migration: apps: resolve `dyn NavigationService` (from `rayx_app::services::navigation`) in place of the concrete service: await its commands (`navigate_to(uri).await`, `back(region).await`, `ensure_region(name).await` in place of `get_or_create_region`), read `current()`, `journal(region)`, `region_names()` and `location()` without a `Result`, and listen with `on_route_changed`, `on_region_closed` and the others, which return a `ChangeSubscription`.
  - Migration: apps: import `navigation_region`, `navigation_view`, `location_of_address`, `RouteDefinition`, `RouteMap` and `NavigationOptions` from `rayx_services_navigation` in place of `rayx_app_nsh` and `rayx_runtime::services::navigation`; `SelectionService::new` takes a `&dyn NavigationService`.
- *(services)* [**breaking**] I18n service crate, split into I18nService and MessageFormatter
  - Migration: apps: depend on `rayx_services_i18n`, call `add_i18n(..)` from it (`use rayx_services_i18n::I18nAppBuilderExt`) with platform and asset services registered (`add_platform()`, `add_assets(..)`), and register its set with `rayx_services_i18n::register(registry)` for TS (`builder.addI18n(..)`, `builder.addMessageFormatter()`, the `I18nService` class).
  - Migration: apps: resolve `dyn I18nService` (state and commands) and `dyn MessageFormatter` (formatting) from `rayx_app::services::i18n` in place of the concrete `I18nService`; commands are async (`set_language(..).await`), `follow_platform_again(..)` is `follow_platform()`, `subscribe(..)` is `i18n().subscribe(..)` over `I18nState`, and `with_i18n_service`, `I18nChangedEvent`, `apply_platform` and `load_assets` are gone (register your own `dyn I18nService` and `add_message_formatter()`).
  - Migration: apps: take `tr!`, `checked::tr!`, `translate`, `Args` and `args!` from `rayx_services_i18n` (or `rayx`'s prelude and `rayx::checked`) in place of `rayx_app_nsh`, and the TS classes `I18nService`, `I18nOptions`, `DateFormat` and `NumberFormat` from `rayx_services_i18n` in place of `rayx_app_nsh::ts`.
- *(services)* [**breaking**] Operation service crate
  - Migration: apps: depend on `rayx_services_operations` (or `rayx`), call `add_operations()` from it (`use rayx_services_operations::OperationsAppBuilderExt`) where the app calls `add_history()` or `add_persistent_history()` (`addOperations()` before `addHistory()` in TS), and register its set with `rayx_services_operations::register(registry)` for TS's `addOperations`, `OperationService` and `OperationTarget`; `with_operation_service`, `OperationSubscription` and `OperationService::new` are gone (register your own `dyn OperationService` to replace RayX's).
  - Migration: apps: resolve `dyn OperationService` (from `rayx_app::services::operations`) in place of the concrete service, register handlers with `register_handler::<T, _>(id, handler)` and await its calls (`submit(..).await`, `release_target(id).await` with an owned id, `begin_transaction(mode).await`, `transaction.commit().await`); `subscribe(..)` returns a `ChangeSubscription`, and `diagnostics()` reads the published state.
- *(services)* [**breaking**] History service crate
  - Migration: apps: depend on `rayx_services_history` (or `rayx`), call `add_history()`, `add_history_with_store(..)` or `add_persistent_history()` from it (`use rayx_services_history::HistoryAppBuilderExt`) after an operation service (`add_operations()` from `rayx_services_operations`, or the app's own `dyn OperationService`), and register its set with `rayx_services_history::register(registry)` for TS's `addHistory`, `addPersistentHistory` and `HistoryService`; import `HistoryStore`, `InMemoryHistoryStore`, `KeyValueHistoryStore`, `HISTORY_BUCKET`, `HistoryJournalV2` and the limits from `rayx_services_history`.
  - Migration: apps: resolve `dyn HistoryService` (from `rayx_app::services::history`) in place of the concrete service; await its commands with owned scope ids (`history.undo(scope.clone()).await`, `bind_target_scope(..).await`), read `can_undo`, `can_redo` and `stack_depths` without a `Result` (a persisted scope shows once it is bound or used), and listen with `subscribe(..)`, which returns a `ChangeSubscription`; `HistoryService::new`, `with_store`, `uses_operation_service`, `uses_history_store`, `HistorySubscription` and `clear_scope_async` are gone, and `HistoryError::Store` carries the store's message.
- *(services)* [**breaking**] Selection service crate
  - Migration: apps: depend on `rayx_services_selection` (or `rayx`), call `add_selection()` from it (`use rayx_services_selection::SelectionAppBuilderExt`) in place of `rayx_runtime`'s prelude, and register its set with `rayx_services_selection::register(registry)` for TS's `addSelection()` and `SelectionService`; `with_selection_service` and `SelectionService::new` are gone (register your own `dyn SelectionService` to replace RayX's).
  - Migration: apps: resolve `dyn SelectionService` (from `rayx_app::services::selection`) in place of the concrete service, await its commands (`activate(area, status).await`, `publish_status`, `deactivate`, `clear`), and read `active_area()`, `status()` and the revisions as before; register a `dyn NavigationService` before building, since an app without one no longer builds.
- *(services)* [**breaking**] Input map service crate
  - Migration: apps: depend on `rayx_services_input_map`, call `add_input_map(..)` from it (`use rayx_services_input_map::InputMapAppBuilderExt`) with a key-value store registered, and register its set with `rayx_services_input_map::register(registry)` for TS (`builder.addInputMap(..)`, the `InputMapService` and `InputMapOptions` classes).
  - Migration: apps: resolve `dyn InputMapService` from `rayx_app::services::input_map` in place of the concrete `InputMapService`; `load` and `apply_user_document` are async through the trait, `framework_document()` and `application_document()` return owned documents, `subscribe(..)` is `input_map().subscribe(..)` over `InputMapState`, and `InputMapService::new` and `with_input_map_service` are gone (register your own `dyn InputMapService`).
  - Migration: apps: take `InputMapOptions`, `INPUT_MAP_OPTIONS_SECTION` and `INPUT_MAP_STORAGE_BUCKET` from `rayx_services_input_map`, and the document model (`InputMapDocument`, `InputMapEntry`, `InputBinding`, `normalize_key_sequence`, …), `ResolvedInputAction` and `InputMapApplyError` from `rayx_app::services::input_map`.
- *(services)* [**breaking**] Application configuration service crate
  - Migration: apps: depend on `rayx_services_application_configuration`, call `add_application_configuration(..)` from it (`use rayx_services_application_configuration::ApplicationConfigurationAppBuilderExt`) with a platform service registered (`add_platform()`), and register its set with `rayx_services_application_configuration::register(registry)` for TS (`builder.addApplicationConfiguration(..)`, the `ApplicationConfigurationService` and `ApplicationConfigurationOptions` classes).
  - Migration: apps: resolve `dyn ApplicationConfigurationService` from `rayx_app::services::application_configuration` in place of the concrete `ApplicationConfigurationService`; `replace_options` is async, `subscribe(..)` is `configuration().subscribe(..)`, and `ApplicationConfigurationService::new` and `with_application_configuration_service` are gone (register your own `dyn ApplicationConfigurationService`).
  - Migration: apps: take `ApplicationConfiguration` and its parts, `ApplicationConfigurationSnapshot`, `EffectiveMotion`, `ScrollBehavior` and `ApplicationConfigurationError` from `rayx_app::services::application_configuration`, and `APPLICATION_CONFIGURATION_PATH` from `rayx_services_application_configuration`.
- *(app)* Fake the i18n service and the message formatter, every fake publishing alike
- *(app)* Register an implementation of a trait as the trait and a startup service

### Fixes
- *(services)* Serialize the suspension service's changes of state
- *(services)* Publish the suspension state only when the state changes
- *(storage)* [**breaking**] Keep no copy of NSH's storage, and read and write it on the engine thread
  - Migration: apps: a call of the NSH key-value store made off its surface's engine thread answers once the engine ran it, so await it instead of polling it once; a store of a surface whose engine is gone fails its calls.
- *(platform)* Type the snapshot PlatformInfoService.onChange passes in TS
- *(services)* Ask NSH whether its runtime is entered in the storage and navigation crates
- *(navigation)* Make every TS region command wait for the region, retrying a failed make
- *(navigation)* Reject a region's command when the region is disposed while it waits
- *(i18n)* Publish the i18n service's latest state when it changes on several threads
- *(i18n)* [**breaking**] Return promises from the I18nService class's commands
  - Migration: apps: await setLanguage, followPlatform and setTimeZone (or handle the promise) where the next line relies on the change or on an error being thrown.
- *(i18n)* Cover the i18n service's missing asset and platform service hints
- *(history)* [**breaking**] Replay by transaction id, keep edits recorded meanwhile, and load and save through an async store
  - Migration: apps: implement `HistoryStore`'s `load`, `save` and `delete_scope` returning `BoxFuture`s (`futures::future::ready(..).boxed()` for a store that answers at once; the `*_async` methods are gone), and await them where the store is called directly.
- *(history)* Finish a command given up on, save over a stale stored revision, refuse a redo an edit raced
- *(history)* Keep an entry another history undid out of the merged journal
- *(history)* Report a failed replay once when its caller drops the answer unread
- *(history)* Draw entry-id prefixes from real entropy, and document unbounded commits
- *(history)* Delete a cleared scope's journal from the store
- *(history)* [**breaking**] Delete a cleared scope's journal only at the revision the history expects
  - Migration: apps: a HistoryStore implementation's delete_scope takes the expected revision and returns Result<bool, _> (delete only when the stored journal is at that revision, else answer false); a TS HistoryStoreContract's deleteScope(scope, expectedRevision) returns Promise<boolean> the same way.
- *(history)* Hold a journal's check and write under one lock per key-value store and scope
- *(storage)* Share one NSH store between the surfaces over one NSH storage
- *(history)* Lock a journal per bucket and scope in the process, whatever store object
- *(services)* [**breaking**] Declare the values TS classes give and take with their types
  - Migration: apps: a TS implementation of NavigationServiceContract returns a NavigationResult whose direction, status and route type-check as NavigationDirection, RoutingStatus and NavigationRoute; replaceOptions takes an ApplicationConfiguration, not a value typed unknown.
- *(cli)* Await the key-value store in a new mixed app's launch counter
- *(operations)* Keep a command's arguments in the call queued for TS, not in the caller's future
- *(nsh)* Queue the engine runner's work where JS runs Rust code outside a call
- *(app)* [**breaking**] Declare a contract command's result as a promise in TS
  - Migration: apps: a TS implementation of a contract command returns a promise (an async method) to type-check; a method returning the value at once still runs.
- *(nsh)* Run the work queued for the engine as the app quits, before the engine stops
- *(nsh)* Wait at quit for engine work already running, within the quit's limit
- *(nsh)* Stop a cancellable engine run once its caller drops it, for history's call timers
- *(i18n)* Follow every state the i18n service publishes in the message formatter
- *(app)* Publish the fake input map's state outside its lock
- *(app)* Make TS implementations of contracts before other startup factories run
- *(operations)* [**breaking**] Carry a transaction by its id, entering it on the thread for a synchronous body only
  - Migration: apps: submit into a transaction with `transaction.submit(..)` / `transaction.resubmit(..)`, or wrap the synchronous code that calls `operations.submit(..)` in `transaction.enter(|| ..)`; `current_transaction_id()` and `begin_transaction(Join | Reject)` see only a transaction entered that way.
- *(operations)* Refuse a submission into a transaction that is not open
- *(operations)* Release a target's handle that TS never took
- *(app)* Make FakeI18n, FakeHistory and FakeInputMap behave as RayX's services do
- *(nsh)* Take a typed array or an ArrayBuffer where a TS contract answers bytes
- *(operations)* State that transaction ids are unique and increasing per service
- *(history)* [**breaking**] Report unheard failures, rebase on another writer's journal, time out silent calls, keep the replay order
  - Migration: apps: handle `HistoryError::TimedOut` where they match `HistoryError` exhaustively; set `HistoryOptions::call_timeout_ms` (or register a `HistoryTimer` outside NSH) to change the history's call timeout.
- *(history)* [**breaking**] Keep each session's stacks its own, merge journals by entry id, never cut a commit short
  - Migration: apps: give `HistoryEntry` values they construct an `id` (any string unique to the entry); entries deserialized without one get an empty id.
- *(app)* [**breaking**] Let the last registration of a key win completely
  - Migration: apps: an implementation that loads at start and was registered with add_startup_singleton(Arc::clone(&x)) plus add_singleton_as::<dyn T>(x) registers with add_startup_singleton_for::<dyn T, _>(x) instead; one that relied on replace_service_as::<dyn T> to drop a startup service registered as another type registers that one as dyn T (add_startup_service_for).
- *(app)* [**breaking**] Tie a registration's requirements to the registration that declared them
  - Migration: apps: declare a registration's require_for::<S, T> before registering S; one declared after S's registration moves to the next registration of S.
- *(app)* Give a trait the startup role of an implementation naming its own key
- *(app)* Find a published operation target in any order

### Refactoring
- *(app)* Group the NSH host's files by topic and by service
- [**breaking**] Rename rayx_app to rayx_app_nsh and rayx_core to rayx_app
  - Migration: apps: depend on `rayx_app` for `rayx_core` and on `rayx_app_nsh` for the old `rayx_app`; `rayx::core` is `rayx::app` and `rayx::app` is `rayx::app_nsh`.
- [**breaking**] Remove rayx_runtime, every service being a crate of its own
  - Migration: apps: depend on the service crates (or the `rayx` umbrella's features) in place of `rayx_runtime`, and import services from `rayx_app::services::<name>` and builder extensions from `rayx_services_<name>`.

### Tests
- *(services)* Check that RayX's platform service publishes only real changes
- *(i18n)* Check the I18nService class's declaration in the i18n crate's set
- *(history)* Answer every waiting save after the history's lock is poisoned
- *(cli)* Expect the workspace job to free the runner's disk and build without debug info
- *(services)* Resolve a registered service class whose service is missing
- *(services)* Restore the coverage lost when the services moved to their crates
- *(nsh)* Cover work queued off the engine thread as the app quits
- *(rayx)* Replace RayX's implementations with an app's own, every consumer using them
- *(services)* Implement each of the twelve service contracts in TS
- *(rayx)* Undo and redo through RayX's own operation service, end to end
- *(rayx)* Spell the old package names in parts so the docs check finds none
- *(app)* Cover a startup dependency on a startup service's registration key
- *(app)* Use every fake by its trait in a plain provider

## [0.11.1] - 2026-11-17

### Features
- *(nsh)* Pin NSH to nshx-0.5.0-6
  - Migration: apps: patch every NSH crate in [patch.crates-io] to `nshx-0.5.0-6` and move rayx and rayx_core to this release's tag.
- *(nsh)* Pin NSH to nshx-0.5.0-5
  - Migration: apps: patch every NSH crate in [patch.crates-io] to `nshx-0.5.0-5` and move rayx and rayx_core to this release's tag.

### Fixes
- *(tsx)* Validate TS options after every configure step

### Tests
- *(app)* Write the dev tools switch probe's rayx_app path with forward slashes

## [0.11.0] - 2026-11-16

### Features
- *(devtools)* Add the dev tools' agent protocol crate
- *(cli)* Make new apps with the dev tools' layer
- *(cli)* Make new apps with RayX's app builder
- *(components)* Add rayx_components with RayX's theme and Button, and the gallery app
- *(components)* [**breaking**] Move RayX's theme onto NSH theme documents and typed keys, on nshx-0.5.0-4
  - Migration: apps: read tokens through `rayx_components::theme::keys` (`keys::theme::colors::roles::primary::BG.get()`, or `value(key)`) instead of `rayx().theme.…`; `bind_text_style` takes `text_style_keys!(path)` instead of a closure.
  - Migration: apps: switch modes with `setModifier("accent", "sky")` / `Accent::Sky.set()` instead of `Themes.set(..)` / `set_theme(RayXTheme::of(..))`.
  - Migration: apps: make custom themes with NSH's `update_tokens( Tokens::default().with(KEY, color!(..)))`, `reset_tokens`, theme files and `ThemeScope` instead of `rayx_theme!`, `rayx_patch!`, `update_theme` and `reset_theme`.
  - Migration: apps: pin NSH to `nshx-0.5.0-4` with RayX.
- *(app)* [**breaking**] Add the dev tools' agent and layer to dev builds
  - Migration: apps: Rust code that calls `AppServices::load()` directly passes the `System` it has (TSX's `AppServices.load()` is unchanged); mount `DevLayer` once at the root, absolute (`.child(dev_layer())`, `<DevLayer style={{ position: "absolute" }} />`), to get the dev tools' layer in dev builds.
- *(core)* [**breaking**] Key services at run time, keep thread-bound ones and make startup services asynchronously
  - Migration: apps: `StartupInitializable::startup_service_type` returns `Option<StartupServiceType>`: wrap the key in `Some(..)`, or leave the method out to key the service by its registration.
  - Migration: apps: `StartupServiceType` is `Clone`, no longer `Copy`, and `name()` returns `&str`; `StartupRegistry::services()` returns a `Vec`.
- *(app)* [**breaking**] Build, start and resolve RayX apps from TypeScript as from Rust
  - Migration: apps: replace `rayx_app::set_app(build)` and `rayx_app::start()` with `build(RayXAppBuilder::new()).start()` (or `.start_async().await`) in the root.
  - Migration: apps: in TSX, replace `await AppServices.load()` with `const services = await new RayXAppBuilder()…start()`, `app.scope()` / `scope.child()` with `services.createScope()`, and `X.of(app)` with `services.getRequiredService(X)` under the Rust service names (`I18n` → `I18nService`, `Navigation` → `NavigationService`, `Storage` → `KeyValueStore`, `Platform` → `PlatformInfoService` and `SuspensionService`, …).
  - Migration: apps: keep the function an `onChange` returns to stop that listener; Rust bridges take `Instance<ServiceProvider>`.
- *(app)* [**breaking**] Write services in TypeScript in RayX's one container, and give TSX apps a one-line crate
  - Migration: apps: a TSX app's crate becomes `rayx::native_module!("my-app")` (with a block for its own exports) in place of a hand-written `#[native_module]` that registers RayX's classes.
  - Migration: apps: app-defined Rust services reach a TS-built app through `#[rayx::app_extension]` functions and `builder.apply(..)`.
- *(app)* [**breaking**] Declare an app crate's UI set with the #[rayx::native_module] attribute
  - Migration: apps: replace `rayx::native_module!("my-app", |registry| { … })` with `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) { … }`.
- *(core)* [**breaking**] Mark ServiceResolutionError and AppError non-exhaustive
  - Migration: apps: a `match` over `ServiceResolutionError` or `AppError` outside RayX needs a wildcard arm (`_ => …`).
  - Migration: apps: `ServiceResolutionError` has new variants: `MissingKey { service }` (a runtime key with nothing registered), `WrongThread { service }` (a thread-bound service resolved on another thread) and `Failed { service, message }` (a factory's error).
  - Migration: apps: `AppError::NoApp` is gone (there is no `set_app` to name a builder), and `AppError::BuilderStarted` is new (a TS builder used after `start()`).
  - Migration: apps: `rayx_app::start_app()` is gone: call `RayXAppBuilder::start()` (a task) or `.start_async().await` (`StartAppBuilderExt`) on the app's builder.
  - Migration: apps: the Rust structs behind the TS service classes moved to `rayx_app::ts` under the Rust services' names (`I18n` → `ts::I18nService`, `Navigation` → `ts::NavigationService`, `Storage` → `ts::KeyValueStore`, `Platform` → `ts::PlatformInfoService` and `ts::SuspensionService`, `Assets` → `ts::AssetService`, `Fonts` → `ts::FontService`, `AppConfiguration` → `ts::ApplicationConfigurationService`, `NavigationRegion` → `ts::NavigationRegion`), with the i18n class's value types (`DateFormat`, `DateLength`, `NumberFormat`, `NumberKind`, `TimeDetail` → `rayx_app::ts::…`). They are no longer in `rayx_app`'s root or prelude; Rust bridges take `Instance<ServiceProvider>` and resolve through it.
  - Migration: apps: declare an app crate's UI set with `#[rayx::native_module("my-app")] fn native(registry: &mut Registry) { … }`. The `rayx::native_module!` named in an earlier note of this release was replaced before the release.

### Tests
- *(cli)* Prove the dev tools on apps made by rayx new
- *(release)* Expect migration notes read from the commit's raw message

## [0.10.1] - 2026-11-15

### Features
- *(nsh)* Pin NSH to nshx-0.5.0-3
  - Migration: apps: patch every NSH crate in [patch.crates-io] to `nshx-0.5.0-3` and move rayx and rayx_core to this release's tag; on GPUI, Return and Space on a focus a newer dialog covers go to the dialog now (upstream NSH's rule).

## [0.10.0] - 2026-11-15

### Features
- *(nsh)* Pin NSH to nshx-0.5.0-2
  - Migration: apps: patch every NSH crate in [patch.crates-io] to `nshx-0.5.0-2` and move rayx and rayx_core to this release's tag; on Windows and GPUI, Tab and the focus a popup or sheet gives back are upstream NSH's now (the fork's own were dropped in nshx-0.5.0-2).
- *(app)* [**breaking**] Follow deep links on every platform and take NSH 0.5's lifecycle and environment
  - Migration: apps: move rayx and rayx_core to this release's tag; handle the new `PointerCapability`, `SuspensionEventKind` and `SuspensionEventReason` variants where a match names them; fill the new fields where a `PlatformSnapshot`, `InputCapabilities` or `SuspensionSnapshot` is built literally (or start from `PlatformSnapshot::desktop` / `handheld` and `..Default::default()`); pass the navigation's base path to `location_of_address`; keep `base_path` at `nsh.json`'s `web.base` on every platform; follow links through the navigation rather than `UrlScheme.on("open")`.

### Tests
- *(cli)* Expect nsh 0.5's test runner in a new app's generated workspace

## [0.9.0] - 2026-11-12

### Features
- *(nsh)* [**breaking**] Move to NSH 0.5, pinned to nshx-0.5.0-1
  - Migration: gamify: set the NSH crates to "0.5.0" in src/frontend/Cargo.toml, patch every NSH crate in src/frontend/.cargo/config.toml to `nshx-0.5.0-1`, set the navigation's `base_path` to the web base, handle `PlatformFamily::Tizen` where it matches the family, apply NSH's own nshx-0.5.0-1 changes (GPUI's `set_theme_overrides` is upstream's, `UrlScheme`'s `change` gives `reason`), and move rayx_core and rayx_app to this release's tag.

## [0.8.1] - 2026-11-09

### Features
- *(i18n)* Let dXn.I18n's t! and args! format with the i18n service
- *(i18n)* Check tr! calls against the app's messages when it builds

### Maintenance
- *(deps)* Move serde_spanned, toml_datetime, toml_edit and toml_writer to their newest compatible versions in Cargo.lock, for dxn_i18n_macros' toml 1.2

## [0.8.0] - 2026-11-09

### Build and CI
- *(nsh)* Pin NSH to nshx-0.4.0-1, which fork-0.4.0-1 was renamed to
- *(nsh)* Pin NSH to nshx-0.4.0-2
- *(nsh)* Pin NSH to nshx-0.4.0-4

### Documentation
- *(storage)* Name NSH's persistent store in the storage module's docs

### Features
- *(i18n)* [**breaking**] Show the language the app picks in NSH too
  - Migration: apps that build an `I18nChangedEvent`, or destructure one without `..`, add the `chosen` field.
- *(app)* [**breaking**] Run ready hooks once the app's services started
  - Migration: apps that match `AppError` exhaustively add an arm for `AppError::Ready`.
- *(platform)* [**breaking**] Give the platform and suspension services a desktop's displays and window
  - Migration: apps that build `PlatformSnapshot`, `SuspensionSnapshot` or `SuspensionEvent` with literals add the new fields (or `..Default::default()` for the snapshots), and apps that match `SuspensionEventKind` or `SuspensionEventReason` exhaustively add arms for the new variants.
- *(assets)* Catalog the files a crate ships from NSH's assets!() list
- *(i18n)* Add tr!, the app's messages in a Rust component

### Fixes
- *(cli)* Patch the NSH crates of nsh's generated workspace in a new app
- *(i18n)* Let the app's translations override its libraries'
- *(input-map)* Match key sequences whatever their spelling
- *(platform)* Give startup services the device, not the snapshot's defaults
- *(fonts)* Register the fonts the app ships once, by NSH
- *(deps)* Take dxn_i18n and dxn_intl v0.1.2

## [0.7.0] - 2026-11-07

### Features
- *(nsh)* [**breaking**] Move to NSH 0.4, pinned to fork-0.4.0-1
  - Migration: gamify: set the NSH crates to "0.4.0" in src/frontend/Cargo.toml, patch every NSH crate in src/frontend/.cargo/config.toml to `fork-0.4.0-1`, install the nsh CLI from that tag, and move rayx_core and rayx_app to this release's tag.

## [0.6.3] - 2026-11-02

### Build and CI
- *(nsh)* Pin NSH to fork-0.3.0-4, in RayX and as rayx's default

### Features
- *(app)* Show the navigation's location as NSH's and follow the platform's moves

## [0.6.2] - 2026-11-02

### Fixes
- *(cli)* Close rayx serve's connections without a reset

## [0.6.1] - 2026-11-02

### Build and CI
- *(release)* Find rayx-cli by the v* tags

### Documentation
- *(services)* Describe the app services as a reference

### Features
- *(runtime)* Show instants in the platform's time zone
- *(app)* Show instants in the user's time zone and format dates and numbers for TSX

### Maintenance
- Name no consumer app in code, tests and docs
- Replace the betting examples in comments and tests with neutral ones

## [0.6.0] - 2026-11-02

### Build and CI
- Check rayx-cli on Linux, macOS and Windows
- *(nsh)* Pin NSH to fork-0.3.0-3, in RayX and as rayx's default

### Documentation
- *(cli)* Fold RayX.CLI's contributor rules and decisions into RayX's docs

### Features
- *(cli)* Import RayX.CLI as the rayx-cli crate
- *(cli)* Ship rayx with RayX's releases, on RayX's version
- *(cli)* Install and update rayx from rayx-ui/RayX
- *(cli)* Make rayx's default NSH pin RayX's own
- *(cli)* Scaffold RayX apps with rayx new
- *(cli)* Serve index.html for an app's own routes in rayx serve
- *(runtime)* [**breaking**] Chain add_selection like the other add_* methods
  - Migration: an app that registers the selection service drops the `?` (or `.unwrap()`) after `add_selection()`.
- *(runtime)* Keep history in the app's key-value store
- *(runtime)* [**breaking**] Give navigation one location for all its regions, taken in from the platform
  - Migration: a `NavigationLocationSynchronizer` takes `synchronize(&self, location: &str, mode)` and may answer `location()` with the platform's location at startup.
- *(rayx)* Put rayx_app::services in the prelude
- *(app)* Keep the app's key-value store in NSH's storage
- *(app)* Give TSX the navigation, storage, configuration, asset, font and platform services
- *(app)* Bind dXn.Reactive view models to NSH signals and TSX classes
- *(app)* Show navigation regions as NSH navigation stacks, with a TSX class

### Fixes
- *(macros)* Let #[derive(AppService)] name the rayx umbrella

### Refactoring
- *(core)* [**breaking**] Remove ShutdownService and the AppScope, WindowScope and ViewScope wrappers
  - Migration: a service that implemented `ShutdownService` releases what it holds in `Drop` instead.

### Tests
- *(cli)* Give the Lima check test a df of its own

## [0.5.1] - 2026-10-29

### Build and CI
- *(runtime)* Add the rayx_runtime crate for RayX's services

### Features
- *(runtime)* Add the operation service
- *(runtime)* Add the history service
- *(runtime)* Add the in-memory key-value storage service
- *(runtime)* Add the navigation service
- *(runtime)* Add the selection service
- *(runtime)* Add the input map service
- *(runtime)* Add the platform service
- *(runtime)* Add the suspension service
- *(runtime)* Add the application configuration service
- *(runtime)* Add the asset service
- *(runtime)* Add the font service
- *(runtime)* Add the i18n service
- *(core)* Let services keep change listeners
- *(rayx)* Add the rayx umbrella crate with a prelude
- *(app)* Run RayX's services on NSH
- *(app)* Give TSX the i18n service

## [0.5.0] - 2026-10-27

### Build and CI
- *(nsh)* [**breaking**] Pin NSH to fork-0.3.0-1
  - Migration: patch NSH to `fork-0.3.0-1` (or a newer `fork-0.3.0-*` tag) in `[patch.crates-io]`, depend on the NSH crates at `0.3.0`, and move the app's own NSH imports to NSH 0.3's modules (`nsh_core::ui::*` to `nsh_core::prelude` or `nsh_core::element`, TSX `nsh/device` and `nsh/animated` to `nsh`, `Input` to `nsh/base`).

### Features
- *(core)* [**breaking**] Add configuration sources, .NET's options model and RayXAppBuilder
  - Migration: register options with `services_mut().add_options::<T>()` (`.bind("Section")`, `.configure(..)`, `.validate(..)`) or `services_mut().configure::<T>(move |o| *o = options.clone())` instead of `configuration_mut().options_value(options)`, and read them with `options::<T>()?` instead of `required_options::<T>()?`.
  - Migration: replace the test builders that implement `AppBuilder` and call `build_options_sync` and `with_added_options` with `RayXAppBuilder::new()` and `.build()?`, and rename `NshAppBuilder` to `RayXAppBuilder` (`set_app` takes `fn(RayXAppBuilder) -> RayXAppBuilder`).

### Refactoring
- *(core)* [**breaking**] Rename rayx_app_di to rayx_core
  - Migration: depend on `rayx_core` instead of `rayx_app_di` (same tag) and rename the `rayx_app_di::` paths to `rayx_core::`, including any dependency checks.

## [0.4.0] - 2026-10-23

The first release of RayX on [Native Shell (NSH)](https://github.com/dXnSix/nsh). RayX moves from
GPUI and gpux to NSH; the GPUI-based RayX is now `RayX.Old`. The workspace starts from the crates of
`nsh-app` `v0.3.0` (`19bd4c4`), renamed with the repository, and its version continues
`nsh-app`'s.

### Added

- `rayx_app_di` (was `nsh_app_di`): the pure-Rust core with RayX's DI API: a `ServiceProvider` with
  singleton, scoped and transient services, scopes, options, configuration and the `AppBuilder`,
  and startup services initialized asynchronously in dependency order, each within its timeout. It
  has no NSH dependency and builds for every target, wasm32 included.
- `rayx_app` (was `nsh_app`): the NSH layer: one root provider per engine surface (`set_app`,
  `install`), startup from TSX (`AppServices.load()`) or a Rust root (`start`, `app_state`), a scope
  per screen (`use_screen_scope`, `ServiceScope`) and view models activated with their screen
  (`use_view_model`).
- `rayx_app_macros` (was `nsh_app_macros`): `#[derive(AppService)]`, re-exported by both layers.

### Changed

- `rayx_app` activates view models with `dxn_reactive` `v1.1.0` (formerly `rayx_reactive`) and
  re-exports its `ActivatableViewModel`, `ActivationToken` and `ViewModelActivator`.
- The workspace builds with the NSH fork's tag `fork-0.2.0-4`; the NSH layer
  test builds its engine config with `EngineConfig::new`, which that tag added.

### Migration

- Coming from `nsh-app` `v0.3.0`: depend on `rayx_app_di`/`rayx_app` from
  `{ git = "https://github.com/rayx-ui/RayX", tag = "v0.4.0" }` instead of `nsh_app_di`/`nsh_app`,
  rename their paths (`nsh_app_di::` to `rayx_app_di::`, `nsh_app::` to `rayx_app::`), and move to
  `dxn_reactive` `v1.1.0`.
