//! Translations of several packages: a message the app's own package and a library both have
//! comes from the package listed first in the asset options (the app's), whatever their names'
//! alphabetical order; a message only the library has still comes from it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_core::RayXAppBuilder;
use rayx_core::di::StartupInitializationContext;
use rayx_runtime::services::assets::{AssetReader, MemoryAssetReader};
use rayx_runtime::services::i18n::*;
use serde_json::json;

/// The app's package `app` and a library `widgets`, both translating `ok` (`app` sorts first).
fn reader() -> MemoryAssetReader {
    MemoryAssetReader::new()
        .with(
            "asset:/@app/assets.toml",
            r#"files = ["locales/en.json"]"#.as_bytes(),
        )
        .with(
            "asset:/@app/locales/en.json",
            r#"{ "ok": "Got it" }"#.as_bytes(),
        )
        .with(
            "asset:/@widgets/assets.toml",
            r#"files = ["locales/en.json"]"#.as_bytes(),
        )
        .with(
            "asset:/@widgets/locales/en.json",
            r#"{ "ok": "OK", "cancel": "Cancel" }"#.as_bytes(),
        )
}

fn start(packages: &[&str]) -> rayx_core::ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_i18n(|_| {});
    builder.configuration.add_value(json!({
        "RayX": { "Services": { "Assets": { "packages": packages } } }
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
    services
}

#[test]
fn the_apps_translations_override_its_libraries() {
    let services = start(&["app", "widgets"]);
    let i18n = services.required_service::<I18nService>().unwrap();
    assert_eq!(i18n.t("ok"), "Got it");
    assert_eq!(i18n.t("cancel"), "Cancel");
}

#[test]
fn the_package_listed_first_wins_whatever_its_name() {
    let services = start(&["widgets", "app"]);
    let i18n = services.required_service::<I18nService>().unwrap();
    assert_eq!(i18n.t("ok"), "OK");
}
