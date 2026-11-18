# Assets

The files the app's packages ship in their `assets/` directories, as the platform lists them or a manifest does, served
as `asset:/` URLs.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::assets::AssetService` (`rayx::app::services::assets`) |
| Crate | `rayx_services_assets` (`rayx::services::assets`) |
| Feature | `assets` (default) |
| Rust | `add_assets(..)`, `add_asset_files(files)`, `add_bundled_assets(uris)` (`AssetAppBuilderExt`) |
| TS | `addAssets(..)`, and the class `AssetService` |
| Requires | nothing |

```rust
use rayx::prelude::*; // AssetAppBuilderExt, with the `assets` feature

let builder = RayXAppBuilder::new()
    .add_assets(|options| {
        options.package("my-app").package("shared"); // packages whose manifests are read
    })
    .add_asset_files(assets!()); // the files this crate ships, as NSH lists them for its UI set
// TS: new RayXAppBuilder().addAssets((options) => { options.package("my-app"); })
```

`add_assets(configure)` registers RayX's implementation as the app's `dyn AssetService`, replacing any earlier
registration of it: a startup service (keyed `dyn AssetService`) that reads the packages' manifests into the catalog
when the app starts. The app's start requires a `dyn AssetService`, RayX's or the app's own; [fonts](fonts.md) and
[i18n](i18n.md) require one too.

`add_asset_files(files)` (NSH's `assets!()`) and `add_bundled_assets(uris)` (any list of `asset:/@<package>/<path>`
URLs) add files to the catalog, so their package needs no manifest listing them. Packages they add come after the
asset options' packages, in the order their files are added. TS has no counterpart.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `assets` feature on, or
`rayx_services_assets::register(registry)` by hand.

## Configure

`AssetOptions` bind from `RayX:Services:Assets` (`ASSET_OPTIONS_SECTION`), then the closure changes them and they are
validated at the start:

- `packages`: the packages whose manifests are read, the app's own first; `package(name)` adds one;
- `manifest`: the manifest's file name, `assets.toml`;
- `validation`: an `AssetValidationPolicy`, `WarnOnly` (problems are diagnostics), `Strict` (problems fail the start)
  or `Disabled`;
- `byte_cache_budget`: how many bytes of read files stay in memory, 16 MiB (`DEFAULT_ASSET_BYTE_CACHE_BUDGET`).

```json
{ "RayX": { "Services": { "Assets": { "packages": ["my-app"], "byteCacheBudget": 1048576 } } } }
```

TS's options object has `package(name)` only.

## The manifest

A package whose files are added (`add_asset_files`, `add_bundled_assets`) needs a manifest only for metadata, and its
manifest is read only when it ships one; a package whose files are not added lists them in its manifest. The manifest is TOML,
`assets.toml` by default:

```toml
files = ["icons/close.svg", "locales/en.json"]

[[asset]]
path = "images/logo.png"
scales = [2, 3]
tags = ["brand"]
```

- `files = [..]` lists plain files; `[[asset]]` tables add a kind, density `scales`, tags, locale, theme, platform,
  title, description, size and custom data.
- Density variants are `@1.5x`, `@2x`, `@3x` and `@4x` file names (as NSH's images load them), declared with `scales`
  or listed and folded into their base. There are no density folders.
- A listed file the manifest does not mention takes its kind from its name; it is in the catalog with no metadata.
- Manifest problems are diagnostics or startup failures, as `AssetValidationPolicy` says.

## The contract

`AssetService` (`AssetServiceContract` in TS) is called from any thread.

Published state:

- `catalog()` (`catalog`): a `StateCell<Option<AssetCatalog>>`, the catalog once loaded (`None` until then), with
  what reading the manifests found wrong.

Commands (`async`, `Result<_, AssetError>`; promises in TS):

- `load_catalog()` (`loadCatalog`): loads the catalog from the manifests and publishes it (the app's start does).
- `load_bytes(path)` (`loadBytes`): the bytes of the asset at `path`, `Vec<u8>`.
- `load_text(path)` (`loadText`): its UTF-8 text.

Posted commands:

- `clear_cache()` (`clearCache`): forgets the bytes read so far.

Paths are logical paths (`@<package>/<path>`), or paths in the first package that has them; the commands take an
owned `String`.

Rust helpers, on `dyn AssetService`, synchronous except `load_json`:

- `loaded_catalog()`: the catalog, or `AssetError::NotLoaded`;
- `descriptor(path)` and `query(&AssetQuery)`: an asset's `AssetDescriptor` (package, path, kind, MIME type, scales,
  metadata), and the assets a query matches;
- `uri(path)`: its `asset:/` URL; `uri_for_scale(path, scale)`: its variant's for a scale;
- `diagnostics()`: what reading the manifests found wrong;
- `load_json::<T>(path)`: the asset decoded from JSON.

## Use

```rust
use rayx::app::services::assets::AssetService;

