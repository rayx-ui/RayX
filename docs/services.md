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

On NSH (`rayx_app`): every surface's builder starts with `NshAssetReader` and `NshFontRegistrar` (the asset and font
services' platform parts: `fetch` of `asset:/` URLs, `register_font`); `add_app_settings(package)` adds the package's
bundled `appsettings.json` as a configuration source (`AppSettingsSource`, loaded asynchronously by
`RayXAppBuilder::build_async`, which `start_app` uses); `use_platform_sync()`, in the root component, keeps the platform
and suspension services current from NSH's environment and app state once the app is ready. TSX apps have no root
component of their own to call it from; their platform and suspension services keep their defaults until a Rust
component calls it.

The services, and what each ports and leaves out:

## App (`services::app` in RayX.Old)

The builder (`AppBuilder`, `ConfigurationAppBuilderExt`, `ServiceAppBuilderExt`) went to `rayx_core::services::app` in
`v0.5.0`, with `RayXAppBuilder`. Left out: `AppContext`, a GPUI global with the platform snapshot (on NSH, the surface's
root provider is `rayx_app::app_services()`, and the platform service holds the snapshot), and `GpuxServiceAppExt`.

## Collaboration (`services::collaboration` in RayX.Old): not ported yet

It is a thin integration (sessions over one storage, operations projected onto the operation service, the origin
policy, remote changes) of RayX.Old's `rayx_collab`, an Automerge engine that is RayX.Old's crate and cannot be a
dependency here. By the agreed rule for reusable code, the engine becomes a standalone `dXn` crate (UI-free, wasm32)
that RayX consumes by tag; the integration comes with it. Until then RayX has no collaboration service.

## I18n (`services::i18n`)

`I18nService`, a startup service after the asset service, wraps [dXn.I18n](https://github.com/dXnSix/dXn.I18n)'s
`Translator` (MessageFormat 2 messages, bundles, fallback lookup) and formats numbers, dates and plurals with
[dXn.Intl](https://github.com/dXnSix/dXn.Intl) (ICU4X) for the same locale. It loads the asset catalog's translations
(`locales/<language>.json`, or `locales/<language>/<namespace>.json` whose ids a namespace other than the default one
prefixes: `checkout.title`); follows the platform service's preferred languages and clock (the best of the app's
languages for the user's list, with the user's region and `-u-hc-` hour cycle) until the app picks a language with
`set_language`; looks messages up through the language's parents, the fallback languages and the default one; and tells
subscribers about every change. TSX reaches it through `rayx_app`'s `I18n` class (`I18n.of(app).t("cart", { n: 3 })`,
`setLanguage`, `onChange`). Registered with `add_i18n(|options| ..)`, which registers the asset service too;
`I18nOptions` bind from `RayX:Services:I18N`.

- Ported: the default and fallback languages, the resources path, namespaces, the missing-key policy, language changes
  and their subscribers, following the user's languages.
- Changed: messages are MessageFormat 2 (RayX.Old's were i18next-style `{{name}}` interpolation with `_one`/`_other`
  plural keys), with CLDR plurals for every language and numbers and dates formatted in Rust, the same on every
  platform; resources come from the asset catalog, every language at startup, so changing the language is immediate.
- Left out: the separators and interpolation markers (`namespace_separator`, `key_separator`, `interpolation_*`,
  `plural_separator`, `context_separator`: MessageFormat 2 has its own syntax), `default_date_format` and
  `default_number_format` (patterns: messages and `format_number`/`format_date_time` take MessageFormat 2 and dXn.Intl
  options), and the scope helper (`I18nScope`): a key prefix is a namespace now.

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

## Assets (`services::assets`)

The files the app's packages ship in their `assets/` directories (the only place assets live), listed in a neutral
manifest per package, `assets.toml` by default (`AssetOptions::manifest`): `files = [..]` for plain files, `[[asset]]`
tables for files with a kind, density `scales`, tags, locale, theme, platform, title, description, size and custom
data. Density variants are `@1.5x`, `@2x`, `@3x` and `@4x` file names (as NSH's images load them), declared with
`scales` or listed and folded into their base; there are no density folders. `AssetService`, a startup service, reads
the manifests of `AssetOptions::packages` (the app's own first) into an `AssetCatalog`, gives each asset's
`asset:/@<package>/<path>` URL (and its variant for a scale) for NSH's elements, answers queries, and loads bytes,
text and JSON through the platform's `AssetReader` (`rayx_app` reads with NSH's `fetch`; `MemoryAssetReader` holds
files in memory), keeping them within a byte budget. Manifest problems are diagnostics or startup failures, as
`AssetValidationPolicy` says. Registered with `add_assets(|options| ..)`, options bound from `RayX:Services:Assets`.

- Ported: asset kinds and MIME types from file names, the byte cache, queries, validation policies and diagnostics,
  the service's lookups and loads.
- Changed: the manifest is TOML and neutral (RayX.Old's was a JSON index and manifest pair written by its tooling);
  assets are read through NSH's bundled-file reading, so the same code serves desktop, mobile and web.
- Left out: the filesystem and browser backends and their roots (development, installed, wasm public roots,
  `RAYX_ASSETS_ROOT`): NSH packages each crate's `assets/` and serves `asset:/` URLs itself; hot reload and per-file
  change events (a session restart reads assets again); writable assets, import, move and delete (the editor
  features of RayX.Old's asset manager, `AssetManager` and its snapshots, previews and filters); content sniffing of
  kinds (kinds come from the manifest or the file name); `AssetRenderSource` (GPUI image sources: NSH loads images
  from the URL).

## Fonts (`services::fonts`)

`FontService`, a startup service after the asset service, registers the asset catalog's font files (`ttf`, `otf`,
`woff`, `woff2`) with the platform at run time through the `FontRegistrar` trait (`rayx_app` registers with NSH's
`register_font`, which reads the family name), registers other fonts from their data while the app runs
(`register_font(data)`), and resolves the aliases the app names fonts by: an alias's family, its load fallback
(another alias or a family) when that family did not load, and its glyph fallbacks, as one family list for NSH's
`font_family` / `fontFamily` fallback lists (`font_family("body")` is `Inter, Noto Color Emoji`). Registered with
`add_fonts(|options| ..)`, which registers the asset service too; `FontOptions` bind from `RayX:Services:Fonts`.

- Ported: the options (aliases, load and glyph fallbacks, default alias, automatic registration of font assets),
  their validation, alias resolution and diagnostics.
- Changed: fonts are registered while the app runs (NSH's `register_font`), not added to GPUI's text system before
  the first window; the platform reads the family name, so WOFF and WOFF2 fonts work too; resolved aliases are family
  lists instead of GPUI `Font` values with `FontFallbacks`.
- Left out: reading font metadata with `ttf-parser` (weight, style, variable axes: NSH reads what it needs, and
  `ttf-parser` reads no WOFF), the variable-axes diagnostic, and GPUI's `Font` conversions.
