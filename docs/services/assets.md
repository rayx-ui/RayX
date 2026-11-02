# Assets

The files the app's packages ship in their `assets/` directories, listed in a manifest and served as `asset:/` URLs.

## Register

```rust
builder.add_assets(|options| {
    options.package("shared");
})
```

## Configure

Options bind from `RayX:Services:Assets`, then the closure changes them: the packages whose manifests are read (the
app's own first), the manifest's file name (`assets.toml`), the validation policy and the byte cache's budget.

```json
{ "RayX": { "Services": { "Assets": { "packages": ["my-app"], "byteCacheBudget": 1048576 } } } }
```

## The manifest

Each package lists its assets in a TOML manifest, `assets.toml` by default:

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
- Manifest problems are diagnostics or startup failures, as `AssetValidationPolicy` says.

## Use

```rust
let assets = services.required_service::<AssetService>()?;
let url = assets.uri("images/logo.png")?;                // asset:/@my-app/images/logo.png
let sharp = assets.uri_for_scale("images/logo.png", 2.0)?;
let text = assets.load_text("notes/welcome.md").await?;
```

- `AssetService`, a startup service, reads the manifests into an `AssetCatalog`; `descriptor` and `query` answer
  questions about it, with kinds and MIME types from file names.
- `load_bytes`, `load_text` and `load_json` read through the platform's `AssetReader` and keep what they read within
  the byte budget. `MemoryAssetReader` holds files in memory, for tests.

## On NSH

NSH packages each crate's `assets/` and serves `asset:/` URLs itself, and `rayx_app` reads them with NSH's `fetch`, so
the same code serves desktop, mobile and web. Assets are read-only, and a session restart reads them again (no hot
reload).

## TSX

```tsx
const assets = Assets.of(app);
const logo = assets.url("images/logo.png"); // asset:/@my-app/images/logo.png
```