let assets = services.required_service::<dyn AssetService>()?;
let url = assets.uri("images/logo.png")?;                // asset:/@my-app/images/logo.png
let sharp = assets.uri_for_scale("images/logo.png", 2.0)?;
let text = assets.load_text("notes/welcome.md".into()).await?;
let settings: Settings = assets.load_json("data/settings.json").await?;
```

Kinds and MIME types come from file names. RayX's implementation keeps what it reads within the byte budget.

## On NSH

NSH packages each crate's `assets/` and serves `asset:/` URLs itself (from the app's package on a device, the dev
server during `nsh run`, the page's assets on the web), and RayX's implementation reads them with NSH's `fetch`
through the surface's engine runner, so the same code reads them on every platform and from any thread. Assets are
read-only, and a session restart reads them again (no hot reload).

## TSX

`AssetService`, resolved from the provider `start()` gives, wraps whichever `dyn AssetService` is registered:

- `url(path)`: the asset's `asset:/` URL, for an `<Image source>` or a `fetch` (it throws for an unknown asset);
- `urlForScale(path, scale)`: its variant's for a pixel ratio.

```tsx
const assets = services.getRequiredService(AssetService);
const logo = assets.url("images/logo.png"); // asset:/@my-app/images/logo.png
```

## Replace it

An app may keep its files elsewhere (embedded, downloaded) by registering its own `dyn AssetService` in place of
`add_assets(..)`. RayX's [fonts](fonts.md) and [i18n](i18n.md) then read the app's own assets, the host starts with
it, and the class `AssetService` wraps it.

```rust
builder.add_singleton_as::<dyn AssetService>(Arc::new(MyAssets::new()) as Arc<dyn AssetService>)
```

One that loads its catalog at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_assets(..)` does: the trait and a startup service registered as it, which RayX's fonts wait for
(`builder.add_startup_singleton_for::<dyn AssetService, _>(Arc::new(MyAssets::new()))`, `MyAssets` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_assets(..)`, the app's own replaces it completely: RayX's never starts.

In TS, a class implementing `AssetServiceContract`, registered under `AssetService`: `catalog` a `ContractState`
(`null` until loaded, then the JSON of an `AssetCatalog`: `{ packages, entries, diagnostics }`, `entries` keyed by
logical path), the commands `async` methods, `clearCache()` a plain one. `loadBytes` may answer a `Uint8Array` (or
another typed array or `ArrayBuffer`) or a `number[]`.

```tsx
class MyAssets {
  catalog = new ContractState(null);
  isInitialized() { return this.catalog.value !== null; }
  async initialize() { await this.loadCatalog(); }
  async loadCatalog() { this.catalog.publish({ packages: ["app"], entries: { /* ... */ }, diagnostics: [] }); }
  async loadBytes(path) { /* the file's bytes */ }
  async loadText(path) { /* the file's text */ }
  clearCache() {}
}

const services = await new RayXAppBuilder()
  .addStartupSingletonAs(AssetService, () => new MyAssets())
  .addFonts()
  .start();
```

For tests, `rayx_app::testing::FakeAssets` serves a catalog and files from memory (`file`) and records what it is
asked to load.
