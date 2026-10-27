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
