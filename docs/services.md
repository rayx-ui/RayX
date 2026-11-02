# App services

`rayx_runtime` holds RayX's app services. Like `rayx_core`, it has no NSH dependency and builds for
`wasm32-unknown-unknown`; what a service needs from the platform is a trait in `rayx_runtime` that `rayx_app` implements
on NSH.

## Registering and configuring

Every service is registered on the app builder through an `AppBuilder` extension trait: `add_<service>(..)` keeps a
service registered earlier, `with_<service>_service(..)` replaces it. A service reads its options with `rayx_core`'s
options model from the configuration section `RayX:Services:<Name>` (the application configuration from
`RayX:Application`). Options bind with the core's key matching, so `appsettings.json` may spell keys in camelCase.

Resolve a service from the provider: `services.required_service::<T>()`, or `rayx_app::services()` in a component.

| Service | Registered with | Options section |
| --- | --- | --- |
| [I18n](#i18n) | `add_i18n(..)` | `RayX:Services:I18N` |
| [Operations](#operations) | `add_operations()` | |
| [History](#history) | `add_history()`, `add_persistent_history()`, `add_history_with_store(store)` | |
| [Storage](#storage) | `add_storage()` | |
| [Navigation](#navigation) | `add_navigation(..)` | `RayX:Services:Navigation` |
| [Selection](#selection) | `add_selection()` | |
| [Input map](#input-map) | `add_input_map(..)` | `RayX:Services:InputMap` |
| [Platform](#platform) | `add_platform()` | |
| [Suspension](#suspension) | `add_suspension()` | |
| [Application configuration](#application-configuration) | `add_application_configuration(..)` | `RayX:Application` |
| [Assets](#assets) | `add_assets(..)` | `RayX:Services:Assets` |
| [Fonts](#fonts) | `add_fonts(..)` | `RayX:Services:Fonts` |

## On NSH

Every surface's builder starts with `rayx_app`'s `NshAssetReader` and `NshFontRegistrar` (the asset and font services'
platform parts: `fetch` of `asset:/` URLs, `register_font`) and `NshKeyValueStore` (the app's persistent key-value
store). `add_app_settings(package)` adds the package's bundled `appsettings.json` as a configuration source
(`AppSettingsSource`, loaded asynchronously by `RayXAppBuilder::build_async`, which `start_app` uses).

`use_platform_sync()`, in a Rust root component, keeps the platform and suspension services current from NSH's
environment and app state once the app is ready. A TSX app has no root component, so `AppServices.load()` does the same.

## I18n

`I18nService`, a startup service after the asset service, wraps [dXn.I18n](https://github.com/dXnSix/dXn.I18n)'s
`Translator` (MessageFormat 2 messages, bundles, fallback lookup) and formats numbers, dates and plurals with
[dXn.Intl](https://github.com/dXnSix/dXn.Intl) (ICU4X) for the same locale, the same on every platform.

- It loads the asset catalog's translations: `locales/<language>.json`, or `locales/<language>/<namespace>.json`, whose
  ids a namespace other than the default one prefixes (`checkout.title`). Every language loads at startup, so changing
  the language is immediate.
- It follows the platform service's preferred languages and clock (the best of the app's languages for the user's
  list, with the user's region and `-u-hc-` hour cycle) until the app picks a language with `set_language`.
- It looks messages up through the language's parents, the fallback languages and the default one, and tells
  subscribers about every change.
- Options: the default and fallback languages, the resources path, namespaces and the missing-message policy.

TSX reaches it through the `I18n` class: `I18n.of(app).t("cart", { n: 3 })`, `setLanguage`, `onChange`.
`add_i18n(|options| ..)` registers the asset service too.

## Operations

Typed, versioned domain operations applied to registered targets. `OperationService` registers an
`OperationTargetHandler<T>` per `OperationTargetId`, applies `DomainOperation`s atomically in transactions (join, new
or reject nesting), returns receipts with their inverse operations and publishes `OperationEvent`s. `add_operations()`
registers it as a startup singleton; the `diagnostics` feature adds diagnostics.

## History

Undo and redo per history scope, over the operation service. `HistoryService` records the receipts of the targets
bound to a scope (`bind_target_scope`), undoes and redoes them through their inverse operations, enforces size limits,
keeps scope lifecycles and publishes `HistoryEvent`s.

`HistoryStore` persists each scope's journal (`HistoryJournalV2`):

- `add_history()`: in memory (`InMemoryHistoryStore`);
- `add_persistent_history()`: in the app's `KeyValueStore` (`KeyValueHistoryStore`, each scope's journal as JSON in the
  `rayx.history` bucket), so it persists wherever the store does: on NSH, in the device's storage;
- `add_history_with_store(store)`: any store.

Each registers the operation service too.

## Storage

Values as bytes under keys in named buckets, behind the `KeyValueStore` trait (sync methods, and async ones that
default to them), with JSON helpers on `store.bucket(name)`. `add_storage()` registers an `InMemoryKeyValueStore`
unless a store is registered; `with_key_value_store(store)` sets one.

On NSH, `rayx_app`'s `NshKeyValueStore` is registered first, so `add_storage()` keeps it: values persist in NSH's
storage (`nsh/storage`: a file in the app's data directory, the browser's `localStorage`; strings only, 5 MB in all,
shared with the app's TSX) under `rayx/<bucket>/<key>`, UTF-8 as it is and other bytes in base64. NSH's storage is
reachable only while the engine runs Rust code, so the store mirrors its entries (loaded at startup on the engine
thread): reads come from the mirror on any thread, a write where the storage is reachable goes through at once (failing
when it is full), and one elsewhere (a TSX class's synchronous call, another thread) waits until a task on the engine
thread writes it.

There is no secure storage yet: NSH has no keychain or keystore access.

## Navigation

Routes matched from URIs (`RouteMap`, `/projects/{id}`, literal segments before parameters, query and fragment,
external and not-found status), named regions with their own journals (push, replace, reset, none; back and forward),
lifecycle subscribers (`on_navigated_from`, `on_navigated_to`), route and location events, and one location for the
whole app. `add_navigation(|options| ..)` binds `NavigationOptions`, then the closure changes them; routes that do not
build fail startup.

The location is the URL region's route (`url_region`, the root region by default) behind `base_path`, with every other
region's route in its query (`/app/projects/12?panel=%2Fsettings`).

- After every navigation in any region, the service shows the location through the app's
  `NavigationLocationSynchronizer`, which `add_navigation` takes from DI (`with_navigation_location_synchronizer`): a
  new navigation as a new history entry; back, forward and refresh in place. On NSH it goes through NSH's own
  `location()` and `navigate()`; the default synchronizer does nothing.
- `apply_location` takes a location the platform moved to (its back and forward, a typed address): each region goes to
  the nearest journal entry with its route (back or forward) or to a new one, and the query regions it no longer names
  close (`close_region`, `subscribe_region_closed`). `query_regions` names the regions a location may carry before
  they exist.
- `go_to_entry(region, index)` reaches a journal entry by its index.
- Startup goes to the synchronizer's `location()` (the deep link), else to the default route.

A screen renders the current route itself, and NSH animates its own screen changes; see [Navigation](navigation.md) for
regions as NSH navigation stacks.

## Selection

The app-wide summary of the active selection area: which area (`SelectionAreaId`) publishes, and its
`SelectionStatus` (participants, selected characters, collapsed, selecting), never the selected content. Only the
active area publishes; every route change clears it, and `subscribe(..)` hears every change. `add_selection()`
registers it; it follows the navigation service, registered before or after it, and startup fails without one.

## Input map

Which action a key sequence or pointer gesture runs in a context, in three layers: the framework's and the application's
bindings (`InputMapOptions`, from code and configuration) and the user's document, kept in the app's `KeyValueStore`
and over the other two (`unbind` entries remove a lower layer's binding).

- `resolve(contexts, binding)` tries contexts innermost first, then `""`.
- `bindings_for(action)` lists an action's shortcuts.
- `apply_user_document(expected_revision, document)` validates and stores the user's document, then switches to the
  next revision; listeners hear the changes.

The document model (`InputMapDocument`, entries, keyboard and pointer bindings, modifiers) validates key sequences
itself (`valid_key_sequence`), and the service resolves bindings itself, so any UI layer can feed it.
`add_input_map(|options| ..)` registers the storage too.

## Platform

`PlatformInfoService` holds a `PlatformSnapshot` of the device and the user's settings (OS family and version, form
factor, desktop behaviour, input capabilities, viewport, scale, safe area, keyboard height, colour scheme, font scale,
reduced motion, screen reader, locale, languages, time zone and regional settings) and notifies listeners of changes.
The locale carries NSH's languages, time zone, hour cycle, first weekday, measurement system and temperature unit.

Every surface (window) has its own root provider, so a surface's service describes that surface. The UI layer keeps it
current: `rayx_app`'s `use_platform_sync()` copies NSH's environment into it. NSH does not report the display topology
(several monitors) or high contrast, and does not expose a user-writable directory (apps write through storage).

## Suspension

The app's lifecycle (not started, running, shutting down), activation (focus) and visibility (foreground, background)
as a `SuspensionSnapshot`, moved by the `SuspensionEvent`s the UI layer publishes (`rayx_app` publishes NSH's
`app_state()`), with listeners for every event and for changes of state.

- A change is a change of lifecycle, activation or visibility; an event that only moves the timestamp is not one.
- Timestamps are optional and come from the system clock where the target has one.
- NSH folds the platforms' own callbacks into one app state, so an event's reason is `AppStateChanged` (or the host's
  start and stop). Read the platform service for the platform's state.

## Application configuration

The app's own settings, `ApplicationConfiguration` from the `RayX:Application` section and code: name and description
(startup only), motion policy, programmatic scrolling, tooltip and hover-card timing, notifications and layout
direction. `ApplicationConfigurationService` follows the configuration as it reloads (an options monitor), notifies
listeners, and gives the effective motion (the system's setting from the platform service, the app's policy, a
component's request) and programmatic scroll behaviour. `add_application_configuration(|options| ..)` registers the
platform service too; invalid settings fail startup.

- The settings come from the app's bundled `appsettings.json`, read asynchronously with NSH's asset reading, so the
  app builds with `RayXAppBuilder::build_async` (`rayx_app::start_app` does).
- Unknown keys are ignored (as .NET does), so a shared `appsettings.json` may carry keys of other parts.
- Set the app's name in `appsettings.json` or in code.
- Scrollbars and cursors are left to NSH and the platform.

## Assets

The files the app's packages ship in their `assets/` directories (the only place assets live), listed in a neutral TOML
manifest per package, `assets.toml` by default (`AssetOptions::manifest`): `files = [..]` for plain files, `[[asset]]`
tables for files with a kind, density `scales`, tags, locale, theme, platform, title, description, size and custom
data. Density variants are `@1.5x`, `@2x`, `@3x` and `@4x` file names (as NSH's images load them), declared with
`scales` or listed and folded into their base; there are no density folders.

`AssetService`, a startup service, reads the manifests of `AssetOptions::packages` (the app's own first) into an
`AssetCatalog`, and:

- gives each asset's `asset:/@<package>/<path>` URL (and its variant for a scale) for NSH's elements;
- answers queries, with asset kinds and MIME types from file names;
- loads bytes, text and JSON through the platform's `AssetReader` (`rayx_app` reads with NSH's `fetch`;
  `MemoryAssetReader` holds files in memory), keeping them within a byte budget.

Manifest problems are diagnostics or startup failures, as `AssetValidationPolicy` says. NSH packages each crate's
`assets/` and serves `asset:/` URLs itself, so the same code serves desktop, mobile and web. Assets are read-only, and
a session restart reads them again (no hot reload).

## Fonts

`FontService`, a startup service after the asset service, registers the asset catalog's font files (`ttf`, `otf`,
`woff`, `woff2`) with the platform at run time through the `FontRegistrar` trait (`rayx_app` registers with NSH's
`register_font`, which reads the family name), and registers other fonts from their data while the app runs
(`register_font(data)`).

It resolves the aliases the app names fonts by: an alias's family, its load fallback (another alias or a family) when
that family did not load, and its glyph fallbacks, as one family list for NSH's `font_family` / `fontFamily`
(`font_family("body")` is `Inter, Noto Color Emoji`). Options: the aliases, load and glyph fallbacks, the default alias
and automatic registration of font assets, all validated.

## Not available yet

- **Collaboration:** shared sessions over one storage, with other participants' operations applied through the
  operation service. It will come with a standalone dXn collaboration engine.
