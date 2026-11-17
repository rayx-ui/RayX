# Fonts

Fonts registered at run time, and the aliases the app names them by.

## Register

```rust
builder.add_fonts(|options| {
    options
        .default_alias("body")
        .alias("body", |alias| {
            alias.family("Inter").glyph_fallback("emoji");
        })
        .alias("emoji", |alias| {
            alias.family("Noto Color Emoji");
        });
})
```

RayX's implementation is `rayx_services_fonts`'s (`builder.addFonts(..)` in TS, once its set is registered),
registered as the app's `dyn FontService`; the app's start requires one, RayX's or the app's own.

`add_fonts` requires an [asset service](assets.md) (`add_assets(..)`, or the app's own `dyn AssetService`) and
registers none: building without one fails naming it. RayX's font service initializes after the asset service.

## Configure

`FontOptions` bind from `RayX:Services:Fonts`, then the closure changes them: the aliases with their load and glyph
fallbacks, the default alias and automatic registration of font assets, all validated.

```json
{ "RayX": { "Services": { "Fonts": {
  "defaultAlias": "body",
  "aliases": [{ "alias": "body", "family": "Inter" }]
} } } }
```

## Use

```rust
let fonts = services.required_service::<dyn FontService>()?;
let family = fonts.font_family("body"); // "Inter, Noto Color Emoji"
let name = fonts.register_font(bytes).await?;
```

- At startup the service takes the asset catalog's font files (`ttf`, `otf`, `woff`, `woff2`) from the app's
  `dyn AssetService` (which it requires) and reads their families (NSH registered the fonts the app ships before the
  first frame); `register_font(data)` registers others with NSH while the app runs, from any thread.
- An alias resolves to its family, its load fallback (another alias or a family) when that family did not load, and
  its glyph fallbacks, as one family list for NSH's `font_family` / `fontFamily`.

## On NSH

NSH registers every font its UI sets' crates ship in `assets/` before the first frame, so for the catalog's font assets
`rayx_app_nsh` only reads the family name (NSH's `font_family`); a font registered while the app runs (`register_font`)
goes to NSH's `register_font`. Both read WOFF and WOFF2 fonts too.

## TSX

`FontService`, resolved from the provider `start()` gives:

```tsx
const fonts = services.getRequiredService(FontService);
<Text style={{ fontFamily: fonts.family("body") }}>Hello</Text>
```
