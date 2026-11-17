//! RayX's asset service on NSH: the app's files, listed by its packages' manifests (and the
//! files NSH lists) in the catalog RayX's implementation of
//! [`rayx_app::services::assets::AssetService`] publishes when the app starts, and read with
//! NSH's `fetch` of their `asset:/` URLs from any thread (through the surface's engine runner),
//! with a byte cache.
//!
//! - Rust: `RayXAppBuilder::new().add_assets(|options| { options.package("my-app"); })`
//!   ([`AssetAppBuilderExt`], with `add_asset_files(assets!())` and `add_bundled_assets(uris)`).
//! - TS: `new RayXAppBuilder().addAssets((options) => { options.package("my-app"); })`, with the
//!   class [`AssetService`] over whichever `dyn AssetService` is registered.
//! - The app's set: [`register`].

mod assets_ts;
mod builder;
mod bundled;
mod cache;
mod nsh_reader;
mod options;
mod options_ts;
mod reader;
mod service;

use std::sync::Arc;

use nsh_core::element::{Registry, TsAlias};
use rayx_app::StateCell;
use rayx_app::services::assets::*;
use rayx_app_nsh::ts::ContractRegistryExt as _;

pub use assets_ts::AssetService;
pub use builder::{AssetAppBuilderExt, AssetBuilder};
pub use bundled::BundledAssets;
pub use cache::AssetByteCache;
pub use options::{
    ASSET_OPTIONS_SECTION, AssetOptions, AssetValidationPolicy, DEFAULT_ASSET_BYTE_CACHE_BUDGET,
};
pub use options_ts::AssetOptions as TsAssetOptions;

rayx_app_nsh::contract_nsh!(rayx_app::services::assets::AssetService as AssetContract);

/// The asset catalog in TS.
pub const ASSET_CATALOG: TsAlias = TsAlias {
    name: "AssetCatalog",
    ts: "Record<string, unknown>",
    doc: "The app's asset catalog: the JSON of Rust's `AssetCatalog`.",
};

/// Adds the asset service's TS side to an app's set: its class [`AssetService`], its options
/// class, the contract a TS implementation follows (the TS interface `AssetServiceContract`,
/// [`AssetContract`] in Rust) and `builder.addAssets(..)`.
pub fn register(registry: &mut Registry) {
    registry
        .ts_alias(ASSET_CATALOG)
        .class::<TsAssetOptions>()
        .contract_class::<AssetContract, AssetService>()
        .class_extension::<AssetBuilder>();
}

/// RayX's asset service over readers of the test's own: for the crate's tests and an app's
/// (files held in memory, `include_bytes!`).
#[doc(hidden)]
pub mod testing {
    pub use crate::reader::{AssetReader, MemoryAssetReader};

    use super::*;

    /// RayX's asset service with `options`, reading through `reader`, with the files of
    /// `bundled` (when any); its catalog loads with `load_catalog`.
    pub fn assets(
        options: AssetOptions,
        reader: Arc<dyn AssetReader>,
        bundled: Option<Arc<BundledAssets>>,
    ) -> Arc<dyn rayx_app::services::assets::AssetService> {
        crate::service::RayxAssets::new(options, reader, bundled)
    }

    /// `add_assets(configure)`, reading the files through `reader` in place of NSH.
    pub fn add_assets_with<B: rayx_app::AppBuilder>(
        builder: B,
        configure: impl Fn(&mut AssetOptions) + Send + Sync + 'static,
        reader: Arc<dyn AssetReader>,
    ) -> B {
        crate::builder::register(builder, configure, Some(reader))
    }

    /// RayX's asset service over `catalog`, loaded already, reading through `reader`.
    pub fn with_catalog(
        catalog: AssetCatalog,
        reader: Arc<dyn AssetReader>,
    ) -> Arc<dyn rayx_app::services::assets::AssetService> {
        crate::service::RayxAssets::with_catalog(catalog, reader)
    }
}
