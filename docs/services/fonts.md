# Fonts

Fonts registered at run time, and the aliases the app names them by.

## Register

| | |
| --- | --- |
| Trait | `rayx_app::services::fonts::FontService` (`rayx::app::services::fonts`) |
| Crate | `rayx_services_fonts` (`rayx::services::fonts`) |
| Feature | `fonts` (default) |
| Rust | `add_fonts(..)` (`FontAppBuilderExt`) |
| TS | `addFonts(..)`, and the class `FontService` |
| Requires | `dyn AssetService` (`add_assets(..)`) |

```rust
use rayx::prelude::*; // FontAppBuilderExt, with the `fonts` feature

let builder = RayXAppBuilder::new()
    .add_assets(|_| {})
    .add_fonts(|options| {
        options
            .default_alias("body")
            .alias("body", |alias| {
                alias.family("Inter").glyph_fallback("emoji");
            })
            .alias("emoji", |alias| {
                alias.family("Noto Color Emoji");
            });
    });
```

```tsx
new RayXAppBuilder()
  .addAssets()
  .addFonts((options) => {
    options.alias("body", (alias) => { alias.family("Inter").glyphFallback("emoji"); }).defaultAlias("body");
    options.alias("emoji", (alias) => { alias.family("Noto Color Emoji"); });
  });
```

`add_fonts(configure)` registers RayX's implementation as the app's `dyn FontService`, replacing any earlier
registration of it: a startup service (keyed `dyn FontService`) that registers the asset catalog's fonts when the app
starts, after the `dyn AssetService`. The app's start requires a `dyn FontService`, RayX's or the app's own.

It requires an [asset service](assets.md) (`add_assets(..)`, or the app's own `dyn AssetService`) and registers none:
building without one fails naming it.

The TS side is there once the crate's set is registered: `#[rayx::native_module]` with the `fonts` feature on, or
`rayx_services_fonts::register(registry)` by hand.

## Configure

`FontOptions` bind from `RayX:Services:Fonts` (`FONT_CONFIG_PATH`), then the closure changes them; invalid aliases
fail the start:

- `aliases`: each a `FontAlias` (`alias(name, configure)`): its `family` (the alias itself when empty), a load
  fallback (`load_fallback_alias` or `load_fallback_family`) standing in when that family did not load, and its glyph
  fallbacks (`glyph_fallback`, aliases, in order);
- `default_alias`: the alias the app's text uses unless it names another;
- `auto_register_assets`: whether the catalog's font assets are registered at the start (yes by default).

```json
{ "RayX": { "Services": { "Fonts": {
  "defaultAlias": "body",
  "aliases": [{ "alias": "body", "family": "Inter" }]
} } } }
```

TS's options objects have the same methods in camelCase (`defaultAlias`, `autoRegisterAssets`, `alias(name,
configure)`; on an alias `family`, `loadFallbackAlias`, `loadFallbackFamily`, `glyphFallback`).

## The contract

`FontService` (`FontServiceContract` in TS) is called from any thread.

Published state:

- `fonts()` (`fonts`): a `StateCell<FontState>`: the fonts registered (`faces`: logical path and family), every alias
  resolved against them (`aliases`: requested and resolved family, glyph fallback families), the default alias and
  the diagnostics.

Commands (`async`, `Result<_, FontServiceError>`; promises in TS):

- `register_font(data)` (`registerFont`): registers a font from its data (TrueType, OpenType, WOFF, WOFF2) while the
  app runs, resolving the aliases again; answers its family name.
- `register_font_assets()` (`registerFontAssets`): registers the asset catalog's font files, then resolves the aliases
  (RayX's implementation does at the start unless `auto_register_assets` is off); a font that does not load or
  register is a diagnostic.

Rust helpers, on `dyn FontService`, all synchronous (`font_family`, `resolve_alias` and `default_font` are on
`FontState` too):

- `font_family(alias_or_family)`: an alias's family list (`"Inter, Noto Color Emoji"`), or the name itself;
- `resolve_alias(alias)`, `default_font()`, `resolved_aliases()`: `ResolvedFontAlias`es (`family_list()` gives the
  list);
- `registered_fonts()` and `diagnostics()`.

## Use

```rust
use rayx::app::services::fonts::FontService;

