# Assets

The files the app's packages ship in their `assets/` directories, as the platform lists them or a manifest does, served
as `asset:/` URLs: the app's own, and those of every UI set the app serves (a library's, such as RayX's components),
without the app naming their packages.

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
registration of it: a startup service (keyed `dyn AssetService`) that reads the packages' manifests and the files the
app's UI sets ship into the catalog when the app starts. The app's start requires a `dyn AssetService`, RayX's or the
app's own; [fonts](fonts.md) and [i18n](i18n.md) require one too.

`add_asset_files(files)` (NSH's `assets!()`) and `add_bundled_assets(uris)` (any list of `asset:/@<package>/<path>`
URLs) add files to the catalog, so their package needs no manifest listing them. Packages they add come after the
asset options' packages, in the order their files are added. TS has no counterpart.

## The sets' files

Every file a UI set the app serves ships (`registry.assets(assets!())` in the set: the app's crate's, and a library's
set its set serves, such as RayX's `rayx/components`) is in the catalog with nothing named: on NSH the catalog reads
NSH's `served_asset_files()` on the engine thread (through the surface's engine runner) when it loads. Their packages
come after the asset options' packages and the ones `add_asset_files` and `add_bundled_assets` add, so the app's own
files win a path without a package; a served package's manifest (`assets.toml`) is read only when the package ships
one, for its metadata (tags, scales, …). A library's icons are then `assets.uri("@acme-ui/icons/star.svg")`, or
`assets.query(..)` by their tags, with no `options.package("acme-ui")`.

Two ways to reach such a file:

- **Checked at build time**, in TSX: `import star from "asset:/@acme-ui/icons/star.svg"` imports a file a served set
  ships by its URL (an image's `ImageAsset`, a font's family, any other file's URL; `?url` for the URL of any). The
  bundler fails the build for a file no set ships, and `.nsh/types/nsh/set-assets.d.ts` types each one. Use it when the
  code names the file.
- **Looked up at run time**, through this service: the catalog, its metadata and queries, `uri(path)`, `load_bytes`.
  Use it when the file is chosen while the app runs (by a tag, a locale, a name from data).

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

A package whose files are added (`add_asset_files`, `add_bundled_assets`) or shipped by a served set needs a manifest
only for metadata, and its manifest is read only when it ships one; a package whose files are not listed so lists them
in its manifest. The manifest is TOML,
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
through the surface's engine runner, so the same code reads them on every platform and from any thread. The list of
the files the served sets ship is the engine's (`served_asset_files()`, in its context on its thread), read the same
way (a test over `rayx_services_assets::testing::MemoryAssetReader` lists its own with `serve`). Assets are
read-only, and a session restart reads them again (no hot reload).

## TSX

`AssetService`, resolved from the provider `start()` gives, wraps whichever `dyn AssetService` is registered:

- `url(path)`: the asset's `asset:/` URL, for an `<Image source>` or a `fetch` (it throws for an unknown asset);
- `urlForScale(path, scale)`: its variant's for a pixel ratio.

```tsx
// With the provider `start()` gives:
function logoUrl(services: ServiceProvider) {
  const assets = services.getRequiredService(AssetService);
  return assets.url("images/logo.png"); // asset:/@my-app/images/logo.png
}
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
declare function readFile(path: string): Promise<Uint8Array>; // the app's own file access
declare function readTextFile(path: string): Promise<string>;

class MyAssets implements AssetServiceContract {
  catalog = new ContractState<AssetCatalog | null>(null);
  isInitialized() { return this.catalog.value !== null; }
  async initialize() { await this.loadCatalog(); }
  async loadCatalog() { this.catalog.publish({ packages: ["app"], entries: { /* ... */ }, diagnostics: [] }); }
  async loadBytes(path: string) { return readFile(path); }
  async loadText(path: string) { return readTextFile(path); }
  clearCache() {}
}

new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addStartupSingletonAs(AssetService, () => new MyAssets())
  .addFonts()
  .start()
  .then((services) => { /* RayX's AssetService class wraps MyAssets */ });
```

For tests, `rayx_app::testing::FakeAssets` serves a catalog and files from memory (`file`) and records what it is
asked to load.
