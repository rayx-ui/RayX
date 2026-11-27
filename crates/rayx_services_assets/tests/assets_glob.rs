//! `RayxAssets` with globbed manifests, virtual namespaces, exclusions, `when` predicates, and
//! `alias` over `MemoryAssetReader`'s served files.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::services::assets::*;
use rayx_services_assets::testing::{self, MemoryAssetReader};
use rayx_services_assets::{AssetOptions, AssetValidationPolicy};

#[test]
fn rayx_assets_expands_globbed_manifest_over_served_files() {
    let manifest = r#"
namespaces = ["illustrations/empty"]

[[asset]]
path = "icons/**/*.svg"
exclude = ["icons/draft/**"]
namespace = "icons/{subdir}"
kind = "icon"
tags = ["ui"]

[[asset]]
path = "icons/nav/home.svg"
title = "Home"
tags = ["primary"]

[[asset]]
path = "fonts/{sans,mono}/*.ttf"
namespace = "fonts/{subdir}"
kind = "font"

[[asset]]
path = "brand/acme/logo.svg"
kind = "illustration"
when = { brand = "acme", platform = ["macos", "windows"] }
alias = "brand/logo.svg"
"#;

    let reader = MemoryAssetReader::new()
        .serving("asset:/@app/assets.toml", manifest.as_bytes())
        .serving("asset:/@app/icons/close.svg", b"<svg/>".as_slice())
        .serving("asset:/@app/icons/nav/home.svg", b"<svg/>".as_slice())
        .serving("asset:/@app/icons/draft/tmp.svg", b"<svg/>".as_slice())
        .serving("asset:/@app/fonts/sans/Inter.ttf", b"ttf".as_slice())
        .serving("asset:/@app/fonts/mono/JB.ttf", b"ttf".as_slice())
        .serving("asset:/@app/brand/acme/logo.svg", b"<svg/>".as_slice())
        .serving("asset:/@app/data/unlisted.json", b"{}".as_slice());

    let options = AssetOptions {
        validation: AssetValidationPolicy::Strict,
        ..AssetOptions::default()
    };
    let service = testing::assets(options, Arc::new(reader), None);
    block_on(service.load_catalog()).unwrap();

    let catalog = service.loaded_catalog().unwrap();
    let catalog = catalog.as_ref().as_ref().unwrap();
    assert!(
        catalog.diagnostics().is_empty(),
        "{:?}",
        catalog.diagnostics()
    );

    // Glob + exact specificity merge.
    let home = service.descriptor("@app/icons/nav/home.svg").unwrap();
    assert_eq!(home.kind, AssetKind::Icon);
    assert_eq!(home.namespace.as_deref(), Some("icons/nav"));
    assert_eq!(home.title.as_deref(), Some("Home"));
    assert_eq!(home.tags, ["ui", "primary"]);

    // Root-level `{subdir}` collapse.
    let close = service.descriptor("icons/close.svg").unwrap();
    assert_eq!(close.namespace.as_deref(), Some("icons"));
    assert_eq!(close.tags, ["ui"]);

    // Excluded files stay excluded even though served by the reader.
    assert!(service.descriptor("@app/icons/draft/tmp.svg").is_err());

    // Brace glob expanded into `{subdir}` namespaces.
    assert_eq!(
        service
            .descriptor("fonts/sans/Inter.ttf")
            .unwrap()
            .namespace
            .as_deref(),
        Some("fonts/sans")
    );
    assert_eq!(
        service
            .descriptor("fonts/mono/JB.ttf")
            .unwrap()
            .namespace
            .as_deref(),
        Some("fonts/mono")
    );

    // Unlisted served file is still present quietly.
    let unlisted = service.descriptor("data/unlisted.json").unwrap();
    assert_eq!(unlisted.kind, AssetKind::Json);
    assert_eq!(unlisted.namespace, None);

    // Alias resolves through `descriptor` / `uri`.
    assert_eq!(
        service.uri("brand/logo.svg").unwrap(),
        "asset:/@app/brand/acme/logo.svg"
    );

    // Explicit empty namespace is preserved in catalog.
    assert!(catalog.namespaces().contains("illustrations/empty"));

    // Recursive vs exact namespace queries.
    let all_icons = service
        .query(&AssetQuery {
            namespace: Some("icons".to_owned()),
            namespace_recursive: true,
            ..AssetQuery::default()
        })
        .unwrap();
    assert_eq!(all_icons.len(), 2);

    let root_icons = service
        .query(&AssetQuery {
            namespace: Some("icons".to_owned()),
            namespace_recursive: false,
            ..AssetQuery::default()
        })
        .unwrap();
    assert_eq!(root_icons.len(), 1);
    assert_eq!(root_icons[0].path, "icons/close.svg");
}
