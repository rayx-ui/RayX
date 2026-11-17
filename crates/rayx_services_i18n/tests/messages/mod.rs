//! The app's messages for a test: an asset service listing them in its catalog, which RayX's
//! i18n service loads when the app starts.

#![allow(dead_code)]

use std::sync::Arc;

use rayx_app::services::assets::{AssetCatalog, AssetService};
use rayx_app::testing::FakeAssets;

/// An asset service whose catalog lists `files` (`(path, text)`, `locales/en.json`) in the
/// app's package.
#[allow(clippy::unwrap_used)]
pub fn assets(files: &[(&str, &str)]) -> Arc<dyn AssetService> {
    let listed: Vec<String> = files.iter().map(|(path, _)| format!("{path:?}")).collect();
    let mut catalog = AssetCatalog::builder();
    catalog
        .add_manifest_text("app", &format!("files = [{}]", listed.join(", ")))
        .unwrap();
    files
        .iter()
        .fold(FakeAssets::new(catalog.build()), |assets, (path, text)| {
            assets.file(&format!("@app/{path}"), *text)
        }) as Arc<dyn AssetService>
}

/// Registers `files` as the app's assets on `builder`.
pub fn add_messages<B: rayx_app::AppBuilder>(mut builder: B, files: &[(&str, &str)]) -> B {
    builder
        .services_mut()
        .add_singleton_as::<dyn AssetService>(assets(files));
    builder
}
