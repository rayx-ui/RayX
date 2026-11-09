//! The files packages ship, as the platform lists them: the asset catalog has them without a
//! manifest listing them, folds their density variants, and reads a package's manifest only
//! when the package ships one, for its metadata.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::assets::*;
use serde_json::json;

fn start(reader: MemoryAssetReader, bundled: &[&str], packages: &[&str]) -> Arc<AssetService> {
    let mut builder = RayXAppBuilder::new()
        .add_bundled_assets(bundled.iter().copied())
        .add_assets(|_| {});
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": packages } } }
    }));
    builder
        .services
        .add_singleton_as::<dyn AssetReader>(Arc::new(reader) as Arc<dyn AssetReader>);
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services.required_service::<AssetService>().unwrap()
}

#[test]
fn bundled_files_need_no_manifest() {
    let assets = start(
        MemoryAssetReader::new(),
        &[
            "asset:/@app/images/logo.png",
            "asset:/@app/images/logo@2x.png",
            "asset:/@app/locales/en.json",
            "asset:/@app/notes/LICENSE",
            "not an asset URL",
        ],
        &[],
    );
    let catalog = assets.catalog().unwrap();
    assert_eq!(catalog.packages(), ["app"]);
    let logo = assets.descriptor("images/logo.png").unwrap();
    assert_eq!(logo.kind, AssetKind::Image);
    assert_eq!(logo.scales, [1.0, 2.0]);
    assert_eq!(
        assets.descriptor("locales/en.json").unwrap().kind,
        AssetKind::Locale
    );
    assert!(assets.descriptor("images/logo@2x.png").is_err());
    // A file of no known kind is listed, without a warning: the platform ships it.
    assert_eq!(
        assets.descriptor("notes/LICENSE").unwrap().kind,
        AssetKind::Unknown
    );
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}

#[test]
fn a_shipped_manifest_adds_metadata_to_the_bundled_files() {
    let reader = MemoryAssetReader::new().with(
        "asset:/@app/assets.toml",
        r#"
[[asset]]
path = "images/logo.png"
tags = ["brand"]
"#
        .as_bytes(),
    );
    let assets = start(
        reader,
        &[
            "asset:/@app/assets.toml",
            "asset:/@app/images/logo.png",
            "asset:/@app/images/icon.svg",
        ],
        &[],
    );
    assert_eq!(
        assets.descriptor("images/logo.png").unwrap().tags,
        ["brand"]
    );
    assert_eq!(
        assets.descriptor("images/icon.svg").unwrap().kind,
        AssetKind::Svg
    );
    // The manifest is no asset.
    assert!(assets.descriptor("assets.toml").is_err());
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}

#[test]
fn the_options_packages_come_first() {
    let reader = MemoryAssetReader::new().with(
        "asset:/@app/assets.toml",
        r#"files = ["images/logo.png"]"#.as_bytes(),
    );
    let assets = start(
        reader,
        &[
            "asset:/@widgets/images/logo.png",
            "asset:/@widgets/images/close.png",
        ],
        &["app"],
    );
    assert_eq!(assets.catalog().unwrap().packages(), ["app", "widgets"]);
    // A path without a package is the first package's that has it.
    assert_eq!(assets.descriptor("images/logo.png").unwrap().package, "app");
    assert_eq!(
        assets.descriptor("images/close.png").unwrap().package,
        "widgets"
    );
}
