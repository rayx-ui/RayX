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

`add_fonts` registers the [asset service](assets.md) too. `FontService` is a startup service that initializes after it.

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
let fonts = services.required_service::<FontService>()?;
let family = fonts.font_family("body"); // "Inter, Noto Color Emoji"
let name = fonts.register_font(bytes).await?;
```

- At startup the service registers the asset catalog's font files (`ttf`, `otf`, `woff`, `woff2`) with the platform
  through the `FontRegistrar` trait; `register_font(data)` adds others while the app runs.
- An alias resolves to its family, its load fallback (another alias or a family) when that family did not load, and
  its glyph fallbacks, as one family list for NSH's `font_family` / `fontFamily`.

## On NSH

`rayx_app` registers fonts with NSH's `register_font`, which reads the family name, so WOFF and WOFF2 fonts work too.

## TSX

```tsx
const fonts = Fonts.of(app);
<Text style={{ fontFamily: fonts.family("body") }}>Hello</Text>
```