let fonts = services.required_service::<dyn FontService>()?;
let family = fonts.font_family("body"); // "Inter, Noto Color Emoji"
let name = fonts.register_font(bytes).await?; // bytes: Vec<u8>
```

- At the start RayX's implementation takes the catalog's font files (`ttf`, `otf`, `woff`, `woff2`) from the app's
  `dyn AssetService` and reads their families; `register_font(data)` registers others while the app runs, from any
  thread.
- An alias resolves to its family, its load fallback (another alias or a family) when that family did not load, and
  its glyph fallbacks, as one family list for NSH's `font_family` / `fontFamily`.

## On NSH

NSH registers every font its UI sets' crates ship in `assets/` before the first frame, so for the catalog's font assets
RayX's implementation only reads the family name (NSH's `font_family`); a font registered while the app runs
(`register_font`) goes to NSH's `register_font` through the surface's engine runner. Both read WOFF and WOFF2 fonts
too.

## TSX

`FontService`, resolved from the provider `start()` gives, wraps whichever `dyn FontService` is registered:

- `family(alias)`: the `fontFamily` for an alias (its family and fallbacks), or the name itself as a family;
- `aliases()`: every alias, resolved (`{ alias, requested_family, resolved_family, glyph_fallback_families }`).

```tsx
const fonts = services.getRequiredService(FontService);
<Text style={{ fontFamily: fonts.family("body") }}>Hello</Text>
```

## Replace it

An app may register fonts itself by registering its own `dyn FontService` in place of `add_fonts(..)`; it needs no
asset service then, unless it reads one. The host starts with it, and the class `FontService` wraps it.

```rust
builder.add_singleton_as::<dyn FontService>(Arc::new(MyFonts::new()) as Arc<dyn FontService>)
```

One that must initialize at the start registers itself with `add_startup_singleton_for` in place of `add_singleton_as`,
as `add_fonts(..)` does: the trait and a startup service registered as it
(`builder.add_startup_singleton_for::<dyn FontService, _>(Arc::new(MyFonts::new()))`, `MyFonts` implementing
`AsyncInitializable` and `StartupInitializable`; [Startup](../startup.md#implementing-a-trait)). Registered
after `add_fonts(..)`, the app's own replaces it completely: RayX's never starts.

To keep RayX's fonts over the app's own files, register the app's own `dyn AssetService` instead ([assets](assets.md)):
RayX's font service reads the font files through it.

In TS, a class implementing `FontServiceContract`, registered under `FontService`: `fonts` a `ContractState` holding
a whole `FontState` (as JSON), `registerFont(data)` (`data` a `number[]`) and `registerFontAssets()` `async` methods.

```tsx
class MyFonts {
  fonts = new ContractState({ faces: [], aliases: [], default_alias: null, diagnostics: [] });
  isInitialized() { return this.fonts.value.faces.length > 0; }
  async initialize() { await this.registerFontAssets(); }
  async registerFont(data) { /* register it; answer its family */ }
  async registerFontAssets() {
    this.fonts.publish({ ...this.fonts.value, faces: [{ logical_path: "@app/fonts/Inter.woff2", family: "Inter" }] });
  }
}

new RayXAppBuilder()
  .addPlatform()
  .addSuspension()
  .addAssets()
  .addStartupSingletonAs(FontService, () => new MyFonts())
  .start()
  .then((services) => { /* RayX's FontService class wraps MyFonts */ });
```

For tests, `rayx_app::testing::FakeFonts` publishes the state the test gives it and records the fonts registered.
