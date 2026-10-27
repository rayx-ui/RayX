//! The asset service: manifests, the catalog with density variants and queries, loading
//! through a reader with a byte budget, validation policies and registration.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::assets::*;
use serde_json::json;

const APP_MANIFEST: &str = r#"
files = ["fonts/Inter.woff2", "locales/en.json", "themes/dark.json"]

[[asset]]
path = "images/logo.png"
scales = [2, 3]
tags = ["brand"]
theme = "dark"
title = "Logo"
custom = { order = 1 }

[[asset]]
path = "images/hero.png"

[[asset]]
path = "images/hero@2x.png"

[[asset]]
path = "images/badge@3x.png"
"#;

const SHARED_MANIFEST: &str = r#"
files = ["locales/en.json", "locales/de.json", "sounds/goal.mp3"]
"#;

fn catalog() -> AssetCatalog {
    let mut builder = AssetCatalog::builder();
    builder.add_manifest_text("app", APP_MANIFEST).unwrap();
    builder
        .add_manifest_text("shared", SHARED_MANIFEST)
        .unwrap();
    builder.build()
}

#[test]
fn manifests_list_files_with_kinds_and_metadata() {
    let catalog = catalog();
    assert_eq!(catalog.packages(), ["app", "shared"]);
    let logo = catalog.get("@app/images/logo.png").unwrap();
    assert_eq!(logo.kind, AssetKind::Image);
    assert_eq!(logo.mime_type.as_deref(), Some("image/png"));
    assert_eq!(logo.tags, ["brand"]);
    assert_eq!(logo.title.as_deref(), Some("Logo"));
    assert_eq!(logo.custom, json!({ "order": 1 }));
    assert_eq!(
        catalog.get("@app/fonts/Inter.woff2").unwrap().kind,
        AssetKind::Font
    );
    assert_eq!(
        catalog.get("@app/locales/en.json").unwrap().kind,
        AssetKind::Locale
    );
    assert_eq!(
        catalog.get("@app/themes/dark.json").unwrap().kind,
        AssetKind::Theme
    );
    assert_eq!(
        catalog.get("@shared/sounds/goal.mp3").unwrap().kind,
        AssetKind::Audio
    );
    assert!(
        catalog.diagnostics().is_empty(),
        "{:?}",
        catalog.diagnostics()
    );
}

#[test]
fn density_variants_fold_into_their_base() {
    let catalog = catalog();
    let logo = catalog.get("@app/images/logo.png").unwrap();
    assert_eq!(logo.scales, [1.0, 2.0, 3.0]);
    assert_eq!(logo.uri(), "asset:/@app/images/logo.png");
    assert_eq!(logo.uri_for_scale(1.0), "asset:/@app/images/logo.png");
    assert_eq!(logo.uri_for_scale(2.625), "asset:/@app/images/logo@3x.png");
    assert_eq!(logo.uri_for_scale(4.0), "asset:/@app/images/logo@3x.png");
    let hero = catalog.get("@app/images/hero.png").unwrap();
    assert_eq!(hero.scales, [1.0, 2.0]);
    assert!(catalog.get("@app/images/hero@2x.png").is_none());
    // A file named with a density suffix and no base is that file alone.
    let badge = catalog.get("@app/images/badge@3x.png").unwrap();
    assert!(badge.scales.is_empty());
    assert_eq!(badge.uri_for_scale(2.0), "asset:/@app/images/badge@3x.png");
}

#[test]
fn bare_paths_resolve_in_package_order() {
    let catalog = catalog();
    assert_eq!(catalog.get("locales/en.json").unwrap().package, "app");
    assert_eq!(catalog.get("./locales/de.json").unwrap().package, "shared");
    assert!(catalog.get("locales/fr.json").is_none());
}

#[test]
fn queries_combine_their_criteria() {
    let catalog = catalog();
    let locales = catalog.query(&AssetQuery::kind(AssetKind::Locale));
    assert_eq!(locales.len(), 3);
    let shared_locales = catalog.query(&AssetQuery {
        packages: ["shared".to_owned()].into(),
        prefix: Some("locales/".to_owned()),
        ..AssetQuery::default()
    });
    assert_eq!(shared_locales.len(), 2);
    let dark_brand = catalog.query(&AssetQuery {
        tags: ["brand".to_owned()].into(),
        theme: Some("dark".to_owned()),
        extensions: ["png".to_owned()].into(),
        ..AssetQuery::default()
    });
    assert_eq!(dark_brand.len(), 1);
    assert_eq!(dark_brand[0].logical_path(), "@app/images/logo.png");
}

#[test]
fn manifest_problems_are_diagnostics() {
    let mut builder = AssetCatalog::builder();
    builder
        .add_manifest_text(
            "app",
            r#"
files = ["a.png", "a.png", "../escape.png", "/absolute.png", "data.xyz"]

[[asset]]
path = "b.png"
scales = [5]

[[asset]]
path = "c.svg"
scales = [2]
"#,
        )
        .unwrap();
    let catalog = builder.build();
    let messages: Vec<_> = catalog
        .diagnostics()
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.severity,
                diagnostic.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    use AssetDiagnosticSeverity::{Error, Warning};
    assert!(messages.contains(&(Error, "a.png".into())));
    assert!(messages.contains(&(Error, "../escape.png".into())));
    assert!(messages.contains(&(Error, "/absolute.png".into())));
    assert!(messages.contains(&(Warning, "data.xyz".into())));
    assert!(messages.contains(&(Error, "b.png".into())));
    assert!(messages.contains(&(Warning, "c.svg".into())));
    assert!(catalog.has_errors());
    assert!(catalog.get("@app/c.svg").unwrap().scales.is_empty());
    assert!(
        AssetCatalog::builder()
            .add_manifest_text("app", "files = 3")
            .is_err()
    );
    assert!(
        AssetCatalog::builder()
            .add_manifest_text("app", "unknown = []")
            .is_err()
    );
}

