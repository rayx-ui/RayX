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
