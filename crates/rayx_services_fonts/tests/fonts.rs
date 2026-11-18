//! The font service: registering the catalog's fonts through the platform's registrar,
//! resolving aliases with load and glyph fallbacks, run-time registration and registration.
//! The font files come from the app's `dyn AssetService` (a fake here).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use rayx_app::di::{ServiceValidationProblem, StartupInitializationContext};
use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::services::fonts::{
    FontDiagnosticCode, FontService, FontServiceError, RegisteredFontFace, ResolvedFontAlias,
};
use rayx_app::testing::FakeAssets;
use rayx_app::{AppBuildError, RayXAppBuilder, ServiceProvider, ServiceValidationError};
use rayx_services_fonts::testing::{FontRegistrar, add_fonts_with};
use rayx_services_fonts::{FontAppBuilderExt as _, FontOptions};
use serde_json::json;

/// Reads "family:<name>" data as a font of that family, and refuses anything else.
#[derive(Default)]
struct FakeRegistrar {
    registered: Mutex<Vec<String>>,
}

impl FontRegistrar for FakeRegistrar {
    fn register(&self, data: Vec<u8>) -> BoxFuture<'static, Result<String, String>> {
        let text = String::from_utf8(data).unwrap_or_default();
        let family = text.strip_prefix("family:").map(str::to_owned);
        if let Some(family) = &family {
            self.registered.lock().unwrap().push(family.clone());
        }
        async move { family.ok_or_else(|| "not a font".to_owned()) }.boxed()
    }
}

/// The app's assets: a catalog of three fonts, one a font, one garbage and one missing.
fn assets() -> Arc<dyn AssetService> {
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text(
            "app",
            "files = [\"fonts/Inter.woff2\", \"fonts/Broken.ttf\", \"fonts/Missing.otf\"]",
        )
        .unwrap();
    FakeAssets::new(catalog.build())
        .file("@app/fonts/Inter.woff2", "family:Inter")
        .file("@app/fonts/Broken.ttf", "garbage") as Arc<dyn AssetService>
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

/// A builder with RayX's font service over `registrar`, `configure` setting its options, and
/// the app's assets.
fn builder(
    configure: impl Fn(&mut FontOptions) + Send + Sync + 'static,
    registrar: Arc<dyn FontRegistrar>,
) -> RayXAppBuilder {
    let mut builder = add_fonts_with(RayXAppBuilder::new(), configure, registrar);
    builder
        .services
        .add_singleton_as::<dyn AssetService>(assets());
    builder
}

fn initialize(services: &ServiceProvider) {
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
}

fn start(registrar: Arc<FakeRegistrar>) -> ServiceProvider {
    let services = builder(|options| *options = self::options(), registrar)
        .build()
        .unwrap();
    initialize(&services);
    services
}

#[test]
fn startup_registers_the_catalogs_fonts_and_diagnoses_failures() {
    let registrar = Arc::new(FakeRegistrar::default());
    let services = start(Arc::clone(&registrar));
    let fonts = services.required_service::<dyn FontService>().unwrap();
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
    assert!(services.required_service::<dyn AssetService>().is_ok());
}

#[test]
fn aliases_resolve_with_load_and_glyph_fallbacks() {
    let services = start(Arc::new(FakeRegistrar::default()));
    let fonts = services.required_service::<dyn FontService>().unwrap();
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
    let fonts = services.required_service::<dyn FontService>().unwrap();
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
    let mut invalid = FontOptions::default();
    invalid.default_alias("missing");
    assert!(invalid.clone().normalized().is_err());
    let services = builder(
        move |configured| *configured = invalid.clone(),
        Arc::new(FakeRegistrar::default()),
    )
    .build()
    .unwrap();
    assert!(services.validate_options_on_start().is_err());
    assert!(
        block_on(
            services
                .startup_registry()
                .initialize(StartupInitializationContext::new(services.clone())),
        )
        .is_err()
    );

    // Outside an NSH app's start, NSH's registrar has no engine to register with.
    let mut builder = RayXAppBuilder::new().add_fonts(|options| {
        options.auto_register_assets(false);
    });
    builder
        .services
        .add_singleton_as::<dyn AssetService>(assets());
    let services = builder.build().unwrap();
    initialize(&services);
    let fonts = services.required_service::<dyn FontService>().unwrap();
    match block_on(fonts.register_font(b"family:Inter".to_vec())) {
        Err(FontServiceError::Registration(reason)) => {
            assert!(reason.contains("not started on an NSH surface"), "{reason}");
        }
        other => panic!("registered without an NSH surface: {other:?}"),
    }
    assert!(fonts.registered_fonts().is_empty());
}

#[test]
fn aliases_bind_from_configuration() {
    let mut builder = builder(|_| {}, Arc::new(FakeRegistrar::default()));
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Fonts": {
            "autoRegisterAssets": false,
            "defaultAlias": "body",
            "aliases": [ { "alias": "body", "family": "Inter" }, { "alias": "Mono" } ]
        } } }
    }));
    let services = builder.build().unwrap();
    initialize(&services);
    let options = services.options::<FontOptions>().unwrap();
    assert!(!options.auto_register_assets);
    let fonts = services.required_service::<dyn FontService>().unwrap();
    // Nothing registered: auto registration is off.
    assert!(fonts.registered_fonts().is_empty());
    assert_eq!(fonts.default_font().unwrap().resolved_family, "Inter");
    assert_eq!(fonts.resolve_alias("Mono").unwrap().resolved_family, "Mono");
}

#[test]
fn building_without_an_asset_service_fails_with_its_hint() {
    let builder = add_fonts_with(
        RayXAppBuilder::new(),
        |_| {},
        Arc::new(FakeRegistrar::default()),
    );
    let Err(AppBuildError::Services(ServiceValidationError { problems })) = builder.build() else {
        panic!("built without an asset service");
    };
    assert!(
        problems.iter().any(|problem| matches!(
            problem,
            ServiceValidationProblem::Missing { needed, hint, .. }
                if needed.contains("AssetService") && hint.contains("add_assets(..)")
        )),
        "{problems:?}"
    );
}