fn reader() -> MemoryAssetReader {
    MemoryAssetReader::new()
        .with("asset:/@app/assets.toml", APP_MANIFEST.as_bytes())
        .with("asset:/@shared/assets.toml", SHARED_MANIFEST.as_bytes())
        .with(
            "asset:/@app/locales/en.json",
            &br#"{ "hello": "Hello" }"#[..],
        )
        .with(
            "asset:/@shared/locales/de.json",
            "{ \"hallo\": \"Hallo\" }".as_bytes(),
        )
        .with("asset:/@app/images/logo.png", vec![0_u8; 10])
        .with("asset:/@app/images/hero.png", vec![1_u8; 10])
}

fn options(validation: AssetValidationPolicy) -> AssetOptions {
    let mut options = AssetOptions {
        validation,
        ..AssetOptions::default()
    };
    options.package("app").package("shared");
    options
}

#[test]
fn the_service_reads_manifests_and_assets_through_its_reader() {
    let service = AssetService::new(
        options(AssetValidationPolicy::Strict),
        Some(Arc::new(reader())),
    );
    assert_eq!(service.catalog().err(), Some(AssetError::NotLoaded));
    block_on(service.load_catalog()).unwrap();
    assert_eq!(
        service.uri("images/logo.png").unwrap(),
        "asset:/@app/images/logo.png"
    );
    assert_eq!(
        service.uri_for_scale("images/logo.png", 2.0).unwrap(),
        "asset:/@app/images/logo@2x.png"
    );
    let greeting: serde_json::Value = block_on(service.load_json("locales/en.json")).unwrap();
    assert_eq!(greeting["hello"], "Hello");
    assert_eq!(
        block_on(service.load_text("locales/de.json")).unwrap(),
        "{ \"hallo\": \"Hallo\" }"
    );
    assert!(matches!(
        block_on(service.load_bytes("fonts/Inter.woff2")),
        Err(AssetError::Read { .. })
    ));
    assert!(matches!(
        block_on(service.load_bytes("missing.png")),
        Err(AssetError::NotFound(_))
    ));
}

#[test]
fn loaded_bytes_stay_within_the_budget() {
    let mut options = options(AssetValidationPolicy::WarnOnly);
    options.byte_cache_budget = 15;
    let reader = reader();
    let service = AssetService::new(options, Some(Arc::new(reader.clone())));
    block_on(service.load_catalog()).unwrap();
    let first = block_on(service.load_bytes("images/logo.png")).unwrap();
    // Read from memory: the reader's copy changing does not reach it.
    reader.insert("asset:/@app/images/logo.png", vec![9_u8; 10]);
    assert_eq!(
        block_on(service.load_bytes("images/logo.png")).unwrap(),
        first
    );
    // A second file goes over the budget, so the first leaves.
    block_on(service.load_bytes("images/hero.png")).unwrap();
    assert_eq!(
        block_on(service.load_bytes("images/logo.png")).unwrap()[0],
        9
    );

    let mut cache = AssetByteCache::new(4);
    assert!(!cache.insert("big", vec![0_u8; 5].into()));
    assert!(cache.insert("a", vec![0_u8; 2].into()));
    assert!(cache.insert("b", vec![0_u8; 2].into()));
    cache.get("a");
    assert!(cache.insert("c", vec![0_u8; 2].into()));
    assert!(cache.contains("a") && !cache.contains("b") && cache.contains("c"));
    assert_eq!(cache.current_bytes(), 4);
}

#[test]
fn a_missing_manifest_fails_strict_validation_and_warns_otherwise() {
    let mut options = options(AssetValidationPolicy::Strict);
    options.package("missing");
    let strict = AssetService::new(options.clone(), Some(Arc::new(reader())));
    assert!(matches!(
        block_on(strict.load_catalog()),
        Err(AssetError::Read { .. })
    ));

    options.validation = AssetValidationPolicy::WarnOnly;
    let lenient = AssetService::new(options.clone(), Some(Arc::new(reader())));
    let catalog = block_on(lenient.load_catalog()).unwrap();
    assert_eq!(catalog.diagnostics().len(), 1);
    assert_eq!(catalog.diagnostics()[0].package, "missing");
    assert!(catalog.get("images/logo.png").is_some());

    options.validation = AssetValidationPolicy::Disabled;
    let quiet = AssetService::new(options, Some(Arc::new(reader())));
    assert!(
        block_on(quiet.load_catalog())
            .unwrap()
            .diagnostics()
            .is_empty()
    );

    let without_reader = AssetService::new(self::options(AssetValidationPolicy::Strict), None);
    assert!(block_on(without_reader.load_catalog()).is_err());
}

#[test]
fn add_assets_binds_options_and_loads_the_catalog_on_start() {
    let mut builder = RayXAppBuilder::new().add_assets(|options| {
        options.package("shared");
    });
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": ["app"], "byteCacheBudget": 1024 } } }
    }));
    builder
        .services
        .add_singleton_as::<dyn AssetReader>(Arc::new(reader()) as Arc<dyn AssetReader>);
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    let assets = services.required_service::<AssetService>().unwrap();
    assert_eq!(assets.options().packages, ["app", "shared"]);
    assert_eq!(assets.options().byte_cache_budget, 1024);
    assert_eq!(assets.catalog().unwrap().len(), 9);
}
