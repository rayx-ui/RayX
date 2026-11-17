//! Translations of several packages: a message the app's own package and a library both have
//! comes from the package listed first in the asset options (the app's), whatever their names'
//! alphabetical order; a message only the library has still comes from it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use futures::executor::block_on;
use rayx_app::RayXAppBuilder;
use rayx_app::di::StartupInitializationContext;
use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::services::i18n::MessageFormatter;
use rayx_app::testing::{FakeAssets, add_host_fakes};
use rayx_services_i18n::I18nAppBuilderExt as _;

/// The app's package `app` and a library `widgets`, both translating `ok` (`app` sorts first),
/// listed in `packages`' order.
fn assets(packages: &[&str]) -> Arc<dyn AssetService> {
    let mut catalog = AssetCatalog::builder();
    for package in packages {
        catalog
            .add_manifest_text(package, r#"files = ["locales/en.json"]"#)
            .unwrap();
    }
    FakeAssets::new(catalog.build())
        .file("@app/locales/en.json", r#"{ "ok": "Got it" }"#)
        .file(
            "@widgets/locales/en.json",
            r#"{ "ok": "OK", "cancel": "Cancel" }"#,
        ) as Arc<dyn AssetService>
}

fn start(packages: &[&str]) -> rayx_app::ServiceProvider {
    let mut builder = RayXAppBuilder::new().add_i18n(|_| {});
    builder
        .services
        .add_singleton_as::<dyn AssetService>(assets(packages));
    let services = add_host_fakes(builder).build().unwrap();
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
    let i18n = services.required_service::<dyn MessageFormatter>().unwrap();
    assert_eq!(i18n.t("ok"), "Got it");
    assert_eq!(i18n.t("cancel"), "Cancel");
}

#[test]
fn the_package_listed_first_wins_whatever_its_name() {
    let services = start(&["widgets", "app"]);
    let i18n = services.required_service::<dyn MessageFormatter>().unwrap();
    assert_eq!(i18n.t("ok"), "OK");
}
