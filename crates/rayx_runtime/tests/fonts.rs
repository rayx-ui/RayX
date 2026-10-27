//! The font service: registering the catalog's fonts through the platform's registrar,
//! resolving aliases with load and glyph fallbacks, run-time registration and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::LocalBoxFuture;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::assets::{AssetReader, AssetService, MemoryAssetReader};
use rayx_runtime::services::fonts::*;
use serde_json::json;

/// Reads "family:<name>" data as a font of that family, and refuses anything else.
#[derive(Default)]
struct FakeRegistrar {
    registered: Mutex<Vec<String>>,
}

impl FontRegistrar for FakeRegistrar {
    fn register(&self, data: Vec<u8>) -> LocalBoxFuture<'static, Result<String, String>> {
        let text = String::from_utf8(data).unwrap_or_default();
        let family = text.strip_prefix("family:").map(str::to_owned);
        if let Some(family) = &family {
            self.registered.lock().unwrap().push(family.clone());
        }
        async move { family.ok_or_else(|| "not a font".to_owned()) }.boxed_local()
    }
}

fn reader() -> MemoryAssetReader {
    MemoryAssetReader::new()
        .with(
            "asset:/@app/assets.toml",
            "files = [\"fonts/Inter.woff2\", \"fonts/Broken.ttf\", \"fonts/Missing.otf\"]"
                .as_bytes(),
        )
        .with("asset:/@app/fonts/Inter.woff2", "family:Inter".as_bytes())
        .with("asset:/@app/fonts/Broken.ttf", "garbage".as_bytes())
}

fn options() -> FontOptions {
    let mut options = FontOptions::default();
    options.alias("emoji", |alias| {
        alias
            .family("Noto Color Emoji")
            .load_fallback_family("Apple Color Emoji");
    });
    options.alias("body", |alias| {
        alias.family("Inter").glyph_fallback("emoji");
    });
    options.alias("display", |alias| {
        alias.family("Display, Pro").load_fallback_alias("body");
    });
    options.default_alias("body");
    options
}

fn start(registrar: Arc<FakeRegistrar>) -> rayx_core::ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_fonts(|options| *options = self::options());
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": ["app"] } } }
    }));
    builder
        .services
        .add_singleton_as::<dyn AssetReader>(Arc::new(reader()) as Arc<dyn AssetReader>);
    builder
        .services
        .add_singleton_as::<dyn FontRegistrar>(registrar as Arc<dyn FontRegistrar>);
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services
}

#[test]
fn startup_registers_the_catalogs_fonts_and_diagnoses_failures() {
    let registrar = Arc::new(FakeRegistrar::default());
    let services = start(Arc::clone(&registrar));
    let fonts = services.required_service::<FontService>().unwrap();
    assert_eq!(*registrar.registered.lock().unwrap(), ["Inter"]);
    assert_eq!(
        fonts.registered_fonts(),
        [RegisteredFontFace {
            logical_path: Some("@app/fonts/Inter.woff2".into()),
            family: "Inter".into(),
        }]
    );
    let codes: Vec<_> = fonts
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert!(codes.contains(&FontDiagnosticCode::RegistrationFailed));
    assert!(codes.contains(&FontDiagnosticCode::MissingAsset));
    assert!(services.required_service::<AssetService>().is_ok());
}

#[test]
fn aliases_resolve_with_load_and_glyph_fallbacks() {
    let services = start(Arc::new(FakeRegistrar::default()));
    let fonts = services.required_service::<FontService>().unwrap();
    // Inter loaded; the emoji family did not, but it has no way of knowing a system font:
    // its load fallback stands in.
    let body = fonts.default_font().unwrap();
    assert_eq!(body.resolved_family, "Inter");
    assert_eq!(body.glyph_fallback_families, ["Apple Color Emoji"]);
    assert_eq!(body.family_list(), "Inter, Apple Color Emoji");
    let display = fonts.resolve_alias("display").unwrap();
    assert_eq!(display.requested_family, "Display, Pro");
    assert_eq!(display.resolved_family, "Inter");
    assert_eq!(fonts.font_family("body"), "Inter, Apple Color Emoji");
    assert_eq!(fonts.font_family("Courier"), "Courier");
    assert!(
        fonts
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == FontDiagnosticCode::LoadFallbackSubstitution)
    );
    let quoted = ResolvedFontAlias {
        alias: "a".into(),
        requested_family: "Display, Pro".into(),
        resolved_family: "Display, Pro".into(),
        glyph_fallback_families: vec![],
    };
    assert_eq!(quoted.family_list(), "\"Display, Pro\"");
}

#[test]
fn fonts_registered_at_run_time_resolve_their_aliases_again() {
    let registrar = Arc::new(FakeRegistrar::default());
    let services = start(Arc::clone(&registrar));
    let fonts = services.required_service::<FontService>().unwrap();
    assert_eq!(
        fonts.resolve_alias("emoji").unwrap().resolved_family,
        "Apple Color Emoji"
    );
    let family = block_on(fonts.register_font(b"family:Noto Color Emoji".to_vec())).unwrap();
    assert_eq!(family, "Noto Color Emoji");
    assert_eq!(
        fonts.resolve_alias("emoji").unwrap().resolved_family,
        "Noto Color Emoji"
    );
    assert!(matches!(
        block_on(fonts.register_font(b"nope".to_vec())),
        Err(FontServiceError::Registration(_))
    ));
}

#[test]
fn invalid_aliases_fail_and_a_missing_registrar_is_an_error() {
    let mut options = FontOptions::default();
    options.default_alias("missing");
    assert!(FontService::new(options.clone(), None).is_err());
    let services = RayXAppBuilder::new()
        .add_fonts(move |configured| *configured = options.clone())
        .build()
        .unwrap();
    assert!(services.validate_options_on_start().is_err());

    let service = FontService::new(FontOptions::default(), None).unwrap();
    assert_eq!(
        block_on(service.register_font(Vec::new())),
        Err(FontServiceError::NoRegistrar)
    );
}

#[test]
fn aliases_bind_from_configuration() {
    let mut builder = RayXAppBuilder::new().add_fonts(|_| {});
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Fonts": {
            "autoRegisterAssets": false,
            "defaultAlias": "body",
            "aliases": [ { "alias": "body", "family": "Inter" }, { "alias": "Mono" } ]
        } } }
    }));
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    let fonts = services.required_service::<FontService>().unwrap();
    assert!(!fonts.options().auto_register_assets);
    assert_eq!(fonts.default_font().unwrap().resolved_family, "Inter");
    assert_eq!(fonts.resolve_alias("Mono").unwrap().resolved_family, "Mono");
}
