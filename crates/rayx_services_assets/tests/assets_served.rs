//! The files the app's UI sets ship (a library's set, served by the app's, ships its own): the
//! asset catalog has them without the app naming their package, reads a served package's manifest
//! only when the package ships one, for its metadata, and puts their packages after the asset
//! options' and the files the app added.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::RayXAppBuilder;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::assets::*;
use rayx_services_assets::AssetAppBuilderExt as _;
use rayx_services_assets::testing::{self, MemoryAssetReader};
use serde_json::json;

fn start(reader: MemoryAssetReader, bundled: &[&str], packages: &[&str]) -> Arc<dyn AssetService> {
    let mut builder = testing::add_assets_with(
        RayXAppBuilder::new().add_bundled_assets(bundled.iter().copied()),
        |_| {},
        Arc::new(reader),
    );
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": packages } } }
    }));
    let services = builder.build().unwrap();
    services.validate_options_on_start().unwrap();
    block_on(
        services
            .startup_registry()
            .initialize(StartupInitializationContext::new(services.clone())),
    )
    .unwrap();
    services.required_service::<dyn AssetService>().unwrap()
}

/// The packages of the service's catalog, once loaded.
fn packages(assets: &Arc<dyn AssetService>) -> Vec<String> {
    let catalog = assets.loaded_catalog().unwrap();
    catalog.as_ref().as_ref().unwrap().packages().to_vec()
}

#[test]
fn a_served_library_s_files_are_in_the_catalog_unnamed() {
    let reader = MemoryAssetReader::new()
        .serving("asset:/@acme-ui/icons/star.svg", b"<svg/>".as_slice())
        .serving("asset:/@acme-ui/icons/star.png", b"png".as_slice())
        .serving("asset:/@acme-ui/icons/star@2x.png", b"png2".as_slice())
        .serving("asset:/@acme-ui/fonts/Inter.ttf", b"ttf".as_slice());
    let assets = start(reader, &[], &[]);
    assert_eq!(packages(&assets), ["acme-ui"]);
    assert_eq!(
        assets.descriptor("@acme-ui/icons/star.svg").unwrap().kind,
        AssetKind::Icon
    );
    let png = assets.descriptor("icons/star.png").unwrap();
    assert_eq!(png.package, "acme-ui");
    assert_eq!(png.scales, [1.0, 2.0]);
    assert_eq!(
        assets.descriptor("fonts/Inter.ttf").unwrap().kind,
        AssetKind::Font
    );
    assert_eq!(
        assets.uri("icons/star.svg").unwrap(),
        "asset:/@acme-ui/icons/star.svg"
    );
    assert_eq!(
        block_on(assets.load_bytes("@acme-ui/icons/star.svg".into())).unwrap(),
        b"<svg/>"
    );
    // A served package that ships no manifest is no problem: the set lists its files.
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}

#[test]
fn a_served_package_s_manifest_adds_its_metadata() {
    let reader = MemoryAssetReader::new()
        .serving(
            "asset:/@acme-ui/assets.toml",
            r#"
[[asset]]
path = "icons/star.svg"
tags = ["brand"]
"#
            .as_bytes(),
        )
        .serving("asset:/@acme-ui/icons/star.svg", b"<svg/>".as_slice());
    let assets = start(reader, &[], &[]);
    assert_eq!(
        assets.descriptor("@acme-ui/icons/star.svg").unwrap().tags,
        ["brand"]
    );
    assert!(assets.descriptor("@acme-ui/assets.toml").is_err());
    assert!(
        assets.diagnostics().is_empty(),
        "{:?}",
        assets.diagnostics()
    );
}

#[test]
fn the_app_s_packages_come_before_the_served_ones() {
    let reader = MemoryAssetReader::new()
        .with(
            "asset:/@app/assets.toml",
            r#"files = ["icons/star.svg"]"#.as_bytes(),
        )
        .with("asset:/@app/icons/star.svg", b"app".as_slice())
        .serving("asset:/@acme-ui/icons/star.svg", b"acme".as_slice())
        .serving("asset:/@acme-ui/icons/bell.svg", b"bell".as_slice())
        // A file both added by the app and shipped by a set it serves is listed once.
        .serving("asset:/@shared/data/cities.json", b"[]".as_slice());
    let assets = start(
        reader,
        &[
            "asset:/@shared/data/cities.json",
            "asset:/@shared/data/rivers.json",
        ],
        &["app"],
    );
    // The options' packages, then the files the app added, then the served sets' files.
    assert_eq!(packages(&assets), ["app", "shared", "acme-ui"]);
    // A path without a package is the first package's that has it: the app's own.
    assert_eq!(assets.descriptor("icons/star.svg").unwrap().package, "app");
    assert_eq!(
        assets.descriptor("icons/bell.svg").unwrap().package,
        "acme-ui"
    );
    assert_eq!(
        block_on(assets.load_bytes("@acme-ui/icons/star.svg".into())).unwrap(),
        b"acme"
    );
    assert!(assets.descriptor("data/rivers.json").is_ok());
}
